use clap::Subcommand;
use enum_map::Enum;
use strum::EnumIter;
use tree_sitter::Language;

#[derive(Copy, Clone, Debug, EnumIter, Subcommand, Enum)]
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
