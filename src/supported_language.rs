use std::{fmt::Display, str::FromStr};

use allocative::Allocative;
use clap::Subcommand;
use dupe::Dupe;
use enum_map::Enum;
use strum::EnumIter;
use tree_sitter::Language;

use crate::{error::Error, result::Result};

#[derive(Copy, Clone, Debug, Dupe, EnumIter, Subcommand, Enum, Allocative, PartialEq, Eq)]
pub enum SupportedLanguage {
    Go,
    Rust,
}

impl SupportedLanguage {
    pub fn name(&self) -> &'static str {
        use SupportedLanguage::*;
        match self {
            Go => "go",
            Rust => "rust",
        }
    }

    pub fn try_from_extension(extension: &str) -> Result<Self> {
        match extension {
            "go" => Ok(Self::Go),
            "rs" => Ok(Self::Rust),
            _ => Err(Error::UnknownExtension(extension.into())),
        }
    }

    pub fn ts_language(&self) -> Language {
        use SupportedLanguage::*;
        match self {
            Rust => tree_sitter_rust::language(),
            Go => tree_sitter_go::language(),
        }
    }
}

impl FromStr for SupportedLanguage {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "rust" => Ok(Self::Rust),
            "go" => Ok(Self::Go),
            _ => Err(Error::UnknownLanguage(s.to_string())),
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
