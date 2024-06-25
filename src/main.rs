#![deny(missing_debug_implementations)]

#[cfg(test)]
#[macro_use]
extern crate pretty_assertions;

mod associations;
mod cli;
mod context;
mod error;
mod ignore_markers;
mod irritation;
mod logger;
mod parse;
mod plural;
mod query;
mod result;
mod scriptlets;
mod source_file;
mod source_path;
mod suggestion;
mod supported_language;
mod trigger;
mod verbosity;
mod vex;

#[cfg(test)]
mod vextest;

use std::{env, fs, process::ExitCode};

use camino::{Utf8Path, Utf8PathBuf};
use cli::{InitCmd, ListCmd, MaxProblems, ToList};
use dupe::Dupe;
use indoc::printdoc;
use lazy_static::lazy_static;
use log::{info, log_enabled, trace, warn};
use owo_colors::{OwoColorize, Stream, Style};
use scriptlets::{
    action::Action, event::EventKind, handler_module::HandlerModule, Observable, ObserveOptions,
};
use source_file::SourceFile;
use strum::IntoEnumIterator;
use tree_sitter::QueryCursor;

use crate::{
    cli::{Args, CheckCmd, Command},
    context::{Context, EXAMPLE_VEX_FILE},
    error::{Error, IOAction},
    irritation::Irritation,
    plural::Plural,
    result::Result,
    scriptlets::{
        event::{MatchEvent, OpenFileEvent, OpenProjectEvent},
        intents::Intent,
        query_cache::QueryCache,
        query_captures::QueryCaptures,
        PreinitOptions, PreinitingStore, VexingStore,
    },
    source_path::{PrettyPath, SourcePath},
    supported_language::SupportedLanguage,
    trigger::FilePattern,
    verbosity::Verbosity,
};

