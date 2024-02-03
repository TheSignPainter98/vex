use camino::Utf8PathBuf;
use joinery::JoinableIterator;
use strum::IntoEnumIterator;

use crate::scriptlets::{action::Action, event::EventType, PrettyPath};

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum Error {
    #[error("already inited in a parent directory {found_root}")]
    AlreadyInited { found_root: Utf8PathBuf },

    #[error("{0} declares empty query")]
    EmptyQuery(PrettyPath),

    #[error("import cycle detected: {}", .0.iter().join_with(" -> "))]
    ImportCycle(Vec<PrettyPath>),

    #[error("cannot find manifest, try running `vex init` in the project’s root")]
    ManifestNotFound,

    // #[error("{0} has no file name")]
    // NoFileName(Utf8PathBuf),
    #[error("{0} declares no init function")]
    NoInit(PrettyPath),

    #[error("{0} declares no callbacks")]
    NoCallbacks(PrettyPath),

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

    #[error("{what} unavailable while {}", .action.name())]
    Unavailable { what: &'static str, action: Action },

    #[error("unknown event '{0}', expected one of: {}", EventType::iter().join_with(", "))]
    UnknownEvent(String),

    #[error("unknown language '{0}'")]
    UnknownLanguage(String),
}
