use anyhow::Context;
use camino::Utf8PathBuf;
use tokio::fs;
use tree_sitter::{Parser, Tree};

use crate::supported_language::SupportedLanguage;

pub struct SourceFile {
    pub path: Utf8PathBuf,
    pub content: String,
    pub tree: Tree,
    pub lang: SupportedLanguage,
}

impl SourceFile {
    pub async fn new(path: Utf8PathBuf, lang: SupportedLanguage) -> anyhow::Result<Self> {
        let content = fs::read_to_string(&path).await?;
        let tree = {
            let mut parser = Parser::new();
            parser
                .set_language(lang.ts_language())
                .with_context(|| format!("failed to load {} grammar", lang.name()))?;
            parser
                .parse(&content, None)
                .with_context(|| format!("failed to parse {path}"))?
        };

        Ok(Self {
            path,
            content,
            lang,
            tree,
        })
    }
}
