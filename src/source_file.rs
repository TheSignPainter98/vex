use std::fs;

use allocative::Allocative;
use dupe::Dupe;
use log::{info, log_enabled};
use tree_sitter::{Parser, Tree};

use crate::{
    error::{Error, IOAction},
    result::Result,
    source_path::SourcePath,
    supported_language::SupportedLanguage,
};

#[derive(Debug)]
pub struct SourceFile {
    path: SourcePath,
    language: Option<SupportedLanguage>,
}

impl SourceFile {
    pub fn new(path: SourcePath, language: Option<SupportedLanguage>) -> Result<Self> {
        let path = path.dupe();
        Ok(Self { path, language })
    }

    pub fn path(&self) -> &SourcePath {
        &self.path
    }

    pub fn language(&self) -> Option<SupportedLanguage> {
        self.language
    }

    pub fn parse(&self) -> Result<ParsedSourceFile> {
        if log_enabled!(log::Level::Info) {
            info!("parsing {}", self.path);
        }
        let content =
            fs::read_to_string(self.path.abs_path.as_str()).map_err(|cause| Error::IO {
                path: self.path.pretty_path.dupe(),
                action: IOAction::Read,
                cause,
            })?;
        let Some(language) = self.language else {
            return Err(Error::NoKnownLanguage(self.path.pretty_path.dupe()));
        };
        ParsedSourceFile::new_with_content(self.path.dupe(), content, language)
    }
}

#[derive(Clone, Debug, Allocative)]
pub struct ParsedSourceFile {
    pub path: SourcePath,
    pub content: String,
    pub language: SupportedLanguage,
    #[allocative(skip)]
    pub tree: Tree,
}

impl ParsedSourceFile {
    pub fn new_with_content(
        path: SourcePath,
        content: impl Into<String>,
        language: SupportedLanguage,
    ) -> Result<Self> {
        let content = content.into();

        let tree = {
            let mut parser = Parser::new();
            parser.set_language(language.ts_language())?;
            let tree = parser
                .parse(&content, None)
                .expect("unexpected parser failure");
            if tree.root_node().has_error() {
                return Err(Error::UnparseableAsLanguage {
                    path: path.pretty_path.dupe(),
                    language,
                });
            }
            tree
        };
        Ok(ParsedSourceFile {
            path,
            content,
            tree,
            language,
        })
    }
}

impl PartialEq for ParsedSourceFile {
    fn eq(&self, other: &Self) -> bool {
        (&self.path, &self.content, self.language) == (&other.path, &other.content, other.language)
    }
}

impl Eq for ParsedSourceFile {}
