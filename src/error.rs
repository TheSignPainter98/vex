use std::{fmt, io, num, path, str::Utf8Error};

use derive_more::Display;
use joinery::JoinableIterator;
use strum::IntoEnumIterator;

use crate::{
    language::Language,
    query::Query,
    scriptlets::{action::Action, event::EventKind, LoadPath, Location},
    source_path::PrettyPath,
};

// TODO(kcza): box this!
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("{what} unavailable while {}", .action.pretty_name())]
    ActionUnavailable { what: &'static str, action: Action },

    #[error(
        "already inited in a parent directory {found_root}, to continue regardless, use --force"
    )]
    AlreadyInited { found_root: PrettyPath },

    #[error("cannot discern language of {path}: multiple patterns match (could be {language} or {other_language})")]
    AmbiguousLanguage {
        path: PrettyPath,
        language: Language,
        other_language: Language,
    },

    #[error(transparent)]
    Clap(#[from] clap::Error),

    #[error("query is empty")]
    EmptyQuery,

    #[error("cannot load language {language}: {cause}")]
    ExternalLanguage {
        language: Language,
        cause: ExternalLanguageError,
    },

    #[error(transparent)]
    Fmt(#[from] fmt::Error),

    #[error(transparent)]
    FromPathBuf(#[from] camino::FromPathBufError),

    #[error("cannot load {language} parser: {cause}")]
    InaccessibleParserFiles {
        language: Language,
        cause: anyhow::Error,
    },

    #[error("invalid ID '{raw_id}': {reason}")]
    InvalidID {
        raw_id: String,
        reason: InvalidIDReason,
    },

    #[error("invalid key '{raw_key}': {reason}")]
    InvalidScriptArgKey {
        raw_key: String,
        reason: InvalidIDReason,
    },

    #[error("import cycle detected: {}", .0.iter().join_with(" -> "))]
    ImportCycle(Vec<PrettyPath>),

    #[error("cannot load {load}: {reason}")]
    InvalidLoad {
        load: String,
        module: PrettyPath,
        reason: InvalidLoadReason,
    },

    #[error("invalid ignore query: {0}")]
    InvalidIgnoreQuery(InvalidIgnoreQueryReason),

    #[error("test invalid: {0}")]
    InvalidTest(String),

    #[error("{0}")]
    InvalidWarnCall(&'static str),

    #[error("cannot {action} {path}: {cause}")]
    IO {
        path: PrettyPath,
        action: IOAction,
        cause: io::Error,
    },

    #[error(transparent)]
    Language(#[from] tree_sitter::LanguageError),

    #[error("cannot find manifest, try running `vex init` in the project’s root")]
    ManifestNotFound,

    #[error("cannot discern language of {0}")]
    NoParserForFile(PrettyPath),

    #[error("cannot parse {0}: parser not provided")]
    NoParserForLanguage(Language),

    #[error("cannot find module '{0}'")]
    NoSuchModule(PrettyPath),

    #[error("cannot find vexes directory at {0}")]
    NoVexesDir(PrettyPath),

    #[error("{0} is not a check path")]
    NotACheckPath(PrettyPath),

    #[error(transparent)]
    ParseInt(#[from] num::ParseIntError),

    #[error(r#"cannot compile "{pattern}": {} at position {}"#, cause.msg, cause.pos - cause_pos_offset)]
    Pattern {
        pattern: String,
        cause_pos_offset: usize,
        cause: glob::PatternError,
    },

    #[error(transparent)]
    Query(#[from] tree_sitter::QueryError),

    #[error("ignoring '*' makes other ignore ids redundant")]
    RedundantIgnore,

    #[error(transparent)]
    SetLogger(#[from] log::SetLoggerError),

    #[error(transparent)]
    Starlark(anyhow::Error),

    #[error(transparent)]
    StripPrefix(#[from] path::StripPrefixError),

    #[error("test run invalid")]
    TestRunInvalid,

    #[error(transparent)]
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

    #[error(
        "unknown operator '{operator_name}' in '#{operator}'{}, expected one of {}",
        suggestion.map(|suggestion| format!(" (did you mean '{suggestion}'?)")).unwrap_or_default(),
        Query::KNOWN_OPERATORS.iter().join_with(", ")
    )]
    UnknownOperator {
        operator: String,
        operator_name: String,
        suggestion: Option<&'static str>,
    },

    #[error("{path}:{location}: cannot parse {language}")]
    UnparseableAsLanguage {
        path: PrettyPath,
        language: Language,
        location: Location,
    },

    #[error(transparent)]
    Utf8(#[from] Utf8Error),
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

#[derive(Debug, thiserror::Error)]
pub enum ExternalLanguageError {
    #[error("manifest language info missing `parser-dir` field")]
    MissingParserDir,
}

#[derive(Debug, Display)]
pub enum InvalidIDReason {
    #[display(fmt = "can only contain a-z, 0-9, ':' and '-'")]
    IllegalChar,

    #[display(fmt = "cannot start with '{_0}'")]
    IllegalStartChar(char),

    #[display(fmt = "cannot end with '{_0}'")]
    IllegalEndChar(char),

    #[display(fmt = "cannot contain '{found}'")]
    UglySubstring { found: String, index: usize },

    #[display(fmt = "too few characters ({len} < {min_len})")]
    TooShort { len: usize, min_len: usize },

    #[display(fmt = "too many characters ({len} > {max_len})")]
    TooLong { len: usize, max_len: usize },
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
        LoadPath::MIN_COMPONENT_LEN
    )]
    TooShortComponent,

    #[display(
        fmt = "load path stem must be at least {} characters",
        LoadPath::MIN_COMPONENT_LEN
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

    #[display(fmt = "load path cannot be outside of the vexes directory")]
    OutsideDirectory,

    #[display(fmt = "load path invalid, see docs")] // TODO(kcza): link to spec once public.
    NonSpecific,
}

#[derive(Debug, Display)]
pub enum InvalidIgnoreQueryReason {
    #[display(fmt = "query captured nothing")]
    CapturedNothing,

    #[display(fmt = "query did not capture 'vex:ignore' marker at {path}:{location}")]
    CapturedTextExcludesIgnoreMarker {
        path: PrettyPath,
        location: Location,
    },

    #[display(fmt = "{_0}")]
    General(Box<Error>),

    #[display(fmt = "missing capture group '{_0}'")]
    MissingCaptureGroup(&'static str),
}
