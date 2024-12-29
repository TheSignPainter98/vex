use std::{
    collections::BTreeMap,
    fmt::Display,
    iter,
    str::FromStr,
    sync::{Arc, OnceLock},
};

use allocative::Allocative;
use dupe::Dupe;
use indoc::indoc;
use lazy_static::lazy_static;
use serde::{Deserialize as Deserialise, Serialize as Serialise};
use tree_sitter::{Language as TSLanguage, Query};

use crate::{error::Error, result::Result};

#[derive(
    Clone, Debug, Dupe, Allocative, PartialOrd, Ord, PartialEq, Eq, Hash, Deserialise, Serialise,
)]
#[serde(rename_all = "kebab-case")]
pub enum Language {
    Go,
    Python,
    Rust,
    External(Arc<str>),
}

impl Language {
    pub fn name(&self) -> &str {
        match self {
            Self::Go => "go",
            Self::Python => "python",
            Self::Rust => "rust",
            Self::External(l) => l,
        }
    }

    pub fn ts_language(&self) -> &TSLanguage {
        lazy_static! {
            static ref LANGUAGES: BTreeMap<Language, OnceLock<TSLanguage>> = Language::iter()
                .zip(iter::repeat_with(OnceLock::new))
                .collect();
        };

        LANGUAGES[self].get_or_init(|| match self {
            Self::Go => tree_sitter_go::language(),
            Self::Python => tree_sitter_python::language(),
            Self::Rust => tree_sitter_rust::language(),
            Self::External(_) => todo!(),
        })
    }

    pub fn ignore_query(&self) -> &Query {
        lazy_static! {
            static ref IGNORE_QUERIES: BTreeMap<Language, OnceLock<Query>> = Language::iter()
                .zip(iter::repeat_with(OnceLock::new))
                .collect();
        }

        IGNORE_QUERIES[self].get_or_init(|| {
            let raw = match self {
                Self::Go => indoc! {r#"
                    (
                        (comment) @marker (#match? @marker "^/[/*] *vex:ignore")
                        .
                        (_)? @ignore
                    )
                "#},
                Self::Python => indoc! {r#"
                    (
                        (comment) @marker (#match? @marker "^# *vex:ignore")
                        .
                        (_)? @ignore
                    )
                "#},
                Self::Rust => indoc! {r#"
                    (
                        (line_comment) @marker (#match? @marker "^// *vex:ignore")
                        .
                        (_)? @ignore
                    )
                "#},
                Self::External(_) => todo!(),
            };
            Query::new(self.ts_language(), raw).expect("internal error: ignore query invalid")
        })
    }
}

impl FromStr for Language {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let ret = match s {
            "go" => Self::Go,
            "python" => Self::Python,
            "rust" => Self::Rust,
            _ => Self::External(s.into()),
        };
        Ok(ret)
    }
}

impl Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.name().fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Range;

    use crate::{source_file::ParsedSourceFile, source_path::SourcePath};

    use super::*;

    #[test]
    fn str_conversion_roundtrip() -> anyhow::Result<()> {
        let languages = [
            Language::Go,
            Language::Python,
            Language::Rust,
            Language::External("lua".into()),
        ];
        for lang in languages {
            assert_eq!(lang, lang.name().parse()?);
        }
        Ok(())
    }

    #[test]
    #[allow(clippy::single_range_in_vec_init)]
    fn ignore_queries() {
        Test::language(Language::Go)
            .with_source(indoc! {r#"
                package main

                func main() {
                    // vex:ignore *
                    x := []int{
                        1,
                        2,
                        3,
                    }
                    // unrelated
                    z := 1;
                }
            "#})
            .ignores_ranges(&[32..102]);
        Test::language(Language::Python)
            .with_source(indoc! {r#"
                def main():
                    _ = _ # Placeholder line to avoid bug in Python grammar causing two consecutive body fields to be created.
                    # vex:ignore *
                    x = [
                        1,
                        2,
                        3,
                    ]
                    # unrelated
                    z = 1;
            "#})
            .ignores_ranges(&[127..190]);
        Test::language(Language::Rust)
            .with_source(indoc! {r#"
                fn main() {
                    // vex:ignore *
                    let x = [
                        1,
                        2,
                        3,
                    ];
                    // unrelated
                    let z = 1;
                }
            "#})
            .ignores_ranges(&[16..85]);

        // Test structs
        #[must_use]
        struct Test {
            language: Language,
            source: Option<&'static str>,
        }

        impl Test {
            fn language(language: Language) -> Self {
                Self {
                    language,
                    source: None,
                }
            }

            fn with_source(mut self, source: &'static str) -> Self {
                self.source = Some(source);
                self
            }

            fn ignores_ranges(self, ranges: &[Range<usize>]) {
                self.setup();

                let source_file = ParsedSourceFile::new_with_content(
                    SourcePath::new_in("test.file".into(), "".into()),
                    self.source.unwrap(),
                    self.language,
                )
                .unwrap();
                let ignore_markers = source_file.ignore_markers().unwrap();
                assert_eq!(ranges, ignore_markers.ignore_ranges().collect::<Vec<_>>());
            }

            fn setup(&self) {
                eprintln!("running {} test...", self.language);
            }
        }
    }
}
