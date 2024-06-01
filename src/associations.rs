use dupe::Dupe;

use crate::{
    error::Error,
    result::Result,
    source_path::SourcePath,
    supported_language::SupportedLanguage,
    trigger::{FilePattern, RawFilePattern},
};

#[derive(Debug)]
pub struct Associations(Vec<(Vec<FilePattern>, SupportedLanguage)>);

impl Associations {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, file_patterns: Vec<FilePattern>, language: SupportedLanguage) {
        self.0.push((file_patterns, language))
    }

    pub fn get_language(&self, source_path: &SourcePath) -> Result<Option<SupportedLanguage>> {
        let mut language_matches = self.0.iter().filter_map(|(patterns, language)| {
            if patterns
                .iter()
                .any(|pattern| pattern.matches(&source_path.pretty_path))
            {
                Some(*language)
            } else {
                None
            }
        });
        let Some(language) = language_matches.next() else {
            return Ok(None);
        };
        if let Some(other_language) = language_matches.next() {
            let path = source_path.pretty_path.dupe();
            return Err(Error::AmbiguousLanguage {
                path,
                language,
                other_language,
            });
        }
        Ok(Some(language))
    }
}

impl Default for Associations {
    fn default() -> Self {
        Self(
            [
                ("*.[ch]", SupportedLanguage::C),
                ("*.go", SupportedLanguage::Go),
                ("*.py", SupportedLanguage::Python),
                ("*.rs", SupportedLanguage::Rust),
            ]
            .into_iter()
            .map(|(pattern, language)| {
                (
                    vec![RawFilePattern::new(pattern).compile().unwrap()],
                    language,
                )
            })
            .collect(),
        )
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn default() {
        Test::file("foo/bar.c").has_association(SupportedLanguage::C);
        Test::file("foo/bar.h").has_association(SupportedLanguage::C);
        Test::file("foo/bar.go").has_association(SupportedLanguage::Go);
        Test::file("foo/bar.py").has_association(SupportedLanguage::Python);
        Test::file("foo/bar.rs").has_association(SupportedLanguage::Rust);
        // *.star=python is an extra association, not a base one.
        Test::file("foo/bar.star").has_no_association();

        // Test structs
        struct Test {
            file: &'static str,
        }

        impl Test {
            fn file(file: &'static str) -> Self {
                Self { file }
            }

            fn has_association(self, expected_language: SupportedLanguage) {
                self.setup();
                assert_eq!(
                    expected_language,
                    Associations::default()
                        .get_language(&SourcePath::new_in(self.file.into(), "".into(),))
                        .unwrap()
                        .unwrap()
                );
            }

            fn has_no_association(self) {
                self.setup();
                assert_eq!(
                    None,
                    Associations::default()
                        .get_language(&SourcePath::new_in(self.file.into(), "".into(),))
                        .unwrap()
                )
            }

            fn setup(&self) {
                eprintln!("testing {}...", self.file);
            }
        }
    }

    #[test]
    fn ambiguous() {
        let associations = {
            let mut associations = Associations::default();
            let pattern = RawFilePattern::new("*.ambiguous").compile().unwrap();
            associations.insert(vec![pattern.clone()], SupportedLanguage::Rust);
            associations.insert(vec![pattern], SupportedLanguage::C);
            associations
        };
        associations
            .get_language(&SourcePath::new_in("foo.ambiguous".into(), "".into()))
            .unwrap_err();
    }
}
