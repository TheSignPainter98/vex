use std::{fmt::Display, str::FromStr, sync::Arc};

use allocative::Allocative;
use dupe::Dupe;
use serde::{Deserialize as Deserialise, Serialize as Serialise};

use crate::{error::Error, result::Result};

#[derive(
    Clone, Debug, Dupe, Allocative, PartialOrd, Ord, PartialEq, Eq, Hash, Deserialise, Serialise,
)]
#[serde(rename_all = "kebab-case")]
pub enum Language {
    Go,
    Python,
    Rust,
    #[serde(untagged)]
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

    pub fn is_builtin(&self) -> bool {
        !matches!(self, Self::External(_))
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

    use indoc::indoc;

    use crate::{
        context::{Context, Manifest},
        source_file::ParsedSourceFile,
        source_path::SourcePath,
    };

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
    fn is_builtin() {
        Assert::language(Language::Go).considered_builtin();
        Assert::language(Language::Python).considered_builtin();
        Assert::language(Language::Rust).considered_builtin();
        Assert::language(Language::External(Arc::from("lua"))).considered_not_builtin();

        // test types.
        struct Assert {
            language: Language,
        }

        impl Assert {
            pub fn language(language: Language) -> Self {
                Self { language }
            }

            pub fn considered_builtin(self) {
                assert!(self.language.is_builtin());
            }

            pub fn considered_not_builtin(self) {
                assert!(!self.language.is_builtin());
            }
        }
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

                let ctx = Context::new_with_manifest("test-path".into(), Manifest::default());
                let source_file = ParsedSourceFile::new_with_content(
                    SourcePath::new_in("test.file".into(), "".into()),
                    self.source.unwrap(),
                    ctx.language_data(&self.language).unwrap().unwrap().dupe(),
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
