use camino::Utf8PathBuf;

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum Error {
    #[error("already inited in a parent directory {found_root}")]
    AlreadyInited { found_root: Utf8PathBuf },

    #[error("cannot find manifest, try running `vex init` in the project’s root")]
    ManifestNotFound,

    // #[error("{0} has no file extension")]
    // MissingExtension(Utf8PathBuf),
    #[error("{0} missing file name")]
    MissingFileName(Utf8PathBuf),

    #[error("invalid toml type: expected {expected} but got {actual}")]
    TomlTypeError {
        name: String,
        expected: &'static str,
        actual: &'static str,
    },

    #[error("unsupported languages: {}", .0.join(", "))]
    UnknownLanguages(Vec<String>),
    // #[error("unknown file extension: {0}")]
    // UnknownFileExtension(String),
    // #[error("{0} are not yet supported")]
    // Unsupported(&'static str),
}
