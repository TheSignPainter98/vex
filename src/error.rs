use camino::Utf8PathBuf;
use joinery::JoinableIterator;
use strum::IntoEnumIterator;

use crate::scriptlets::{action::Action, event::EventType};

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum Error {
    #[error("already inited in a parent directory {found_root}")]
    AlreadyInited { found_root: Utf8PathBuf },

    #[error("import cycle detected: {}", .0.iter().join_with(" -> "))]
    ImportCycle(Vec<Utf8PathBuf>),

    #[error("cannot find manifest, try running `vex init` in the project’s root")]
    ManifestNotFound,

    // #[error("{0} has no file name")]
    // NoFileName(Utf8PathBuf),
    #[error("{0} declares init function")]
    NoInit(Utf8PathBuf),

    #[error("{0} declares no callbacks")]
    NoCallbacks(Utf8PathBuf),

    #[error("{0} declares no target language")]
    NoLanguage(Utf8PathBuf),

    #[error("{0} declares no query")]
    NoQuery(Utf8PathBuf),

    #[error("{what} is unavailable during {}", .action.name())]
    Unavailable { what: &'static str, action: Action },

    #[error("unknown event '{0}', expected one of: {}", EventType::iter().join_with(", "))]
    UnknownEvent(String),

    #[error("unsupported language: {0}")]
    UnknownLanguage(String),

    #[error("unknown starlark module: {0}")]
    UnknownModule(Utf8PathBuf),
}
