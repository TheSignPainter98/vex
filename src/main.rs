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
mod verbosity;
mod vex;

#[cfg(test)]
mod vextest;

use std::{env, fs, process::ExitCode};

use camino::{Utf8Path, Utf8PathBuf};
use clap::Parser as _;
use cli::{DumpCmd, MaxProblems};
use dupe::Dupe;
use log::{info, log_enabled, trace, warn};
use starlark::environment::Module;
use strum::IntoEnumIterator;
use tree_sitter::QueryCursor;

use crate::{
    cli::{Args, CheckCmd, Command},
    context::{CompiledFilePattern, Context},
    error::{Error, IOAction},
    irritation::Irritation,
    plural::Plural,
    result::Result,
    scriptlets::{
        event::CloseProjectEvent,
        event::{CloseFileEvent, MatchEvent, OpenFileEvent, OpenProjectEvent},
        Observer, PreinitingStore, QueryCaptures, VexingStore,
    },
    source_file::SourceFile,
    source_path::{PrettyPath, SourcePath},
    supported_language::SupportedLanguage,
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
    store.vexes().for_each(|vex| println!("{}", vex.path));
    Ok(())
}

fn check(cmd_args: CheckCmd) -> Result<()> {
    let ctx = Context::acquire()?;
    let store = PreinitingStore::new(&ctx)?.preinit()?.init()?;

    let irritations = vex(&ctx, &store, cmd_args.max_problems)?;
    irritations.iter().for_each(|irr| println!("{irr}"));
    warn!(
        // "scanned {} and found {}",
        // Plural::new(npaths, "path", "paths"),
        "vex found {}",
        Plural::new(irritations.len(), "problem", "problems"),
    );

    Ok(())
}

fn vex(ctx: &Context, store: &VexingStore, max_problems: MaxProblems) -> Result<Vec<Irritation>> {
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
            .collect::<Result<Vec<_>>>()?;
        let allows = ctx
            .allows
            .clone()
            .unwrap_or_default()
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
            .collect::<Vec<_>>()
    };

    for observer in language_observers.values().flatten() {
        for on_open_project in &observer.on_open_project[..] {
            let handler_module = Module::new();
            on_open_project.handle(
                &handler_module,
                &observer.path, // TODO(kcza): what path is this??
                OpenProjectEvent::new(ctx.project_root.dupe()),
            )?;
        }
    }
    let mut irritations = Vec::new();
    for path in paths {
        let src_file = match SourceFile::load(path.dupe()) {
            Ok(f) => f,
            Err(e) if e.is_recoverable() => {
                if log_enabled!(log::Level::Trace) {
                    trace!("ignoring {path}: {e}");
                }
                continue;
            }
            Err(e) => {
                return Err(e);
            }
        };

        for observer in &language_observers[src_file.lang] {
            for on_open_file in &observer.on_open_file[..] {
                let handler_module = Module::new();
                irritations.extend(on_open_file.handle(
                    &handler_module,
                    &observer.path,
                    OpenFileEvent::new(src_file.path.pretty_path.dupe()),
                )?);
            }

            for qmatch in QueryCursor::new().matches(
                observer.query.as_ref(),
                src_file.tree.root_node(),
                src_file.content[..].as_bytes(),
            ) {
                let captures = QueryCaptures::new(&observer.query, &qmatch, &src_file);
                for on_match in &observer.on_match[..] {
                    let handler_module = Module::new();
                    irritations.extend(on_match.handle(
                        &handler_module,
                        &observer.path,
                        MatchEvent::new(src_file.path.pretty_path.dupe(), captures.dupe()),
                    )?);
                }
            }

            for on_close_file in &observer.on_close_file[..] {
                let handler_module = Module::new();
                irritations.extend(on_close_file.handle(
                    &handler_module,
                    &observer.path,
                    CloseFileEvent::new(src_file.path.pretty_path.dupe()),
                )?);
            }
        }
    }
    for language_observer in language_observers.values() {
        for observer in language_observer {
            for on_close_project in &observer.on_close_project[..] {
                let handler_module = Module::new();
                irritations.extend(on_close_project.handle(
                    &handler_module,
                    &observer.path,
                    CloseProjectEvent::new(ctx.project_root.dupe()),
                )?);
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

    irritations.sort();
    if let MaxProblems::Limited(max) = max_problems {
        let max = max as usize;
        if max < irritations.len() {
            irritations.truncate(max);
        }
    }
    Ok(irritations)
}

fn walkdir(
    ctx: &Context,
    path: &Utf8Path,
    ignores: &[CompiledFilePattern],
    allows: &[CompiledFilePattern],
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

fn dump(dump_args: DumpCmd) -> Result<()> {
    let src_path =
        SourcePath::new_absolute(&dump_args.path.canonicalize_utf8().map_err(|e| Error::IO {
            path: PrettyPath::new(Utf8Path::new(&dump_args.path)),
            action: IOAction::Read,
            cause: e,
        })?);
    let src_file = SourceFile::load(src_path)?;
    if src_file.tree.root_node().has_error() {
        return Err(Error::Unparseable {
            path: PrettyPath::new(Utf8Path::new(&dump_args.path)),
            language: src_file.lang,
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
    use std::{fs::File, io::Write};

    use indoc::indoc;
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
            format!("cannot parse {} as rust", test_file.path)
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
                        vex.language('rust')
                        vex.query('(integer_literal) @num')
                        vex.observe('match', on_match)

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
            .unwrap();
        assert_eq!(irritations.len(), MAX as usize);
    }
}
