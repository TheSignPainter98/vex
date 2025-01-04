use dupe::Dupe;

use crate::{
    error::Error,
    language::Language,
    result::Result,
    source_path::SourcePath,
    trigger::{FilePattern, RawFilePattern},
};

#[derive(Debug)]
pub struct Associations(Vec<Association>);

impl Associations {
    pub fn base() -> Self {
        Self(
            [
                ("*.go", Language::Go),
                ("*.py", Language::Python),
                ("*.rs", Language::Rust),
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

    pub fn insert(&mut self, file_patterns: Vec<FilePattern>, language: Language) {
        self.0.push(Association {
            file_patterns,
            in_base: false,
            language,
        })
    }

    pub fn get_language(&self, source_path: &SourcePath) -> Result<Option<&Language>> {
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
                Some((language, in_base))
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
                    language: language.dupe(),
                    other_language: other_language.dupe(),
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
    language: Language,
}

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;

    use crate::context::Context;

    use super::*;

    #[test]
    fn base() {
        Test::file("foo/bar.go").has_association(Language::Go);
        Test::file("foo/bar.py").has_association(Language::Python);
        Test::file("foo/bar.rs").has_association(Language::Rust);
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

            fn has_association(self, expected_language: Language) {
                self.setup();
                assert_eq!(
                    &expected_language,
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
            associations.insert(vec![pattern.clone()], Language::Rust);
            associations.insert(vec![pattern], Language::Go);
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
            associations.insert(vec![pattern], Language::Python);
            associations
        };
        assert_eq!(
            associations
                .get_language(&SourcePath::new_in("actually_python.c".into(), "".into()))
                .unwrap()
                .unwrap(),
            &Language::Python,
        );
    }

    #[test]
    fn nonambiguous_overlap() {
        let associations = {
            let mut associations = Associations::base();
            associations.insert(
                vec![RawFilePattern::new("*.rust-file").compile().unwrap()],
                Language::Rust,
            );
            associations.insert(
                vec![RawFilePattern::new("rust-files/*").compile().unwrap()],
                Language::Rust,
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
            &Language::Rust,
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
        assert_eq!(language, &Language::Python);
    }
}
