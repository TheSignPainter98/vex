use std::fs;

use dupe::Dupe;
use tree_sitter::{Parser, Tree};

use crate::{
    error::{Error, IOAction},
    result::Result,
    source_path::SourcePath,
    supported_language::SupportedLanguage,
    trigger::{Trigger, TriggerCause},
};

#[derive(Debug)]
pub struct SourceFile {
    path: SourcePath,
    language: SupportedLanguage,
}

impl SourceFile {
    pub fn new(path: SourcePath) -> Result<Self> {
        let Some(extension) = path.abs_path.extension() else {
            return Err(Error::NoExtension(path.pretty_path.dupe()));
        };
        let language = SupportedLanguage::try_from_extension(extension)?;
        let path = path.dupe();
        Ok(Self { path, language })
    }

    pub fn parse(&self) -> Result<ParsedSourceFile> {
        let content =
            fs::read_to_string(self.path.abs_path.as_str()).map_err(|cause| Error::IO {
                path: self.path.pretty_path.dupe(),
                action: IOAction::Read,
                cause,
            })?;
        let tree = {
            let mut parser = Parser::new();
            parser
                .set_language(self.language.ts_language())
                .map_err(Error::Language)?;
            parser
                .parse(&content, None)
                .expect("unexpected parser failure")
        };
        let path = self.path.dupe();
        let language = self.language;
        Ok(ParsedSourceFile {
            path,
            content,
            tree,
            language,
        })
    }
}

impl TriggerCause for SourceFile {
    fn matches(&self, trigger: &Trigger) -> bool {
        if let Some(content_trigger) = trigger.content_trigger.as_ref() {
            if content_trigger.language != self.language {
                return false;
            }
        }

        trigger.path_patterns.is_empty()
            || trigger
                .path_patterns
                .iter()
                .any(|trigger| trigger.matches(&self.path.pretty_path))
    }
}

#[derive(Debug)]
pub struct ParsedSourceFile {
    pub path: SourcePath,
    pub content: String,
    pub language: SupportedLanguage,
    pub tree: Tree,
}

impl PartialEq for ParsedSourceFile {
    fn eq(&self, other: &Self) -> bool {
        (&self.path, &self.content, self.language) == (&other.path, &other.content, other.language)
    }
}

impl Eq for ParsedSourceFile {}
