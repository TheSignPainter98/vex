use std::{fmt::Display, iter, str::FromStr, sync::OnceLock};

use allocative::Allocative;
use clap::Subcommand;
use dupe::Dupe;
use enum_map::{Enum, EnumMap};
use lazy_static::lazy_static;
use serde::{Deserialize as Deserialise, Serialize as Serialise};
use strum::{EnumIter, IntoEnumIterator};
use tree_sitter::Language;

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
    C,
    Go,
    Python,
    Rust,
}

impl SupportedLanguage {
    pub fn name(&self) -> &'static str {
        match self {
            Self::C => "c",
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
            Self::C => tree_sitter_c::language(),
            Self::Go => tree_sitter_go::language(),
            Self::Python => tree_sitter_python::language(),
            Self::Rust => tree_sitter_rust::language(),
        })
    }
}

impl FromStr for SupportedLanguage {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "c" => Ok(Self::C),
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
mod test {
    use strum::IntoEnumIterator;

    use super::*;

    #[test]
    fn str_conversion_roundtrip() -> anyhow::Result<()> {
        for lang in SupportedLanguage::iter() {
            assert_eq!(lang, lang.name().parse()?);
        }
        Ok(())
    }
}