// TODO(kcza): move the subcommands to separate files
fn main() -> ExitCode {
    match run() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<ExitCode> {
    let args = Args::parse();
    logger::init(Verbosity::try_from(args.verbosity_level)?)?;

    match args.command {
        Command::Check(cmd_args) => check(cmd_args),
        Command::List(list_args) => list(list_args),
        Command::Init(init_args) => init(init_args),
        Command::Parse(parse_args) => parse::parse(parse_args),
    }?;

    Ok(logger::exit_code())
}

fn list(list_args: ListCmd) -> Result<()> {
    match list_args.what {
        ToList::Checks => {
            let ctx = Context::acquire()?;
            let store = PreinitingStore::new(&ctx)?.preinit(PreinitOptions::default())?;
            store
                .vexes()
                .map(|vex| &vex.vex_id)
                .for_each(|id| println!("{}", id));
        }
        ToList::Languages => SupportedLanguage::iter().for_each(|lang| println!("{}", lang)),
    }
    Ok(())
}

lazy_static! {
    static ref SUCCESS_STYLE: Style = Style::new().green().bold();
}

fn check(cmd_args: CheckCmd) -> Result<()> {
    let ctx = Context::acquire()?;
    let store = {
        let preinit_opts = PreinitOptions {
            lenient: cmd_args.lenient,
        };
        PreinitingStore::new(&ctx)?.preinit(preinit_opts)?.init()?
    };

    let RunData {
        irritations,
        num_files_scanned,
    } = vex(&ctx, &store, cmd_args.max_problems)?;
    irritations.iter().for_each(|irr| println!("{irr}"));
    if log_enabled!(log::Level::Info) {
        info!(
            "scanned {}",
            Plural::new(num_files_scanned, "file", "files"),
        );
    }
    if !irritations.is_empty() {
        warn!(
            "found {}",
            Plural::new(irritations.len(), "problem", "problems"),
        );
    } else {
        println!(
            "{}: no problems found",
            "success".if_supports_color(Stream::Stdout, |text| text.style(*SUCCESS_STYLE))
        );
    }

    Ok(())
}

#[derive(Debug)]
struct RunData {
    irritations: Vec<Irritation>,
    num_files_scanned: usize,
}

impl RunData {
    #[cfg(test)]
    fn into_irritations(self) -> Vec<Irritation> {
        self.irritations
    }
}

fn vex(ctx: &Context, store: &VexingStore, max_problems: MaxProblems) -> Result<RunData> {
    let files = {
        let mut paths = Vec::new();
        let ignores = ctx
            .ignores
            .clone()
            .into_inner()
            .into_iter()
            .map(|ignore| ignore.compile())
            .collect::<Result<Vec<_>>>()?;
        let allows = ctx
            .allows
            .clone()
            .into_iter()
            .map(|allow| allow.compile())
            .collect::<Result<Vec<_>>>()?;
        walkdir(
            ctx,
            ctx.project_root.as_ref(),
            &ignores,
            &allows,
            &mut paths,
        )?;

        let associations = ctx.associations()?;
        paths
            .into_iter()
            .map(|p| SourcePath::new(&p, &ctx.project_root))
            .map(|p| {
                let language = associations.get_language(&p)?;
                Ok(SourceFile::new(p, language))
            })
            .collect::<Result<Vec<_>>>()?
    };

    let project_queries_hint = store.project_queries_hint();
    let file_queries_hint = store.file_queries_hint();

    let query_cache = QueryCache::with_capacity(project_queries_hint + file_queries_hint);

    let mut irritations = vec![];
    let frozen_heap = store.frozen_heap();
    let project_queries = {
        let mut project_queries = Vec::with_capacity(project_queries_hint);

        let event = OpenProjectEvent::new(ctx.project_root.dupe());
        let handler_module = HandlerModule::new();
        let observe_opts = ObserveOptions {
            action: Action::Vexing(event.kind()),
            query_cache: &query_cache,
            ignore_markers: None,
        };
        store.observers_for(event.kind()).observe(
            &handler_module,
            handler_module.heap().alloc(event),
            observe_opts,
        )?;
        handler_module
            .into_intents_on(frozen_heap)?
            .into_iter()
            .for_each(|intent| match intent {
                Intent::Find {
                    language,
                    query,
                    on_match,
                } => project_queries.push((language, query, on_match)),
                Intent::Observe { .. } => panic!("internal error: non-init observe"),
                Intent::Warn(irr) => irritations.push(irr),
            });
        project_queries
    };

    for file in &files {
        let Some(language) = file.language() else {
            if log_enabled!(log::Level::Info) {
                info!("skipping {}", file.path());
            }
            continue;
        };

        let file_queries = {
            let mut file_queries = Vec::with_capacity(store.file_queries_hint());
            let path = file.path().pretty_path.dupe();

            let event = OpenFileEvent::new(path);
            let handler_module = HandlerModule::new();
            let observe_opts = ObserveOptions {
                action: Action::Vexing(event.kind()),
                query_cache: &query_cache,
                ignore_markers: None,
            };
            store.observers_for(event.kind()).observe(
                &handler_module,
                handler_module.heap().alloc(event),
                observe_opts,
            )?;
            handler_module
                .into_intents_on(frozen_heap)?
                .into_iter()
                .for_each(|intent| match intent {
                    Intent::Find {
                        language,
                        query,
                        on_match,
                    } => file_queries.push((language, query, on_match)),
                    Intent::Observe { .. } => panic!("internal error: non-init observe"),
                    Intent::Warn(irr) => irritations.push(irr.clone()),
                });
            file_queries
        };

        if project_queries
            .iter()
            .chain(file_queries.iter())
            .all(|(l, _, _)| *l != language)
        {
            continue; // No need to parse, the user will never search this.
        }
        let parsed_file = file.parse()?;
        let ignore_markers = parsed_file.ignore_markers()?;
        project_queries
            .iter()
            .chain(file_queries.iter())
            .filter(|(l, _, _)| *l == language)
            .try_for_each(|(_, query, on_match)| {
                QueryCursor::new()
                    .matches(
                        query,
                        parsed_file.tree.root_node(),
                        parsed_file.content.as_bytes(),
                    )
                    .try_for_each(|qmatch| {
                        let handler_module = HandlerModule::new();
                        let event = {
                            let path = parsed_file.path.pretty_path.dupe();
                            let captures = QueryCaptures::new(
                                query,
                                qmatch,
                                &parsed_file,
                                handler_module.heap(),
                            );
                            handler_module.heap().alloc(MatchEvent::new(path, captures))
                        };
                        let observe_opts = ObserveOptions {
                            action: Action::Vexing(EventKind::Match),
                            query_cache: &query_cache,
                            ignore_markers: Some(&ignore_markers),
                        };
                        on_match.observe(&handler_module, event, observe_opts)?;
                        handler_module
                            .into_intents_on(frozen_heap)?
                            .into_iter()
                            .for_each(|intent| match intent {
                                Intent::Find { .. } => {
                                    panic!("internal error: find intended during find")
                                }
                                Intent::Observe { .. } => {
                                    panic!("internal error: non-init observe")
                                }
                                Intent::Warn(irr) => irritations.push(irr),
                            });

                        Ok::<_, Error>(())
                    })
            })?;
    }

    irritations.sort();
    if let MaxProblems::Limited(max) = max_problems {
        let max = max as usize;
        if max < irritations.len() {
            irritations.truncate(max);
        }
    }
    Ok(RunData {
        irritations,
        num_files_scanned: files.len(),
    })
}

fn walkdir(
    ctx: &Context,
    path: &Utf8Path,
    ignores: &[FilePattern],
    allows: &[FilePattern],
    paths: &mut Vec<Utf8PathBuf>,
) -> Result<()> {
    if log_enabled!(log::Level::Trace) {
        trace!("walking {path}");
    }
    let entries = fs::read_dir(path).map_err(|cause| Error::IO {
        path: PrettyPath::new(path),
        action: IOAction::Read,
        cause,
    })?;
    for entry in entries {
        let entry = entry.map_err(|cause| Error::IO {
            path: PrettyPath::new(path),
            action: IOAction::Read,
            cause,
        })?;
        let entry_path = Utf8PathBuf::try_from(entry.path())?;
        let metadata = fs::symlink_metadata(&entry_path).map_err(|cause| Error::IO {
            path: PrettyPath::new(&entry_path),
            action: IOAction::Read,
            cause,
        })?;
        let is_dir = metadata.is_dir();

        let project_relative_path =
            Utf8Path::new(&entry_path.as_str()[ctx.project_root.as_str().len()..]);
        if !allows.iter().any(|p| p.matches(project_relative_path)) {
            let hidden = project_relative_path
                .file_name()
                .is_some_and(|name| name.starts_with('.'));
            if hidden || ignores.iter().any(|p| p.matches(project_relative_path)) {
                if log_enabled!(log::Level::Info) {
                    let dir_marker = if is_dir { "/" } else { "" };
                    info!("ignoring {project_relative_path}{dir_marker}");
                }
                continue;
            }
        }

        if metadata.is_symlink() {
            if log_enabled!(log::Level::Info) {
                let symlink_path = entry_path.strip_prefix(ctx.project_root.as_ref())?;
                info!("ignoring /{symlink_path} (symlink)");
            }
        } else if is_dir {
            walkdir(ctx, &entry_path, ignores, allows, paths)?;
        } else if metadata.is_file() {
            paths.push(entry_path);
        } else {
            panic!("unreachable");
        }
    }

    Ok(())
}

fn init(init_args: InitCmd) -> Result<()> {
    let cwd = Utf8PathBuf::try_from(env::current_dir().map_err(|cause| Error::IO {
        path: PrettyPath::new(Utf8Path::new(".")),
        action: IOAction::Read,
        cause,
    })?)?;
    Context::init(cwd, init_args.force)?;
    let queries_dir = Context::acquire()?.manifest.queries_dir;
    printdoc!(
        "
            {}: vex initialised
            now add style rules in ./{}/
            for an example, open ./{}/{EXAMPLE_VEX_FILE}
        ",
        "success".if_supports_color(Stream::Stdout, |text| text.style(*SUCCESS_STYLE)),
        queries_dir.as_str(),
        queries_dir.as_str(),
    );
    Ok(())
}

#[cfg(test)]
mod test {
    use indoc::indoc;
    use insta::assert_yaml_snapshot;
    use joinery::JoinableIterator;

    use crate::vextest::VexTest;

    use super::*;

    #[test]
    fn max_problems() {
        const MAX: u32 = 47;
        let irritations = VexTest::new("max-problems")
            .with_max_problems(MaxProblems::Limited(MAX))
            .with_scriptlet(
                "vexes/var.star",
                indoc! {r#"
                    def init():
                        vex.observe('open_project', on_open_project)

                    def on_open_project(event):
                        vex.search(
                            'rust',
                            '(integer_literal) @num',
                            on_match,
                        )

                    def on_match(event):
                        vex.warn('oh no a number!', at=(event.captures['num'], 'num'))
                "#},
            )
            .with_source_file(
                "src/main.rs",
                indoc! {r#"
                    fn main() {
                        let x = 1 + 2 + 3 + 4 + 5 + 6 + 8 + 9 + 10;
                        let x = 1 + 2 + 3 + 4 + 5 + 6 + 8 + 9 + 10;
                        let x = 1 + 2 + 3 + 4 + 5 + 6 + 8 + 9 + 10;
                        let x = 1 + 2 + 3 + 4 + 5 + 6 + 8 + 9 + 10;
                        let x = 1 + 2 + 3 + 4 + 5 + 6 + 8 + 9 + 10;
                        let x = 1 + 2 + 3 + 4 + 5 + 6 + 8 + 9 + 10;
                        let x = 1 + 2 + 3 + 4 + 5 + 6 + 8 + 9 + 10;
                        let x = 1 + 2 + 3 + 4 + 5 + 6 + 8 + 9 + 10;
                        let x = 1 + 2 + 3 + 4 + 5 + 6 + 8 + 9 + 10;
                        let x = 1 + 2 + 3 + 4 + 5 + 6 + 8 + 9 + 10;
                        println!("{x}");
                    }
                "#},
            )
            .try_run()
            .unwrap()
            .into_irritations();
        assert_eq!(irritations.len(), MAX as usize);
    }

    #[test]
    fn readme() {
        // Dumb hacky test to serve until mdbook docs are made and tested.
        let collate_snippets = |language| {
            include_str!("../README.md")
                .lines()
                .scan(false, |collate_starlark, line| {
                    Some(if let Some(stripped) = line.strip_prefix("```") {
                        *collate_starlark = stripped.starts_with(language);
                        None
                    } else if *collate_starlark {
                        Some(line)
                    } else {
                        None
                    })
                })
                .flatten()
                .join_with("\n")
                .to_string()
        };
        let collated_starlark_snippets = collate_snippets("python");
        let collated_rust_snippets = collate_snippets("rust");
        let irritations = VexTest::new("README-snippets")
            .with_scriptlet("vexes/test.star", collated_starlark_snippets)
            .with_source_file("src/main.rs", collated_rust_snippets)
            .try_run()
            .unwrap()
            .into_irritations()
            .into_iter()
            .map(|irr| irr.to_string())
            .collect::<Vec<_>>();
        assert_yaml_snapshot!(irritations);
    }
}
