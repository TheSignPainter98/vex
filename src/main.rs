#![deny(missing_debug_implementations)]

#[cfg(test)]
#[macro_use]
extern crate pretty_assertions;

mod associations;
mod cli;
mod context;
mod dump;
mod error;
mod ignore_markers;
mod irritation;
mod logger;
mod plural;
mod query;
mod result;
mod scan;
mod scriptlets;
mod source_file;
mod source_path;
mod suggestion;
mod supported_language;
mod test;
mod trigger;
mod verbosity;
mod vex_id;

#[cfg(test)]
mod vextest;

use std::{env, process::ExitCode};

use camino::Utf8PathBuf;
use indoc::{formatdoc, printdoc};
use log::{debug, info, log_enabled};
use rayon::ThreadPoolBuilder;
use strum::IntoEnumIterator;

use crate::{
    cli::{Args, CheckCmd, Command, InitCmd, ListCmd, ToList},
    context::{Context, EXAMPLE_VEX_FILE},
    error::{Error, IOAction},
    plural::Plural,
    result::Result,
    scan::ProjectRunData,
    scriptlets::{source, InitOptions, PreinitOptions, PreinitingStore},
    source_path::PrettyPath,
    supported_language::SupportedLanguage,
    verbosity::Verbosity,
};

// TODO(kcza): move the subcommands to separate files
fn main() -> ExitCode {
    match run() {
        Ok(c) => c,
        Err(e) => {
            crate::error!("{e}");
            ExitCode::from(u8::MAX)
        }
    }
}

fn run() -> Result<ExitCode> {
    let args = Args::parse();

    let verbosity = if args.quiet {
        Verbosity::Quiet
    } else {
        args.verbosity_level.try_into()?
    };
    logger::init(verbosity)?;

    if log_enabled!(log::Level::Info) {
        print_banner();
    }

    match args.command {
        Command::Check(cmd_args) => check(cmd_args),
        Command::Dump(dump_args) => dump::dump(dump_args),
        Command::List(list_args) => list(list_args),
        Command::Init(init_args) => init(init_args),
        Command::Test => test::test(),
    }?;

    Ok(logger::exit_code())
}

fn print_banner() {
    printdoc! {
        r#"

            oooooo      oooo
             `888.      .8'
              `888.    .8'   .ooooo.   ooooo ooo
               `888.  .8'   d88' `88b   `888.8"
                `888..8'    888ooo888    `888.
                 `8888'     888    .o   .8"888.
                  `88'      `Y8bod8P'  o8o o888o

            Let the pedantry begin.

        "#
    };
}

fn list(list_args: ListCmd) -> Result<()> {
    match list_args.what {
        ToList::Languages => SupportedLanguage::iter().for_each(|lang| println!("{}", lang)),
    }
    Ok(())
}

fn check(cmd_args: CheckCmd) -> Result<()> {
    let ctx = Context::acquire()?;
    let verbosity = logger::verbosity();

    let store = {
        let preinit_opts = PreinitOptions {
            lenient: cmd_args.lenient,
            verbosity,
        };
        let init_opts = InitOptions { verbosity };
        PreinitingStore::new(&source::sources_in_dir(&ctx.vex_dir())?)?
            .preinit(preinit_opts)?
            .init(init_opts)?
    };

    // Configure global `rayon` thread pool.
    ThreadPoolBuilder::new()
        .num_threads(cmd_args.max_concurrent_files.into())
        .build_global()
        .expect("internal error: failed to configure global thread pool");

    let ProjectRunData {
        irritations,
        num_files_scanned,
        num_bytes_scanned,
    } = scan::scan_project(
        &ctx,
        &store,
        cmd_args.max_problems,
        cmd_args.max_concurrent_files,
        verbosity,
    )?;
    irritations
        .iter()
        .for_each(|irr| crate::warn!(custom=true; "{irr}"));

    if log_enabled!(log::Level::Info) {
        info!(
            "scanned {}",
            Plural::new(num_files_scanned, "file", "files"),
        );
    }
    if log_enabled!(log::Level::Debug) {
        let pretty_approx = |num| {
            let num = num as f64;
            if num < 1_000.0 {
                format!("{num}")
            } else if num < 1_000_000.0 {
                format!("{:.1}K", num / 1_000.0)
            } else if num < 1_000_000_000.0 {
                format!("{:.1}M", num / 1_000_000.0)
            } else {
                format!("{:.1}G", num / 1_000_000_000.0)
            }
        };
        debug!("scanned {} bytes", pretty_approx(num_bytes_scanned),);
    }

    let num_problems = irritations.len()
        + *logger::NUM_ERRS.lock().expect("failed to lock NUM_ERRS") as usize
        + *logger::NUM_WARNINGS
            .lock()
            .expect("failed to lock NUM_WARNINGS") as usize;
    if num_problems != 0 {
        crate::warn!("found {}", Plural::new(num_problems, "problem", "problems"));
    } else {
        success!("no problems found");
    }

    Ok(())
}

fn init(init_args: InitCmd) -> Result<()> {
    let cwd = Utf8PathBuf::try_from(env::current_dir().map_err(|cause| Error::IO {
        path: PrettyPath::from("."),
        action: IOAction::Read,
        cause,
    })?)?;
    Context::init(cwd, init_args.force)?;
    let queries_dir = Context::acquire()?.manifest.metadata.queries_dir;
    success!(
        "{}",
        formatdoc!(
            "
                vex initialised
                now add style rules in ./{}/
                for an example, open ./{}/{EXAMPLE_VEX_FILE}",
            queries_dir.as_str(),
            queries_dir.as_str(),
        )
    );
    Ok(())
}

#[cfg(test)]
mod test_ {
    use indoc::indoc;
    use insta::assert_yaml_snapshot;
    use joinery::JoinableIterator;

    use crate::{cli::MaxProblems, vextest::VexTest};

    use super::*;

    #[test]
    fn max_problems() {
        const MAX: u32 = 47;
        let irritations = VexTest::new("max-problems")
            .with_max_problems(MaxProblems::Limited(MAX))
            .with_scriptlet(
                "vexes/test.star",
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
                        vex.warn('test', 'oh no a number!', at=(event.captures['num'], 'num'))
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
            .irritations;
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
            .with_scriptlet("vexes/distracting_operand.star", collated_starlark_snippets)
            .with_source_file("src/main.rs", collated_rust_snippets)
            .try_run()
            .unwrap()
            .irritations
            .into_iter()
            .map(|irr| irr.to_string())
            .collect::<Vec<_>>();
        assert_yaml_snapshot!(irritations);
    }
}
