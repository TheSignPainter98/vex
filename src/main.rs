#![deny(missing_debug_implementations)]

mod cli;
mod context;
mod error;
mod irritation;
mod logger;
mod plural;
mod scriptlets;
mod source_file;
mod supported_language;
mod verbosity;
mod vex;
mod vex_store;

use std::{env, fs, process::ExitCode};

use camino::Utf8PathBuf;
use clap::Parser as _;
use log::{info, log_enabled, trace};
use strum::IntoEnumIterator;

use crate::{
    cli::{Args, Command},
    cli::{CheckCmd, IgnoreCmd, IgnoreKind},
    context::CompiledFilePattern,
    context::{Context, Manifest},
    error::Error,
    scriptlets::PreinitingStore,
    source_file::SourceFile,
    supported_language::SupportedLanguage,
    verbosity::Verbosity,
};

#[tokio::main]
async fn main() -> anyhow::Result<ExitCode> {
    let args = Args::parse();
    logger::init(Verbosity::try_from(args.verbosity_level)?)?;

    match args.command.unwrap_or_default() {
        Command::ListLanguages => list_languages(),
        Command::ListLints => list_lints().await,
        Command::Check(cmd_args) => check(cmd_args),
        Command::Ignore(cmd_args) => ignore(cmd_args),
        Command::Init => init(),
    }?;

    Ok(logger::report())
}

fn list_languages() -> anyhow::Result<()> {
    SupportedLanguage::iter().for_each(|lang| println!("{}", lang));
    Ok(())
}

async fn list_lints() -> anyhow::Result<()> {
    let ctx = Context::acquire()?;
    let store = PreinitingStore::new(&ctx)?.preinit()?;
    store.vexes().for_each(|vex| println!("{}", vex.path));
    Ok(())
}

fn check(_cmd_args: CheckCmd) -> anyhow::Result<()> {
    let ctx = Context::acquire()?;
    let _store = PreinitingStore::new(&ctx)?.preinit()?.init()?;

    let paths = {
        let mut paths = Vec::new();
        let ignores = ctx
            .ignores
            .clone()
            .unwrap_or_default()
            .0
            .into_iter()
            .map(|ignore| ignore.compile(&ctx.project_root))
            .collect::<anyhow::Result<Vec<_>>>()?;
        let allows = ctx
            .allows
            .clone()
            .unwrap_or_default()
            .into_iter()
            .map(|allow| allow.compile(&ctx.project_root))
            .collect::<anyhow::Result<Vec<_>>>()?;
        walkdir(
            &ctx,
            ctx.project_root.clone(),
            &ignores,
            &allows,
            &mut paths,
        )?;
        paths
    };

    // let mut irritations = Vec::new();
    for path in paths {
        let Some(src_file) = SourceFile::load_if_supported(path) else {
            continue;
        };
        let src_file = src_file?;
        println!("linting {}...", src_file.path);
        // let Some(_vexes) = vex_store.get(src_file.lang) else {
        //     continue;
        // };

        // let mut vex_irritations = Vec::new();
        // for vex in vexes {
        //     vex_irritations.extend(vex.check(&src_file)?);
        // }
        // irritations.push((vex.id, vex_irritations));
    }

    // let max_problem_channel_size = match cmd_args.max_problems {
    //     MaxProblems::Limited(lim) => lim as usize,
    //     MaxProblems::Unlimited => 1000, // Large limit but still capped.
    // };
    // let npaths = paths.len();
    // let mut set = JoinSet::new();
    // for path in paths {
    //     let vexes = vexes.clone();
    //     let path = path.clone();
    //     let Some(src_file_result) = SourceFile::maybe_load(path).await else {
    //         continue;
    //     };
    //     set.spawn(async move { vexes.check(src_file_result?).await });
    // }
    //
    // let mut problems = Vec::with_capacity(max_problem_channel_size);
    // while let Some(res) = set.join_next().await {
    //     problems.extend(res??);
    //
    //     if cmd_args.max_problems.is_exceeded_by(problems.len()) {
    //         break;
    //     }
    // }
    //
    // if let MaxProblems::Limited(max_problems) = cmd_args.max_problems {
    //     problems.truncate(max_problems as usize);
    // }
    // problems.sort();
    // for problem in &problems {
    //     if log_enabled!(log::Level::Warn) {
    //         warn!(target: "vex", custom = true; "{problem}");
    //     }
    // }
    // if log_enabled!(log::Level::Info) {
    //     info!(
    //         "scanned {} and found {}",
    //         Plural::new(npaths, "path", "paths"),
    //         Plural::new(problems.len(), "problem", "problems"),
    //     );
    // }

    Ok(())
}

fn walkdir(
    ctx: &Context,
    path: Utf8PathBuf,
    ignores: &[CompiledFilePattern],
    allows: &[CompiledFilePattern],
    paths: &mut Vec<Utf8PathBuf>,
) -> anyhow::Result<()> {
    if log_enabled!(log::Level::Trace) {
        trace!("walking {path}");
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let entry_path = Utf8PathBuf::try_from(entry.path())?;
        let metadata = fs::symlink_metadata(&entry_path)?;
        if !allows.iter().any(|p| p.matches_path(&entry_path)) {
            let hidden = entry_path
                .file_name()
                .is_some_and(|name| name.starts_with('.'));
            if hidden || ignores.iter().any(|p| p.matches_path(&entry_path)) {
                if log_enabled!(log::Level::Info) {
                    let ignore_path = entry_path.strip_prefix(&ctx.project_root)?;
                    let dir_marker = if metadata.is_dir() { "/" } else { "" };
                    info!("ignoring /{ignore_path}{dir_marker}");
                }
                continue;
            }
        }

        if metadata.is_symlink() {
            if log_enabled!(log::Level::Info) {
                let symlink_path = entry_path.strip_prefix(&ctx.project_root)?;
                info!("ignoring /{symlink_path} (symlink)");
            }
        } else if metadata.is_dir() {
            walkdir(ctx, entry_path, ignores, allows, paths)?;
        } else if metadata.is_file() {
            paths.push(entry_path);
        } else {
            panic!("unreachable");
        }
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
        if let Some(unknown) = unknown_languages.first() {
            return Err(Error::UnknownLanguage(unknown.to_string()).into());
        }
    }
    Manifest::ignore(cmd_args.kind, cmd_args.to_ignore)
}

fn init() -> anyhow::Result<()> {
    let cwd = Utf8PathBuf::try_from(env::current_dir()?)?;
    Manifest::init(cwd)
}
