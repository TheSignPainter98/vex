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

pub struct SourceFile {
    pub path: SourcePath,
    content: String,
}

impl SourceFile {
    pub fn new(path: SourcePath) -> Result<Self> {
        let content = fs::read_to_string(path.abs_path.as_str()).map_err(|cause| Error::IO {
            path: path.pretty_path.dupe(),
            action: IOAction::Read,
            cause,
        })?;
        Ok(Self { path, content })
    }

    pub fn parse(&self) -> Result<ParsedSourceFile<'_>> {
        let Some(extension) = self.path.abs_path.extension() else {
            return Err(Error::NoExtension(self.path.pretty_path.dupe()));
        };
        let language = SupportedLanguage::try_from_extension(extension)?;
        let tree = {
            let mut parser = Parser::new();
            parser
                .set_language(language.ts_language())
                .map_err(Error::Language)?;
            parser
                .parse(&self.content, None)
                .expect("unexpected parser failure")
        };
        Ok(ParsedSourceFile {
            path: self.path.dupe(),
            content: &self.content,
            tree,
            language,
        })
    }
}

impl TriggerCause for SourceFile {
    fn matches(&self, trigger: &Trigger) -> bool {
        if trigger.path_patterns.is_empty() {
            return true;
        }
        trigger
            .path_patterns
            .iter()
            .any(|pattern| pattern.matches_path(&self.path.pretty_path))
    }
}

#[derive(Debug)]
pub struct ParsedSourceFile<'c> {
    pub path: SourcePath,
    pub content: &'c str,
    pub language: SupportedLanguage,
    pub tree: Tree,
}

impl TriggerCause for ParsedSourceFile<'_> {
    fn matches(&self, trigger: &Trigger) -> bool {
        if let Some(content_trigger) = trigger.content_trigger.as_ref() {
            if !content_trigger.matches(self) {
                return false;
            }
        }

        trigger.path_patterns.is_empty()
            || trigger
                .path_patterns
                .iter()
                .any(|trigger| trigger.matches_path(&self.path.pretty_path))
    }
}

impl PartialEq for ParsedSourceFile<'_> {
    fn eq(&self, other: &Self) -> bool {
        (&self.path, &self.content, self.language) == (&other.path, &other.content, other.language)
    }
}

impl Eq for ParsedSourceFile<'_> {}
