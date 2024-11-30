use std::{fmt::Display, iter, str::FromStr, sync::OnceLock};

use allocative::Allocative;
use clap::Subcommand;
use dupe::Dupe;
use enum_map::{Enum, EnumMap};
use indoc::indoc;
use lazy_static::lazy_static;
use serde::{Deserialize as Deserialise, Serialize as Serialise};
use strum::{EnumIter, IntoEnumIterator};
use tree_sitter::{Language, Query};

use crate::{error::Error, result::Result};

#[derive(
    Copy,
    Clone,
    Debug,
    Dupe,
    EnumIter,
    Subcommand,
    Enum,
    Allocative,
    PartialEq,
    Eq,
    Hash,
    Deserialise,
    Serialise,
)]
#[serde(rename_all = "kebab-case")]
pub enum SupportedLanguage {
    Go,
    Python,
    Rust,
}

impl SupportedLanguage {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Go => "go",
            Self::Python => "python",
            Self::Rust => "rust",
        }
    }

    pub fn ts_language(&self) -> &Language {
        lazy_static! {
            static ref LANGUAGES: EnumMap<SupportedLanguage, OnceLock<Language>> =
                SupportedLanguage::iter()
                    .zip(iter::repeat_with(OnceLock::new))
                    .collect();
        };

        LANGUAGES[*self].get_or_init(|| match self {
            Self::Go => tree_sitter_go::language(),
            Self::Python => tree_sitter_python::language(),
            Self::Rust => tree_sitter_rust::language(),
        })
    }

    pub fn ignore_query(&self) -> &Query {
        lazy_static! {
            static ref IGNORE_QUERIES: EnumMap<SupportedLanguage, OnceLock<Query>> =
                SupportedLanguage::iter()
                    .zip(iter::repeat_with(OnceLock::new))
                    .collect();
        }

        IGNORE_QUERIES[*self].get_or_init(|| {
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
            };
            Query::new(self.ts_language(), raw).expect("internal error: ignore query invalid")
        })
    }
}

impl FromStr for SupportedLanguage {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "go" => Ok(Self::Go),
            "python" => Ok(Self::Python),
            "rust" => Ok(Self::Rust),
            _ => Err(Error::UnsupportedLanguage(s.to_string())),
        }
    }
}

impl Display for SupportedLanguage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.name().fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Range;

    use strum::IntoEnumIterator;

    use crate::{source_file::ParsedSourceFile, source_path::SourcePath};

    use super::*;

    #[test]
    fn str_conversion_roundtrip() -> anyhow::Result<()> {
        for lang in SupportedLanguage::iter() {
            assert_eq!(lang, lang.name().parse()?);
        }
        Ok(())
    }

    #[test]
    #[allow(clippy::single_range_in_vec_init)]
    fn ignore_queries() {
        Test::language(SupportedLanguage::Go)
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
        Test::language(SupportedLanguage::Python)
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
        Test::language(SupportedLanguage::Rust)
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
            language: SupportedLanguage,
            source: Option<&'static str>,
        }

        impl Test {
            fn language(language: SupportedLanguage) -> Self {
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
