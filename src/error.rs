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
    Starlark(anyhow::Error),

    #[error("{0}")]
    Clap(#[from] clap::Error),

    #[error("{0} declares empty query")]
    EmptyQuery(PrettyPath),

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

    #[error("cannot find manifest, try running `vex init` in the project’s root")]
    ManifestNotFound,

    #[error("{0} declares no callbacks")]
    NoCallbacks(PrettyPath),

    #[error("{0} has no file extension")]
    NoExtension(PrettyPath),

    #[error("{0} declares no init function")]
    NoInit(PrettyPath),

    #[error("{0} declares no target language")]
    NoLanguage(PrettyPath),

    #[error("{0} declares no match observer")]
    NoMatch(PrettyPath),

    #[error("{0} declares no query")]
    NoQuery(PrettyPath),

    #[error("cannot find module '{0}'")]
    NoSuchModule(PrettyPath),

    #[error("cannot find vexes directory at {0}")]
    NoVexesDir(Utf8PathBuf),

    #[error("{0}")]
    ParseInt(#[from] num::ParseIntError),

    #[error("{0}")]
    Pattern(#[from] glob::PatternError),

    #[error("{0}")]
    Query(#[from] tree_sitter::QueryError),

    #[error("{0}")]
    SetLogger(#[from] log::SetLoggerError),

    #[error("{0}")]
    StripPrefix(#[from] path::StripPrefixError),

    #[error("{0}")]
    Toml(#[from] toml_edit::de::Error),

    #[error("cannot freeze a {0}")]
    Unfreezable(&'static str),

    #[error("unknown event '{0}', expected one of: {}", EventType::iter().join_with(", "))]
    UnknownEvent(String),

    #[error("unknown extension '{0}'")]
    UnknownExtension(String),

    #[error("unknown language '{0}'")]
    UnknownLanguage(String),

    #[error("cannot parse {path} as {language}")]
    Unparseable {
        path: PrettyPath,
        language: SupportedLanguage,
    },
}

impl Error {
    pub fn is_recoverable(&self) -> bool {
        match self {
            Self::ActionUnavailable { .. }
            | Self::AlreadyInited { .. }
            | Self::Clap { .. }
            | Self::EmptyQuery(..)
            | Self::FromPathBuf { .. }
            | Self::ImportCycle(..)
            | Self::InvalidWarnCall(..)
            | Self::Language(..)
            | Self::ManifestNotFound
            | Self::NoCallbacks(..)
            | Self::NoInit(..)
            | Self::NoLanguage(..)
            | Self::NoMatch(..)
            | Self::NoQuery(..)
            | Self::NoSuchModule(..)
            | Self::NoVexesDir(..)
            | Self::ParseInt(..)
            | Self::Pattern(..)
            | Self::Query(..)
            | Self::SetLogger(..)
            | Self::Starlark { .. }
            | Self::StripPrefix(..)
            | Self::Toml(..)
            | Self::Unfreezable(..)
            | Self::UnknownEvent(..)
            | Self::UnknownLanguage(..) => false,
            Self::IO { .. }
            | Self::NoExtension(..)
            | Self::UnknownExtension { .. }
            | Self::Unparseable { .. } => true,
        }
    }
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
