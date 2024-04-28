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
mod result;
mod scriptlets;
mod source_file;
mod source_path;
mod supported_language;
mod trigger;
mod verbosity;
mod vex;

#[cfg(test)]
mod vextest;

use std::{cell::OnceCell, env, fs, iter, process::ExitCode};

use camino::{Utf8Path, Utf8PathBuf};
use clap::Parser as _;
use cli::{DumpCmd, MaxProblems};
use dupe::Dupe;
use log::{info, log_enabled, trace, warn};
use source_file::SourceFile;
use starlark::environment::Module;
use strum::IntoEnumIterator;
use tree_sitter::QueryCursor;

use crate::{
    cli::{Args, CheckCmd, Command},
    context::Context,
    error::{Error, IOAction},
    irritation::Irritation,
    plural::Plural,
    result::Result,
    scriptlets::{
        event::CloseProjectEvent,
        event::{CloseFileEvent, OpenFileEvent, OpenProjectEvent, QueryMatchEvent},
        Observer, PreinitingStore, QueryCaptures, VexingStore,
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
        Command::ListLanguages => list_languages(),
        Command::ListLints => list_lints(),
        Command::Check(cmd_args) => check(cmd_args),
        Command::Dump(dump_args) => dump(dump_args),
        Command::Init => init(),
    }?;

    Ok(logger::report())
}

fn list_languages() -> Result<()> {
    SupportedLanguage::iter().for_each(|lang| println!("{}", lang));
    Ok(())
}

fn list_lints() -> Result<()> {
    let ctx = Context::acquire()?;
    let store = PreinitingStore::new(&ctx)?.preinit()?;
    store
        .vexes()
        .for_each(|vex| println!("{}", vex.path.pretty_path));
    Ok(())
}

