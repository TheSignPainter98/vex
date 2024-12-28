use std::{fmt::Display, ops::Deref};

use allocative::Allocative;
use camino::{Utf8Path, Utf8PathBuf};
use glob::Pattern;
use serde::{Deserialize, Serialize};

use crate::{error::Error, result::Result};

#[derive(Clone, Debug, Allocative)]
pub struct FilePattern(#[allocative(skip)] pub Pattern);

impl FilePattern {
    pub fn matches(&self, path: &Utf8Path) -> bool {
        self.0.matches(path.as_str())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct RawFilePattern<S>(S);

impl<S: AsRef<str>> RawFilePattern<S> {
    pub fn new(raw: S) -> Self {
        Self(raw)
    }

    pub fn compile(self) -> Result<FilePattern> {
        let pattern = {
            let mut pattern_buf = Utf8PathBuf::with_capacity("**/".len() + self.len() + "*".len());
            let original_start_index = if !self.starts_with('/') {
                pattern_buf.push("**");
                "**".len()
            } else {
                0
            };
            pattern_buf.push(self.deref());
            if self.ends_with('/') {
                pattern_buf.push("*");
            }
            Pattern::new(pattern_buf.as_str()).map_err(|cause| Error::Pattern {
                pattern: self.deref().into(),
                cause_pos_offset: original_start_index,
                cause,
            })?
        };
        Ok(FilePattern(pattern))
    }
}

impl<S: AsRef<str>> Deref for RawFilePattern<S> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl<S: AsRef<str>> Display for RawFilePattern<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.as_ref().fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use indoc::{formatdoc, indoc};

    use crate::{irritation::Irritation, vextest::VexTest};

    #[test]
    fn supported_language() {
        struct LanguageTest {
            language: &'static str,
            query: &'static str,
            files: &'static [LanguageTestFile],
        }
        struct LanguageTestFile {
            path: &'static str,
            content: &'static str,
        }

        let language_tests = [
            LanguageTest {
                language: "go",
                query: "(source_file)",
                files: &[LanguageTestFile {
                    path: "main.go",
                    content: indoc! {r#"
                        package main

                        import "fmt"

                        func main() {
                            fmt.Println("Hello, world!")
                        }
                    "#},
                }],
            },
            LanguageTest {
                language: "python",
                query: "(module)",
                files: &[LanguageTestFile {
                    path: "main.py",
                    content: indoc! {r#"
                        def main():
                            print('Hello, world!')

                        if __name__ == '__main__':
                            main()
                    "#},
                }],
            },
            LanguageTest {
                language: "rust",
                query: "(source_file)",
                files: &[LanguageTestFile {
                    path: "src/main.rs",
                    content: indoc! {r#"
                        fn main() {
                            println!("Hello, world!");
                        }
                    "#},
                }],
            },
        ];
        for language_test in &language_tests {
            let mut test = VexTest::new(language_test.language).with_scriptlet(
                "vexes/test.star",
                formatdoc! {
                    r#"
                        def init():
                            vex.observe('open_project', on_open_project)

                        def on_open_project(event):
                            vex.search(
                                '{language}',
                                '{query}',
                                on_match,
                            )

                        def on_match(event):
                            vex.warn('test', 'language={language}: opened and matched %s' % event.path)
                    "#,
                    language = language_test.language,
                    query = language_test.query,
                },
            );
            for other_language_test in &language_tests {
                for file in other_language_test.files {
                    test = test.with_source_file(file.path, file.content);
                }
            }
            test = test.with_source_file(
                "file-with.unknown-extension",
                "I appear to have burst into flames.",
            );

            let run = test.try_run().unwrap();
            assert_eq!(
                language_tests
                    .iter()
                    .map(|lt| lt.files.len() as u64)
                    .sum::<u64>(),
                run.num_files_scanned,
                "wrong number of files scanned"
            );
            assert_eq!(
                language_test.files.len(),
                run.irritations.len(),
                "wrong number of irritations lodged: got {:?}",
                run.irritations
                    .iter()
                    .map(Irritation::to_string)
                    .collect::<Vec<_>>(),
            );
        }
    }

    #[test]
    fn malformed_query() {
        VexTest::new("empty")
            .with_scriptlet(
                "vexes/test.star",
                indoc! {r#"
                    def init():
                        vex.observe('open_project', on_open_project)

                    def on_open_project(event):
                        vex.search(
                            'rust',
                            '',
                            on_match,
                        )

                    def on_match(event):
                        pass
                "#},
            )
            .returns_error(r"query is empty");
        VexTest::new("comment only")
            .with_scriptlet(
                "vexes/test.star",
                indoc! {r#"
                    def init():
                        vex.observe('open_project', on_open_project)

                    def on_open_project(event):
                        vex.search(
                            'rust',
                            '; this query contains nothing!',
                            on_match,
                        )

                    def on_match(event):
                        pass
                "#},
            )
            .returns_error(r"query is empty");
        VexTest::new("syntax-error")
            .with_scriptlet(
                "vexes/test.star",
                indoc! {r#"
                    def init():
                        vex.observe('open_project', on_open_project)

                    def on_open_project(event):
                        vex.search(
                            'rust',
                            '(binary_expression', # Missing closing bracket
                            on_match,
                        )

                    def on_match(event):
                        pass
                "#},
            )
            .returns_error("Invalid syntax");
    }

    #[test]
    fn path_globbing() {
        #[must_use]
        struct PathGlobTest {
            name: &'static str,
            root_dir: &'static str,
            test_paths: &'static [&'static str],
            path_pattern: Option<&'static str>,
            expected_matches: Option<&'static [&'static str]>,
        }

        impl PathGlobTest {
            fn new(
                name: &'static str,
                root_dir: &'static str,
                test_paths: &'static [&'static str],
            ) -> Self {
                Self {
                    name,
                    root_dir,
                    test_paths,
                    path_pattern: None,
                    expected_matches: None,
                }
            }

            fn path_pattern(mut self, path_pattern: &'static str) -> Self {
                self.path_pattern = Some(path_pattern);
                self
            }

            fn matches(mut self, expected_matches: &'static [&'static str]) {
                self.expected_matches = Some(expected_matches);
                self.run()
            }

            fn run(self) {
                eprintln!("running test {}...", self.name);

                let path_pattern = self.path_pattern.unwrap();
                let pattern = RawFilePattern::new(path_pattern).compile().unwrap();
                let matches = self
                    .test_paths
                    .iter()
                    .filter(|test_path| {
                        pattern.matches(&Utf8PathBuf::from(self.root_dir).join(test_path))
                    })
                    .copied()
                    .collect::<Vec<_>>();
                assert_eq!(matches, self.expected_matches.unwrap(),);
            }
        }

        let root_dir = "/some-project";
        let test_paths = &[
            "/foo.rs",
            "/bar.rs",
            "/foo",
            "/bar/foo",
            "/bar/foo.rs",
            "/baz/bar/foo.rs",
            "/qux/baz/bar/foo.rs",
            "/qux/baz/bar/bar.rs",
            "/qux/baz/bar/baz.go",
        ];

        PathGlobTest::new("empty", root_dir, test_paths)
            .path_pattern("")
            .matches(test_paths);

        // File filter tests.
        PathGlobTest::new("nonexistent-file", root_dir, test_paths)
            .path_pattern("i_do_not_exist.rs")
            .matches(&[]);
        PathGlobTest::new("full-file-name", root_dir, test_paths)
            .path_pattern("foo.rs")
            .matches(&[
                "/foo.rs",
                "/bar/foo.rs",
                "/baz/bar/foo.rs",
                "/qux/baz/bar/foo.rs",
            ]);
        PathGlobTest::new("full-file-name-absolute", root_dir, test_paths)
            .path_pattern("/foo.rs")
            .matches(&["/foo.rs"]);
        PathGlobTest::new("file-stem", root_dir, test_paths)
            .path_pattern("foo")
            .matches(&["/foo", "/bar/foo"]);
        PathGlobTest::new("file-stem-absolute", root_dir, test_paths)
            .path_pattern("/foo")
            .matches(&["/foo"]);
        PathGlobTest::new("file-glob", root_dir, test_paths)
            .path_pattern("*.rs")
            .matches(&[
                "/foo.rs",
                "/bar.rs",
                "/bar/foo.rs",
                "/baz/bar/foo.rs",
                "/qux/baz/bar/foo.rs",
                "/qux/baz/bar/bar.rs",
            ]);

        // Dir filter tests.
        PathGlobTest::new("nonexistent-dir", root_dir, test_paths)
            .path_pattern("i_do_not_exist/")
            .matches(&[]);
        PathGlobTest::new("dir", root_dir, test_paths)
            .path_pattern("bar/")
            .matches(&[
                "/bar/foo",
                "/bar/foo.rs",
                "/baz/bar/foo.rs",
                "/qux/baz/bar/foo.rs",
                "/qux/baz/bar/bar.rs",
                "/qux/baz/bar/baz.go",
            ]);
        PathGlobTest::new("dir-absolute", root_dir, test_paths)
            .path_pattern("/bar/")
            .matches(&["/bar/foo", "/bar/foo.rs"]);
        PathGlobTest::new("root", root_dir, test_paths)
            .path_pattern("/")
            .matches(test_paths);
        PathGlobTest::new("multi-part", root_dir, test_paths)
            .path_pattern("baz/bar/")
            .matches(&[
                "/baz/bar/foo.rs",
                "/qux/baz/bar/foo.rs",
                "/qux/baz/bar/bar.rs",
                "/qux/baz/bar/baz.go",
            ]);
        PathGlobTest::new("dir-glob", root_dir, test_paths)
            .path_pattern("qux/**/baz/**")
            .matches(&[
                "/qux/baz/bar/foo.rs",
                "/qux/baz/bar/bar.rs",
                "/qux/baz/bar/baz.go",
            ]);
        PathGlobTest::new("dir-glob-with-file", root_dir, test_paths)
            .path_pattern("qux/**/foo.rs")
            .matches(&["/qux/baz/bar/foo.rs"]);
    }

    #[test]
    fn malformed_glob() {
        let pattern = "[".to_string();
        let err = RawFilePattern::new(&pattern).compile().unwrap_err();
        assert_eq!(
            r#"cannot compile "[": invalid range pattern at position 1"#,
            err.to_string()
        );
    }
}
