use dupe::Dupe;

use crate::{
    error::Error,
    result::Result,
    source_path::SourcePath,
    supported_language::SupportedLanguage,
    trigger::{FilePattern, RawFilePattern},
};

#[derive(Debug)]
pub struct Associations(Vec<Association>);

impl Associations {
    pub fn base() -> Self {
        Self(
            [
                ("*.go", SupportedLanguage::Go),
                ("*.py", SupportedLanguage::Python),
                ("*.rs", SupportedLanguage::Rust),
            ]
            .into_iter()
            .map(|(pattern, language)| {
                let file_patterns = vec![RawFilePattern::new(pattern).compile().unwrap()];
                Association {
                    file_patterns,
                    in_base: true,
                    language,
                }
            })
            .collect(),
        )
    }

    pub fn insert(&mut self, file_patterns: Vec<FilePattern>, language: SupportedLanguage) {
        self.0.push(Association {
            file_patterns,
            in_base: false,
            language,
        })
    }

    pub fn get_language(&self, source_path: &SourcePath) -> Result<Option<SupportedLanguage>> {
        let mut language_matches = self.0.iter().rev().filter_map(|association| {
            let Association {
                file_patterns,
                in_base,
                language,
            } = association;
            if file_patterns
                .iter()
                .any(|pattern| pattern.matches(&source_path.pretty_path))
            {
                Some((*language, in_base))
            } else {
                None
            }
        });
        let Some((language, in_base)) = language_matches.next() else {
            return Ok(None);
        };
        if let Some((other_language, other_in_base)) = language_matches.next() {
            if language != other_language && in_base == other_in_base {
                let path = source_path.pretty_path.dupe();
                return Err(Error::AmbiguousLanguage {
                    path,
                    language,
                    other_language,
                });
            }
        }
        Ok(Some(language))
    }
}

#[derive(Debug)]
struct Association {
    file_patterns: Vec<FilePattern>,
    in_base: bool,
    language: SupportedLanguage,
}

#[cfg(test)]
mod test {
    use camino::Utf8PathBuf;

    use crate::context::Context;

    use super::*;

    #[test]
    fn base() {
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
                    Associations::base()
                        .get_language(&SourcePath::new_in(self.file.into(), "".into()))
                        .unwrap()
                        .unwrap()
                );
            }

            fn has_no_association(self) {
                self.setup();
                assert_eq!(
                    None,
                    Associations::base()
                        .get_language(&SourcePath::new_in(self.file.into(), "".into()))
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
            let mut associations = Associations::base();
            let pattern = RawFilePattern::new("*.shrödinger").compile().unwrap();
            associations.insert(vec![pattern.clone()], SupportedLanguage::Rust);
            associations.insert(vec![pattern], SupportedLanguage::Go);
            associations
        };
        associations
            .get_language(&SourcePath::new_in("foo.shrödinger".into(), "".into()))
            .unwrap_err();
    }

    #[test]
    fn override_base() {
        let associations = {
            let mut associations = Associations::base();
            let pattern = RawFilePattern::new("*.c").compile().unwrap();
            associations.insert(vec![pattern], SupportedLanguage::Python);
            associations
        };
        assert_eq!(
            associations
                .get_language(&SourcePath::new_in("actually_python.c".into(), "".into()))
                .unwrap()
                .unwrap(),
            SupportedLanguage::Python,
        );
    }

    #[test]
    fn nonambiguous_overlap() {
        let associations = {
            let mut associations = Associations::base();
            associations.insert(
                vec![RawFilePattern::new("*.rust-file").compile().unwrap()],
                SupportedLanguage::Rust,
            );
            associations.insert(
                vec![RawFilePattern::new("rust-files/*").compile().unwrap()],
                SupportedLanguage::Rust,
            );
            associations
        };
        assert_eq!(
            associations
                .get_language(&SourcePath::new_in(
                    "rust-files/some.rust-file".into(),
                    "".into()
                ))
                .unwrap()
                .unwrap(),
            SupportedLanguage::Rust,
        );
    }

    #[test]
    fn from_manifest() {
        let tempdir = tempfile::tempdir().unwrap();
        let tempdir_path = Utf8PathBuf::try_from(tempdir.path().to_owned()).unwrap();

        Context::init(&tempdir_path, false).unwrap();
        let associations = Context::acquire_in(&tempdir_path)
            .unwrap()
            .associations()
            .unwrap();
        let language = associations
            .get_language(&SourcePath::new_in("asdf.star".into(), "".into()))
            .unwrap()
            .unwrap();
        // Default manifest must add a *.star=python association.
        assert_eq!(SupportedLanguage::Python, language);
    }
}
