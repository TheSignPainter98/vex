use std::fs;

use dupe::Dupe;
use tree_sitter::{Parser, Tree};

use crate::{
    error::{Error, IOAction},
    result::Result,
    source_path::SourcePath,
    supported_language::SupportedLanguage,
};

#[derive(Debug)]
pub struct SourceFile {
    pub path: SourcePath,
    pub content: String,
    pub tree: Tree,
    pub lang: SupportedLanguage,
}

impl SourceFile {
    pub fn load(path: SourcePath) -> Result<Self> {
        let Some(extension) = path.abs_path.extension() else {
            return Err(Error::NoExtension(path.pretty_path.dupe()));
        };
        let lang = SupportedLanguage::try_from_extension(extension)?;
        let content = fs::read_to_string(path.abs_path.as_ref()).map_err(|cause| Error::IO {
            path: path.pretty_path.dupe(),
            action: IOAction::Read,
            cause,
        })?;
        let tree = {
            let mut parser = Parser::new();
            parser
                .set_language(lang.ts_language())
                .map_err(Error::Language)?;
            parser
                .parse(&content, None)
                .expect("unexpected parser failure")
        };
        Ok(Self {
            path,
            content,
            lang,
            tree,
        })
    }
}

impl PartialEq for SourceFile {
    fn eq(&self, other: &Self) -> bool {
        (&self.path, &self.content, self.lang) == (&other.path, &other.content, other.lang)
    }
}

impl Eq for SourceFile {}
