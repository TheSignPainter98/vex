use std::fmt::Display;

use camino::Utf8Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Id(String);

impl Id {
    #[allow(unused)]
    pub fn new(path: &Utf8Path) -> Self {
        Self(path.file_stem().expect("no file stem").to_string())
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
