use std::{io, num, path};

use camino::Utf8PathBuf;
use joinery::JoinableIterator;
use strum::IntoEnumIterator;

use crate::{
    scriptlets::{action::Action, event::EventType},
    source_path::PrettyPath,
    supported_language::SupportedLanguage,
};

// TODO(kcza): box this!
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("{what} unavailable while {}", .action.name())]
    ActionUnavailable { what: &'static str, action: Action },

    #[error("already inited in a parent directory {found_root}")]
    AlreadyInited { found_root: Utf8PathBuf },

    #[error("{0}")]
    Clap(#[from] clap::Error),

    #[error("{0} adds trigger with empty query")]
    EmptyQuery(PrettyPath),

    #[error("{0} adds empty trigger")]
    EmptyTrigger(PrettyPath),

    #[error("{0}")]
    FromPathBuf(#[from] camino::FromPathBufError),

    #[error("import cycle detected: {}", .0.iter().join_with(" -> "))]
    ImportCycle(Vec<PrettyPath>),

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

    #[error(
        "cannot declare triggers with different languages: expected {expected} but found {found}"
    )]
    LanguageMismatch {
        expected: SupportedLanguage,
        found: SupportedLanguage,
    },

    #[error("cannot find manifest, try running `vex init` in the project’s root")]
    ManifestNotFound,

    #[error("{0} declares no callbacks")]
    // TODO(kcza): rename this to 'observer'
    NoCallbacks(PrettyPath),

    #[error("{0} has no file extension")]
    NoExtension(PrettyPath),

    #[error("{0} declares no init function")]
    NoInit(PrettyPath),

    #[error("{0} observes query_match but adds no triggers with queries")]
    NoQuery(PrettyPath),

    #[error("{0} adds trigger with query but does not observe query_match")]
    NoQueryMatch(PrettyPath),

    #[error("cannot find module '{0}'")]
    NoSuchModule(PrettyPath),

    #[error("{0} adds no triggers")]
    NoTriggers(PrettyPath),

    #[error("cannot find vexes directory at {0}")]
    NoVexesDir(Utf8PathBuf),

    #[error("{0}")]
    ParseInt(#[from] num::ParseIntError),

    #[error("cannot compile {pattern}@{}: {}", cause.pos, cause.msg)]
    Pattern {
        pattern: String,
        cause: glob::PatternError,
    },

    #[error("{0}")]
    Query(#[from] tree_sitter::QueryError),

    #[error("cannot add query without specifying a language")]
    QueryWithoutLanguage,

    #[error("{0}")]
    SetLogger(#[from] log::SetLoggerError),

    #[error("{0}")]
    Starlark(anyhow::Error),

    #[error("{0}")]
    StripPrefix(#[from] path::StripPrefixError),

    #[error("{0}")]
    Toml(#[from] toml_edit::de::Error),

    #[error("cannot freeze a {0}")]
    Unfreezable(&'static str),

    #[error(
        "unknown event '{name}'{}, expected one of: {}",
        suggestion.map(|suggestion| format!(" (did you mean '{suggestion}'?)")).unwrap_or_default(),
        EventType::iter().join_with(", "),
    )]
    UnknownEvent {
        name: String,
        suggestion: Option<&'static str>,
    },

    #[error("unknown extension '{0}'")]
    UnknownExtension(String),

    #[error("unsupported language '{0}'")]
    UnknownLanguage(String),

    #[error("cannot parse {path} as {language}")]
    Unparseable {
        path: PrettyPath,
        language: SupportedLanguage,
    },
}

impl Error {
    pub fn starlark(err: anyhow::Error) -> Self {
        match err.downcast().map_err(Error::Starlark) {
            Ok(err) => err,
            Err(err) => err,
        }
    }
}

#[derive(Debug, strum::Display)]
pub enum IOAction {
    #[strum(serialize = "read")]
    Read,

    #[strum(serialize = "write")]
    Write,
}
