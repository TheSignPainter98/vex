#![deny(missing_debug_implementations)]

mod cli;
mod context;
mod error;
mod logger;
mod plural;
mod source_file;
mod supported_language;
mod verbosity;
mod vexes;

use std::{env, sync::Arc};

use async_recursion::async_recursion;
use camino::Utf8PathBuf;
use clap::Parser as _;
use cli::{CheckCmd, IgnoreCmd, IgnoreKind};
use context::Context;
use error::Error;
use log::{info, log_enabled, trace, warn};
use strum::IntoEnumIterator;
use supported_language::SupportedLanguage;
use tokio::{
    fs,
    sync::{mpsc, Semaphore},
    task::JoinSet,
};

use crate::{
    cli::{Args, Command, MaxProblems},
    context::CompiledFilePattern,
    plural::Plural,
    verbosity::Verbosity,
    vexes::Vexes,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    logger::init(Verbosity::try_from(args.verbosity_level)?)?;

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
    let manifest = Context::acquire()?;
    let vexes = Vexes::new(&manifest);
    vexes.vexes().await?.iter().for_each(|(lang, set)| {
        println!("{}:", lang.name());
        set.iter().for_each(|vex| println!("\t{}", vex.name));
    });
    Ok(())
}

async fn check(cmd_args: CheckCmd) -> anyhow::Result<()> {
    let context = Arc::new(Context::acquire()?);
    let vexes = Vexes::new(&context);

    #[async_recursion]
    async fn walkdir(
        manifest: Arc<Context>,
        path: Utf8PathBuf,
        ignores: Arc<Vec<CompiledFilePattern>>,
        allows: Arc<Vec<CompiledFilePattern>>,
        concurrency_limiter: Arc<Semaphore>,
        tx: mpsc::Sender<Utf8PathBuf>,
    ) -> anyhow::Result<()> {
        if log_enabled!(log::Level::Trace) {
            trace!("walking {path}");
        }
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
                if log_enabled!(log::Level::Info) {
                    info!("symlinks are not yet supported, ignoring {entry_path}");
                }
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
                .await
            })
            .await??;
        }

        Ok(())
    }
    let (path_tx, mut path_rx) = mpsc::channel(1024);
    let walk_handle = tokio::spawn({
        let context = context.clone();
        let root = context.project_root.clone();
        let concurrency_limiter =
            Arc::new(Semaphore::new(cmd_args.max_concurrent_files.0 as usize));
        let ignores = Arc::new(
            context
                .ignores
                .clone()
                .unwrap_or_default()
                .0
                .into_iter()
                .map(|ignore| ignore.compile(&context.project_root))
                .collect::<anyhow::Result<Vec<_>>>()?,
        );
        let allows = Arc::new(
            context
                .allows
                .clone()
                .unwrap_or_default()
                .into_iter()
                .map(|allow| allow.compile(&context.project_root))
                .collect::<anyhow::Result<Vec<_>>>()?,
        );
        async move { walkdir(context, root, ignores, allows, concurrency_limiter, path_tx).await }
    });

    let mut npaths = 0;
    let max_problem_channel_size = match cmd_args.max_problems {
        MaxProblems::Limited(lim) => lim as usize,
        MaxProblems::Unlimited => 1000, // Large limit but still capped.
    };
    let mut set = JoinSet::new();
    while let Some(path) = path_rx.recv().await {
        npaths += 1;
        let vexes = vexes.clone();
        let path = path.clone();
        set.spawn(async move { vexes.check(path).await });
    }

    let mut problems = Vec::with_capacity(max_problem_channel_size);
    while let Some(res) = set.join_next().await {
        problems.extend(res??);

        if cmd_args.max_problems.is_exceeded_by(problems.len()) {
            break;
        }
    }
    walk_handle.await??;

    if let MaxProblems::Limited(max_problems) = cmd_args.max_problems {
        problems.truncate(max_problems as usize);
    }
    problems.sort();
    for problem in &problems {
        if log_enabled!(log::Level::Warn) {
            warn!(target: "vex", custom = true; "{problem}");
        }
    }
    if log_enabled!(log::Level::Info) {
        info!(
            "scanned {} and found {}",
            Plural::new(npaths, "path", "paths"),
            Plural::new(problems.len(), "problem", "problems"),
        );
    }

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
    Context::ignore(cmd_args.kind, cmd_args.to_ignore)
}

fn init() -> anyhow::Result<()> {
    let cwd = Utf8PathBuf::try_from(env::current_dir()?)?;
    Context::init(cwd)
}
