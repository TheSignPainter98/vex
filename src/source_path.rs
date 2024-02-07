use std::{fmt::Display, ops::Deref, sync::Arc};

use allocative::Allocative;
use camino::Utf8Path;
use dupe::Dupe;
use starlark::{starlark_simple_value, values::StarlarkValue};
use starlark_derive::{starlark_value, NoSerialize, ProvidesStaticType};

#[derive(Clone, Debug, Dupe)]
pub struct SourcePath {
    pub abs_path: Arc<Utf8Path>,
    pub pretty_path: PrettyPath,
}

impl SourcePath {
    pub fn new(path: &Utf8Path, base_dir: &Utf8Path) -> Self {
        Self {
            abs_path: path.into(),
            pretty_path: PrettyPath(
                path.strip_prefix(base_dir)
                    .expect("path not in base dir")
                    .into(),
            ),
        }
    }

    pub fn as_str(&self) -> &str {
        self.pretty_path.as_str()
    }
}

impl AsRef<str> for SourcePath {
    fn as_ref(&self) -> &str {
        self.pretty_path.as_str()
    }
}

impl Display for SourcePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.pretty_path.fmt(f)
    }
}

#[derive(
    Clone,
    Debug,
    Dupe,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Allocative,
    NoSerialize,
    ProvidesStaticType,
)]
pub struct PrettyPath(#[allocative(skip)] Arc<Utf8Path>);
starlark_simple_value!(PrettyPath);

impl PrettyPath {
    pub fn new(path: &Utf8Path) -> Self {
        Self(Arc::from(path))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&str> for PrettyPath {
    fn from(value: &str) -> Self {
        Self(Utf8Path::new(value).into())
    }
}

impl AsRef<Utf8Path> for PrettyPath {
    fn as_ref(&self) -> &Utf8Path {
        &self.0
    }
}

impl Deref for PrettyPath {
    type Target = Utf8Path;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl Display for PrettyPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[starlark_value(type = "Path")]
impl<'v> StarlarkValue<'v> for PrettyPath {} // TODO(kcza): override Eq to be more lenient to
                                             // string comparisons!
