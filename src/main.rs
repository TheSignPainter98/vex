#![deny(missing_debug_implementations)]

#[cfg(test)]
#[macro_use]
extern crate pretty_assertions;

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

#[cfg(test)]
mod vextest;

use std::{env, fs, process::ExitCode};

use camino::{Utf8Path, Utf8PathBuf};
use clap::Parser as _;
use dupe::Dupe;
use log::{info, log_enabled, trace};
use scriptlets::PrettyPath;
use strum::IntoEnumIterator;
use tree_sitter::QueryCursor;

use crate::{
    cli::{Args, CheckCmd, Command},
    context::{CompiledFilePattern, Context},
    irritation::Irritation,
    scriptlets::{
        event::{CloseFileEvent, MatchEvent, OpenFileEvent, OpenProjectEvent},
        Observer, PreinitingStore, VexingStore,
    },
    source_file::SourceFile,
    supported_language::SupportedLanguage,
    verbosity::Verbosity,
};

fn main() -> anyhow::Result<ExitCode> {
    let args = Args::parse();
    logger::init(Verbosity::try_from(args.verbosity_level)?)?;

    match args.command.unwrap_or_default() {
        Command::ListLanguages => list_languages(),
        Command::ListLints => list_lints(),
        Command::Check(cmd_args) => check(cmd_args),
        Command::Init => init(),
    }?;

    Ok(logger::report())
}

fn list_languages() -> anyhow::Result<()> {
    SupportedLanguage::iter().for_each(|lang| println!("{}", lang));
    Ok(())
}

fn list_lints() -> anyhow::Result<()> {
    let ctx = Context::acquire()?;
    let store = PreinitingStore::new(&ctx)?.preinit()?;
    store.vexes().for_each(|vex| println!("{}", vex.path));
    Ok(())
}

fn check(_cmd_args: CheckCmd) -> anyhow::Result<()> {
    let ctx = Context::acquire()?;
    let store = PreinitingStore::new(&ctx)?.preinit()?.init()?;

    let irritations = vex(&ctx, &store)?;
    println!("Got irritations: {irritations:?}");

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

fn vex(ctx: &Context, store: &VexingStore) -> anyhow::Result<Vec<Irritation>> {
    let language_observers = store.language_observers();
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
            ctx,
            ctx.project_root.as_ref(),
            &ignores,
            &allows,
            &mut paths,
        )?;
        paths
            .into_iter()
            .map(|p| PrettyPath::new(&p))
            .collect::<Vec<_>>()
    };

    for language_observer in language_observers.values() {
        for observer in language_observer {
            for on_open_project in &observer.on_open_project[..] {
                on_open_project.handle(OpenProjectEvent::new(ctx.project_root.dupe()))?;
            }
        }
    }
    // let mut irritations = Vec::new();
    for path in paths {
        let Some(src_file) = SourceFile::load_if_supported(path.dupe()) else {
            continue;
        };
        let src_file = src_file?;

        println!("linting {}...", src_file.path);
        for observers in &language_observers[src_file.lang] {
            for on_open_file in &observers.on_open_file[..] {
                on_open_file.handle(OpenFileEvent::new(path.dupe()))?;
            }

            println!("running a handler set...");
            for qmatch in QueryCursor::new().matches(
                observers.query.as_ref(),
                src_file.tree.root_node(),
                src_file.content[..].as_bytes(),
            ) {
                println!("found {qmatch:?}");
                for on_match in observers.on_match.iter() {
                    on_match.handle(MatchEvent::new(path.dupe()))?;
                }
            }

            for on_close_file in observers.on_close_file.iter() {
                on_close_file.handle(CloseFileEvent::new(path.dupe()))?;
            }
        }

        // let mut vex_irritations = Vec::new();
        // for vex in vexes {
        //     vex_irritations.extend(vex.check(&src_file)?);
        // }
        // irritations.push((vex.id, vex_irritations));
    }
    for language_observer in language_observers.values() {
        for observer in language_observer {
            for on_open_project in &observer.on_open_project[..] {
                on_open_project.handle(OpenProjectEvent::new(ctx.project_root.dupe()))?;
            }
        }
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

    Ok(vec![])
}

fn walkdir(
    ctx: &Context,
    path: &Utf8Path,
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
                    let ignore_path = entry_path.strip_prefix(ctx.project_root.as_ref())?;
                    let dir_marker = if metadata.is_dir() { "/" } else { "" };
                    info!("ignoring /{ignore_path}{dir_marker}");
                }
                continue;
            }
        }

        if metadata.is_symlink() {
            if log_enabled!(log::Level::Info) {
                let symlink_path = entry_path.strip_prefix(ctx.project_root.as_ref())?;
                info!("ignoring /{symlink_path} (symlink)");
            }
        } else if metadata.is_dir() {
            walkdir(ctx, &entry_path, ignores, allows, paths)?;
        } else if metadata.is_file() {
            paths.push(entry_path);
        } else {
            panic!("unreachable");
        }
    }

    Ok(())
}

fn init() -> anyhow::Result<()> {
    let cwd = Utf8PathBuf::try_from(env::current_dir()?)?;
    Context::init(cwd)
}
