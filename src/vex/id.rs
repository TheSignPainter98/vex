use std::{fmt::Display, sync::Arc};

use camino::Utf8Path;
use dupe::Dupe;

#[derive(Clone, Debug, Dupe, PartialEq, Eq, PartialOrd, Ord)]
pub struct Id(Arc<str>);

impl Id {
    #[allow(unused)]
    pub fn new(path: &Utf8Path) -> Self {
        Self(path.file_stem().expect("no file stem").to_string().into())
    }

    #[allow(unused)]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
