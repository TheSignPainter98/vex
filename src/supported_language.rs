use std::{fmt::Display, str::FromStr};

use allocative::Allocative;
use clap::Subcommand;
use enum_map::Enum;
use strum::EnumIter;
use tree_sitter::Language;

use crate::error::Error;

#[derive(Copy, Clone, Debug, EnumIter, Subcommand, Enum, Allocative, PartialEq, Eq)]
pub enum SupportedLanguage {
    Rust,
}

impl SupportedLanguage {
    pub fn name(&self) -> &'static str {
        use SupportedLanguage::*;
        match self {
            Rust => "rust",
        }
    }

    pub fn try_from_extension(extension: &str) -> Option<Self> {
        match extension {
            "rs" => Some(Self::Rust),
            _ => None,
        }
    }

    pub fn ts_language(&self) -> Language {
        use SupportedLanguage::*;
        match self {
            Rust => tree_sitter_rust::language(),
        }
    }
}

impl FromStr for SupportedLanguage {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "rust" => Ok(Self::Rust),
            _ => Err(Error::UnknownLanguage(s.to_string()).into()),
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
