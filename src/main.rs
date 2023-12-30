mod cli;
mod error;
mod manifest;
mod source_file;
mod supported_language;
mod vexes;

use std::{env, sync::Arc};

use async_recursion::async_recursion;
use camino::Utf8PathBuf;
use clap::Parser as _;
use cli::{CheckCmd, IgnoreCmd, IgnoreKind};
use error::Error;
use manifest::Manifest;
use strum::IntoEnumIterator;
use supported_language::SupportedLanguage;
use tokio::{
    fs::{self},
    sync::{mpsc, Semaphore},
    task::JoinSet,
};

use crate::{
    cli::{Args, Command},
    manifest::CompiledFilePattern,
    vexes::Vexes,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    match args.command.unwrap_or_default() {
        Command::ListLanguages => list_languages(),
        Command::ListLints => list_lints().await,
        Command::Check(cmd_args) => check(cmd_args).await,
        Command::Ignore(cmd_args) => ignore(cmd_args),
        Command::Init => init(),
    }
}

fn list_languages() -> anyhow::Result<()> {
    SupportedLanguage::iter().for_each(|lang| println!("{}", lang.name()));
    Ok(())
}

async fn list_lints() -> anyhow::Result<()> {
    let manifest = Manifest::acquire()?;
    let vexes = Vexes::new(&manifest);
    vexes.vexes().await?.iter().for_each(|(lang, set)| {
        println!("{}:", lang.name());
        set.iter().for_each(|vex| println!("\t{}", vex.name));
    });
    Ok(())
}

async fn check(cmd_args: CheckCmd) -> anyhow::Result<()> {
    let manifest = Arc::new(Manifest::acquire()?);
    let vexes = Vexes::new(&manifest);

    #[async_recursion]
    async fn walkdir(
        manifest: Arc<Manifest>,
        path: Utf8PathBuf,
        ignores: Arc<Vec<CompiledFilePattern>>,
        allows: Arc<Vec<CompiledFilePattern>>,
        concurrency_limiter: Arc<Semaphore>,
        tx: mpsc::Sender<Utf8PathBuf>,
    ) -> anyhow::Result<()> {
        let mut dir = fs::read_dir(path).await?;
        let mut child_paths: Vec<Utf8PathBuf> = Vec::new();
        while let Some(entry) = dir.next_entry().await? {
            let _permit = concurrency_limiter.acquire().await?;

            let entry_path = Utf8PathBuf::try_from(entry.path())?;
            if !allows.iter().any(|p| p.matches_path(&entry_path)) {
                let hidden = entry_path
                    .file_name()
                    .is_some_and(|name| name.starts_with('.'));
                if hidden || ignores.iter().any(|p| p.matches_path(&entry_path)) {
                    continue;
                }
            }

            let metadata = fs::symlink_metadata(&entry_path).await?;

            if metadata.is_symlink() {
                eprintln!("symlinks are not supported: ignoring {entry_path}");
            } else if metadata.is_dir() {
                child_paths.push(entry_path);
            } else if metadata.is_file() {
                tx.send(entry_path).await?;
            } else {
                panic!("unreachable");
            }
        }

        for child_path in child_paths {
            let tx = tx.clone();
            let concurrency_limiter = concurrency_limiter.clone();
            let manifest = manifest.clone();
            let ignores = ignores.clone();
            let allows = allows.clone();
            tokio::spawn(async move {
                walkdir(
                    manifest,
                    child_path,
                    ignores,
                    allows,
                    concurrency_limiter,
                    tx,
                )
            });
        }

        Ok(())
    }
    let (tx, mut rx) = mpsc::channel(1024);
    tokio::spawn({
        let manifest2 = manifest.clone();
        let root = manifest.project_root.clone();
        let concurrency_limiter = Arc::new(Semaphore::new(cmd_args.max_concurrent_files.0));
        let ignores = Arc::new(
            manifest
                .ignores
                .clone()
                .unwrap_or_default()
                .0
                .into_iter()
                .map(|ignore| ignore.compile(&manifest.project_root))
                .collect::<anyhow::Result<Vec<_>>>()?,
        );
        let allows = Arc::new(
            manifest
                .allows
                .clone()
                .unwrap_or_default()
                .into_iter()
                .map(|allow| allow.compile(&manifest.project_root))
                .collect::<anyhow::Result<Vec<_>>>()?,
        );
        async move { walkdir(manifest2, root, ignores, allows, concurrency_limiter, tx).await }
    });
    // TODO(kcza): how are errors propagated here?

    let mut npaths = 0;
    let mut set = JoinSet::new();
    while let Some(path) = rx.recv().await {
        npaths += 1;
        let vexes = vexes.clone();
        set.spawn(async move { Ok::<_, anyhow::Error>((path.clone(), vexes.check(path).await?)) });
    }

    let mut problems = Vec::new();
    while let Some(res) = set.join_next().await {
        problems.push(res??)
    }

    problems.sort();
    for (_, file_problems) in problems {
        for problem in file_problems {
            println!("{problem}");
        }
    }
    println!("scanned {npaths} paths");

    // let src = indoc::indoc! {r#"
    //     fn main() {
    //         println!("hello, world!");
    //         sqlx::query("SELECT * FROM foo")?;
    //     }
    // "#};
    // let mut parser = Parser::new();
    // parser
    //     .set_language(tree_sitter_rust::language())
    //     .expect("failed to load Rust grammar");
    // let tree = parser.parse(src, None).unwrap(); // TODO(kcza): make this a result!
    //
    // let query_src = indoc::indoc! {r#"
    //     (call_expression
    //         function: (scoped_identifier
    //             path: (identifier) @_pkg (#eq? @_pkg "sqlx")
    //             name: (identifier) @_func (#eq? @_func "query"))
    //         arguments: (arguments
    //             (string_literal) @sql (#offset! @sql 1 0 -1 0)))
    //     "#};
    // let query = Query::new(tree_sitter_rust::language(), query_src)?;
    // let mut query_cursor = QueryCursor::new();
    // println!("=====");
    // for m in query_cursor.matches(&query, tree.root_node(), src.as_bytes()) {
    //     println!("match: {}, {:?}: {m:?}", m.pattern_index, m.captures);
    // }
    // println!("=====");
    //
    // println!("{}", tree.root_node().to_sexp());

    Ok(())
}

fn ignore(cmd_args: IgnoreCmd) -> anyhow::Result<()> {
    if cmd_args.kind == IgnoreKind::Language {
        let unknown_languages: Vec<_> = cmd_args
            .to_ignore
            .iter()
            .filter(|lang_name| {
                SupportedLanguage::iter().all(|sup_lang| sup_lang.name() != *lang_name)
            })
            .map(ToOwned::to_owned)
            .collect();
        if !unknown_languages.is_empty() {
            return Err(Error::UnknownLanguages(unknown_languages).into());
        }
    }
    Manifest::ignore(cmd_args.kind, cmd_args.to_ignore)
}

fn init() -> anyhow::Result<()> {
    let cwd = Utf8PathBuf::try_from(env::current_dir()?)?;
    Manifest::init(cwd)
}
