use std::{io, num, path};

use camino::Utf8PathBuf;
use derive_more::Display;
use joinery::JoinableIterator;
use strum::IntoEnumIterator;

use crate::{
    scriptlets::{action::Action, event::EventKind, LoadStatementModule},
    source_path::PrettyPath,
    supported_language::SupportedLanguage,
};

// TODO(kcza): box this!
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("{what} unavailable while {}", .action.pretty_name())]
    ActionUnavailable { what: &'static str, action: Action },

    #[error("already inited in a parent directory {found_root}; to ignore it use --force")]
    AlreadyInited { found_root: Utf8PathBuf },

    #[error("{0}")]
    Clap(#[from] clap::Error),

    #[error("query is empty")]
    EmptyQuery,

    #[error("{0}")]
    FromPathBuf(#[from] camino::FromPathBufError),

    #[error("import cycle detected: {}", .0.iter().join_with(" -> "))]
    ImportCycle(Vec<PrettyPath>),

    #[error("cannot load {load}: {reason}")]
    InvalidLoad {
        load: String,
        module: PrettyPath,
        reason: InvalidLoadReason,
    },

    #[error("{0}")]
    InvalidWarnCall(&'static str),

    #[error("cannot {action} {path}: {cause}")]
    IO {
        path: PrettyPath,
        action: IOAction,
        cause: io::Error,
    },

    #[error("{0}")]
    Language(#[from] tree_sitter::LanguageError),

    #[error("cannot find manifest, try running `vex init` in the project’s root")]
    ManifestNotFound,

    #[error("{0} declares no init function")]
    NoInit(PrettyPath),

    #[error("{0} observes no events")]
    NoObservers(PrettyPath),

    #[error("cannot find module '{0}'")]
    NoSuchModule(PrettyPath),

    #[error("cannot find vexes directory at {0}")]
    NoVexesDir(Utf8PathBuf),

    #[error("{0}")]
    ParseInt(#[from] num::ParseIntError),

    #[error(r#"cannot compile "{pattern}": {} at position {}"#, cause.msg, cause.pos - cause_pos_offset)]
    Pattern {
        pattern: String,
        cause_pos_offset: usize,
        cause: glob::PatternError,
    },

    #[error("{0}")]
    Query(#[from] tree_sitter::QueryError),

    #[error("{0}")]
    SetLogger(#[from] log::SetLoggerError),

    #[error("{0}")]
    Starlark(anyhow::Error),

    #[error("{0}")]
    StripPrefix(#[from] path::StripPrefixError),

    #[error("{0}")]
    Toml(#[from] toml_edit::de::Error),

    #[error(
        "unknown event '{name}'{}, expected one of: {}",
        suggestion.map(|suggestion| format!(" (did you mean '{suggestion}'?)")).unwrap_or_default(),
        EventKind::iter().filter(|kind| kind.parseable()).map(|kind| kind.name()).join_with(", "),
    )]
    UnknownEvent {
        name: String,
        suggestion: Option<&'static str>,
    },

    #[error("unknown extension '{0}'")]
    UnknownExtension(String),

    #[error("unsupported language '{0}'")]
    UnknownLanguage(String),

    #[error("cannot parse {0}")]
    Unparseable(PrettyPath),

    #[error("cannot parse {path} as {language}")]
    UnparseableAsLanguage {
        path: PrettyPath,
        language: SupportedLanguage,
    },
}

impl From<anyhow::Error> for Error {
    fn from(err: anyhow::Error) -> Self {
        match err.downcast::<Error>() {
            Ok(err) => err,
            Err(err) => Error::Starlark(err),
        }
    }
}

impl From<starlark::Error> for Error {
    fn from(err: starlark::Error) -> Self {
        err.into_anyhow().into()
    }
}

#[derive(Debug, Display)]
pub enum IOAction {
    #[display(fmt = "create")]
    Create,

    #[display(fmt = "read")]
    Read,

    #[display(fmt = "write")]
    Write,
}

#[derive(Debug, Display)]
pub enum InvalidLoadReason {
    #[display(fmt = "load path cannot have underscores at component-ends")]
    UnderscoresAtEndOfComponent,

    #[display(fmt = "load path cannot contain `//`")]
    DoubleSlash,

    #[display(fmt = "load path can only contain a-z, 0-9, `_`, `.` and `/`, found `{_0}`")]
    ForbiddenChar(char),

    #[display(fmt = "load path cannot have hidden components")]
    HiddenComponent,

    #[display(fmt = "load path can only have a `.` in the file extension")]
    MidwayDot,

    #[display(fmt = "load path must have the `.star` extension")]
    IncorrectExtension,

    #[display(fmt = "load path can only have path operators at the start")]
    MidwayPathOperator,

    #[display(fmt = "load path cannot contain multiple `./`")]
    MultipleCurDirs,

    #[display(fmt = "load path cannot contain successive dots in file component")]
    SuccessiveDots,

    #[display(fmt = "load path cannot contain successive underscores")]
    SuccessiveUnderscores,

    #[display(
        fmt = "load path components must be at least {} characters",
        LoadStatementModule::MIN_COMPONENT_LEN
    )]
    TooShortComponent,

    #[display(
        fmt = "load path stem must be at least {} characters",
        LoadStatementModule::MIN_COMPONENT_LEN
    )]
    TooShortStem,

    #[display(fmt = "load path cannot have underscores at end of stem")]
    UnderscoreAtEndOfStem,

    #[display(fmt = "load path cannot be absolute")]
    Absolute,

    #[display(fmt = "load path must be files, not directories")]
    Dir,

    #[display(fmt = "load path cannot be empty")]
    Empty,

    #[display(fmt = "load path cannot contain both `./` and `../`")]
    MixedPathOperators,

    #[display(fmt = "load path invalid, see docs")] // TODO(kcza): link to spec once public.
    NonSpecific,
}