fn check(cmd_args: CheckCmd) -> Result<()> {
    let ctx = Context::acquire()?;
    let store = PreinitingStore::new(&ctx)?.preinit()?.init()?;

    let RunData {
        irritations,
        num_files_scanned,
    } = vex(&ctx, &store, cmd_args.max_problems)?;
    irritations.iter().for_each(|irr| println!("{irr}"));
    if !irritations.is_empty() {
        warn!(
            "found {} across {}",
            Plural::new(irritations.len(), "problem", "problems"),
            Plural::new(num_files_scanned, "file", "files"),
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
            .map(|ignore| ignore.compile(&ctx.project_root))
            .collect::<Result<Vec<_>>>()?;
        let allows = ctx
            .allows
            .clone()
            .into_iter()
            .map(|allow| allow.compile(&ctx.project_root))
            .collect::<Result<Vec<_>>>()?;
        walkdir(
            ctx,
            ctx.project_root.as_ref(),
            &ignores,
            &allows,
            &mut paths,
        )?;
        paths
            .into_iter()
            .map(|p| SourcePath::new(&p, &ctx.project_root))
            .map(SourceFile::new)
            .collect::<Result<Vec<_>>>()?
    };

    let mut irritations = vec![];

    let observers = store.observers().collect::<Vec<_>>();
    observers
        .iter()
        .flat_map(|obs| iter::repeat(&obs.vex_path).zip(&obs.on_open_project))
        .try_for_each(|(obs_path, obs)| {
            irritations.extend(obs.handle(
                &Module::new(),
                obs_path,
                OpenProjectEvent::new(ctx.project_root.dupe()),
            )?);
            Ok::<_, Error>(())
        })?;

    for file in &files {
        let parsed_file_cell = OnceCell::new(); // TODO(kcza): replace with LazyCell once
                                                // sufficiently stable (tracking issue https://github.com/rust-lang/rust/issues/109736)
        store
            .observers_for(file)
            .filter_map(|(trigger_id, observer)| {
                if let Ok(parsed_file) = parsed_file_cell.get_or_init(|| file.parse()) {
                    Some((parsed_file, trigger_id, observer))
                } else {
                    None
                }
            })
            .try_for_each(|(parsed_file, trigger_id, observer)| {
                observer.on_open_file.iter().try_for_each(|on_open_file| {
                    irritations.extend(on_open_file.handle(
                        &Module::new(),
                        &observer.vex_path,
                        OpenFileEvent::new(parsed_file.path.pretty_path.dupe(), trigger_id.dupe()),
                    )?);
                    Ok::<_, Error>(())
                })?;

                observer
                    .trigger_queries()
                    .try_for_each(|(trigger_id, query)| {
                        QueryCursor::new()
                            .matches(
                                query,
                                parsed_file.tree.root_node(),
                                parsed_file.content.as_bytes(),
                            )
                            .try_for_each(|qmatch| {
                                let captures = QueryCaptures::new(query, &qmatch, parsed_file);
                                observer.on_match.iter().try_for_each(|on_match| {
                                    irritations.extend(on_match.handle(
                                        &Module::new(),
                                        &observer.vex_path,
                                        QueryMatchEvent::new(
                                            parsed_file.path.pretty_path.dupe(),
                                            captures.dupe(),
                                            trigger_id.cloned(),
                                        ),
                                    )?);
                                    Ok::<_, Error>(())
                                })
                            })
                    })?;

                observer
                    .on_close_file
                    .iter()
                    .try_for_each(|on_close_file| {
                        irritations.extend(on_close_file.handle(
                            &Module::new(),
                            &observer.vex_path,
                            CloseFileEvent::new(
                                parsed_file.path.pretty_path.dupe(),
                                trigger_id.dupe(),
                            ),
                        )?);
                        Ok::<_, Error>(())
                    })?;

                Ok::<_, Error>(())
            })?;
    }

    observers
        .iter()
        .flat_map(|obs| iter::repeat(&obs.vex_path).zip(&obs.on_close_project))
        .try_for_each(|(obs_path, obs)| {
            irritations.extend(obs.handle(
                &Module::new(),
                obs_path,
                CloseProjectEvent::new(ctx.project_root.dupe()),
            )?);
            Ok::<_, Error>(())
        })?;

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
        let relative_path =
            Utf8Path::new(&entry_path.as_str()[1 + ctx.project_root.as_str().len()..]);
        if !allows.iter().any(|p| p.matches(relative_path)) {
            let hidden = relative_path
                .file_name()
                .is_some_and(|name| name.starts_with('.'));
            if hidden || ignores.iter().any(|p| p.matches(relative_path)) {
                if log_enabled!(log::Level::Info) {
                    let dir_marker = if metadata.is_dir() { "/" } else { "" };
                    info!("ignoring /{relative_path}{dir_marker}");
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

fn dump(dump_args: DumpCmd) -> Result<()> {
    let src_path =
        SourcePath::new_absolute(&dump_args.path.canonicalize_utf8().map_err(|e| Error::IO {
            path: PrettyPath::new(Utf8Path::new(&dump_args.path)),
            action: IOAction::Read,
            cause: e,
        })?);
    let src_file = SourceFile::new(src_path)?.parse()?;
    if src_file.tree.root_node().has_error() {
        return Err(Error::Unparseable {
            path: PrettyPath::new(Utf8Path::new(&dump_args.path)),
            language: src_file.language,
        });
    }

    println!("{}", src_file.tree.root_node().to_sexp());

    Ok(())
}

fn init() -> Result<()> {
    let cwd = Utf8PathBuf::try_from(env::current_dir().map_err(|cause| Error::IO {
        path: PrettyPath::new(Utf8Path::new(".")),
        action: IOAction::Read,
        cause,
    })?)?;
    Context::init(cwd)
}

#[cfg(test)]
mod test {
    use std::{fs::File, io::Write, path};

    use indoc::indoc;
    use insta::assert_yaml_snapshot;
    use joinery::JoinableIterator;
    use tempfile::TempDir;

    use crate::vextest::VexTest;

    use super::*;

    struct TestFile {
        _dir: TempDir,
        path: Utf8PathBuf,
    }

    impl TestFile {
        fn new(path: impl AsRef<str>, content: impl AsRef<[u8]>) -> TestFile {
            let dir = tempfile::tempdir().unwrap();
            let file_path = Utf8PathBuf::try_from(dir.path().to_path_buf())
                .unwrap()
                .join(path.as_ref());

            fs::create_dir_all(file_path.parent().unwrap()).unwrap();
            File::create(&file_path)
                .unwrap()
                .write_all(content.as_ref())
                .unwrap();

            TestFile {
                _dir: dir,
                path: file_path,
            }
        }
    }

    #[test]
    fn dump_valid_file() {
        let test_file = TestFile::new(
            "path/to/file.rs",
            indoc! {r#"
                fn add(a: i32, b: i32) -> i32 {
                    a + b
                }
            "#},
        );

        let args = Args::try_parse_from(["vex", "dump", test_file.path.as_str()]).unwrap();
        let cmd = args.command.into_dump_cmd().unwrap();
        dump(cmd).unwrap();
    }

    #[test]
    fn dump_nonexistent_file() {
        let file_path = "/i/do/not/exist.rs";
        let args = Args::try_parse_from(["vex", "dump", file_path]).unwrap();
        let cmd = args.command.into_dump_cmd().unwrap();
        let err = dump(cmd).unwrap_err();
        if cfg!(target_os = "windows") {
            assert_eq!(
                err.to_string(),
                "cannot read /i/do/not/exist.rs: The system cannot find the path specified. (os error 3)"
            );
        } else {
            assert_eq!(
                err.to_string(),
                "cannot read /i/do/not/exist.rs: No such file or directory (os error 2)"
            );
        }
    }

    #[test]
    fn dump_invalid_file() {
        let test_file = TestFile::new(
            "src/file.rs",
            indoc! {r#"
                i am not valid a valid rust file!
            "#},
        );
        let args = Args::try_parse_from(["vex", "dump", test_file.path.as_str()]).unwrap();
        let cmd = args.command.into_dump_cmd().unwrap();
        let err = dump(cmd).unwrap_err();
        assert_eq!(
            err.to_string(),
            format!(
                "cannot parse {} as rust",
                test_file.path.as_str().replace(path::MAIN_SEPARATOR, "/")
            )
        );
    }

    #[test]
    fn no_extension() {
        let test_file = TestFile::new("no-extension", "");
        let args = Args::try_parse_from(["vex", "dump", test_file.path.as_str()]).unwrap();
        let cmd = args.command.into_dump_cmd().unwrap();
        let err = dump(cmd).unwrap_err();

        // Assertion relaxed due to strange Github Actions Windows and Macos runner path handling.
        let expected = "no-extension has no file extension";
        assert!(
            err.to_string().ends_with(&expected),
            "unexpected error: expected {expected} but got {err}"
        );
    }

    #[test]
    fn unknown_extension() {
        let test_file = TestFile::new("file.unknown-extension", "");
        let args = Args::try_parse_from(["vex", "dump", test_file.path.as_str()]).unwrap();
        let cmd = args.command.into_dump_cmd().unwrap();
        let err = dump(cmd).unwrap_err();
        assert_eq!(
            err.to_string(),
            format!("unknown extension 'unknown-extension'")
        );
    }

    #[test]
    fn max_problems() {
        const MAX: u32 = 47;
        let irritations = VexTest::new("max-problems")
            .with_max_problems(MaxProblems::Limited(MAX))
            .with_scriptlet(
                "vexes/var.star",
                indoc! {r#"
                    def init():
                        vex.add_trigger(
                            language='rust',
                            query='(integer_literal) @num',
                        )
                        vex.observe('query_match', on_query_match)

                    def on_query_match(event):
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
