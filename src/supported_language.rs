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

    pub fn try_from_extension(extension: &str) -> Result<Self> {
        match extension {
            "c" | "h" => Ok(Self::C),
            "go" => Ok(Self::Go),
            "py" => Ok(Self::Python),
            "rs" => Ok(Self::Rust),
            _ => Err(Error::UnknownExtension(extension.into())),
        }
    }

    pub fn ts_language(&self) -> Language {
        match self {
            Self::C => tree_sitter_c::language(),
            Self::Go => tree_sitter_go::language(),
            Self::Python => tree_sitter_python::language(),
            Self::Rust => tree_sitter_rust::language(),
        }
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
