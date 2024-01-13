use camino::Utf8PathBuf;

use crate::scriptlets::app_object::AttrName;

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum Error {
    #[error("already inited in a parent directory {found_root}")]
    AlreadyInited { found_root: Utf8PathBuf },

    #[error("import cycle detected: {}", .0.into_iter().map(ToString::to_string).collect::<Vec<_>>().join(", "))]
    ImportCycle(Vec<Utf8PathBuf>),

    #[error("cannot find manifest, try running `vex init` in the project’s root")]
    ManifestNotFound,

    // #[error("{0} has no file name")]
    // NoFileName(Utf8PathBuf),
    #[error("{0} declares init function")]
    NoInit(Utf8PathBuf),
    //
    // #[error("{0} declares no callbacks")]
    // NoCallbacks(Id),
    //
    // #[error("{0} declares no target language")]
    // NoLanguage(Id),
    //
    // #[error("{0} declares no trigger")]
    // NoTrigger(Id),
    #[error("{attr} is unavailable during {stage_name}")]
    Unavailable {
        attr: AttrName,
        stage_name: &'static str,
    },

    #[error("unknown event '{0}'")]
    UnknownEvent(String),

    #[error("unsupported language: {0}")]
    UnknownLanguage(String),

    #[error("unknown starlark module {requested} in /{vexes_dir}")]
    UnknownModule {
        vexes_dir: Utf8PathBuf,
        requested: Utf8PathBuf,
    },
}
