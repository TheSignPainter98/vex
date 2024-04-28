use std::{fmt::Display, ops::Deref, sync::Arc};

use allocative::Allocative;
use camino::{Utf8Path, Utf8PathBuf};
use dupe::Dupe;
use glob::{MatchOptions, Pattern};
use serde::{Deserialize, Serialize};
use starlark_derive::Trace;
use tree_sitter::Query;

use crate::{error::Error, result::Result, supported_language::SupportedLanguage};

pub trait TriggerCause {
    fn matches(&self, trigger: &Trigger) -> bool;
}

#[derive(Debug, Allocative)]
pub struct Trigger {
    pub id: Option<TriggerId>,
    pub content_trigger: Option<ContentTrigger>,
    pub path_patterns: Vec<FilePattern>,
}

impl Default for Trigger {
    fn default() -> Self {
        Self {
            id: None,
            content_trigger: None,
            path_patterns: Vec::with_capacity(0),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Dupe, Allocative, Trace)]
pub struct TriggerId(Arc<str>);

impl TriggerId {
    pub fn new(id: &str) -> Self {
        Self(Arc::from(id))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }
}

#[derive(Debug, Allocative)]
pub struct ContentTrigger {
    pub language: SupportedLanguage,
    #[allocative(skip)]
    pub query: Option<Query>,
}

#[derive(Debug, Allocative)]
pub struct FilePattern(#[allocative(skip)] Pattern);

impl FilePattern {
    pub fn matches(&self, path: &Utf8Path) -> bool {
        self.0.matches_path_with(
            path.as_std_path(),
            MatchOptions {
                case_sensitive: true,
                require_literal_separator: false,
                require_literal_leading_dot: true,
            },
        )
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct RawFilePattern<S>(S);

impl<S: AsRef<str>> RawFilePattern<S> {
    pub fn new(raw: S) -> Self {
        Self(raw)
    }

    pub fn compile(self, project_root: impl AsRef<Utf8Path>) -> Result<FilePattern> {
        let project_root = project_root.as_ref();
        let pattern = {
            let mut pattern_buf = Utf8PathBuf::with_capacity(
                project_root.as_str().len() + "**/".len() + self.len() + "*".len(),
            );
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
mod test {
    use super::*;

    use camino::Utf8PathBuf;
    use indoc::{formatdoc, indoc};

    use crate::{irritation::Irritation, vextest::VexTest};

    #[test]
    fn supported_language() {
        struct LanguageTest {
            language: &'static str,
            main_path: &'static str,
            main_content: &'static str,
        }

        let language_tests = [
            LanguageTest {
                language: "go",
                main_path: "main.go",
                main_content: indoc! {r#"
                    package main

                    import "fmt"

                    func main() {
                        fmt.Println("Hello, world!")
                    }
                "#},
            },
            LanguageTest {
                language: "rust",
                main_path: "src/main.rs",
                main_content: indoc! {r#"
                    fn main() {
                        println!("Hello, world!");
                    }
                "#},
            },
        ];
        for language_test in &language_tests {
            let mut test = VexTest::new(language_test.language).with_scriptlet(
                "vexes/test.star",
                formatdoc! {
                    r#"
                        def init():
                            vex.add_trigger(language='{language}')
                            vex.observe('open_file', on_open_file)
                            vex.observe('close_file', on_close_file)

                        def on_open_file(event):
                            vex.warn('language={language}: opened %s' % event.path)

                        def on_close_file(event):
                            vex.warn('language={language}: closed %s' % event.path)
                    "#,
                    language = language_test.language,
                },
            );
            for other_language_test in &language_tests {
                test = test.with_source_file(
                    other_language_test.main_path,
                    other_language_test.main_content,
                );
            }

            let run_data = test.try_run().unwrap();
            assert_eq!(
                language_tests.len(),
                run_data.num_files_scanned,
                "wrong number of files scanned"
            );
            assert_eq!(
                2,
                run_data.irritations.len(),
                "wrong number of irritations lodged: got {:?}",
                run_data
                    .irritations
                    .iter()
                    .map(Irritation::to_string)
                    .collect::<Vec<_>>(),
            );
        }
    }

    #[test]
    fn unsupported_language() {
        VexTest::new("unsupported-language")
            .with_scriptlet(
                "vexes/test.star",
                indoc! {r#"
                    def init():
                        vex.add_trigger(
                            language='brainfuck',
                            query='(binary_expression)',
                        )
                        vex.observe('query_match', on_query_match)

                    def on_query_match(event):
                        pass
                "#},
            )
            .returns_error("unsupported language 'brainfuck'")
    }

    #[test]
    fn malformed_query() {
        VexTest::new("empty")
            .with_scriptlet(
                "vexes/test.star",
                indoc! {r#"
                    def init():
                        vex.add_trigger(
                            language='rust',
                            query='',
                        )
                        vex.observe('query_match', on_query_match)

                    def on_query_match(event):
                        pass
                "#},
            )
            .returns_error(r"query is empty");
        VexTest::new("syntax-error")
            .with_scriptlet(
                "vexes/test.star",
                indoc! {r#"
                    def init():
                        vex.add_trigger(
                            language='rust',
                            query='(binary_expression', # Missing closing bracket
                        )
                        vex.observe('query_match', on_query_match)

                    def on_query_match(event):
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
                let pattern = RawFilePattern::new(path_pattern)
                    .compile(self.root_dir)
                    .unwrap();
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
        let err = RawFilePattern::new(&pattern).compile("").unwrap_err();
        assert_eq!(
            r#"cannot compile "[": invalid range pattern at position 1"#,
            err.to_string()
        );
    }

    #[test]
    fn many_paths() {
        let run_data = VexTest::new("language-with-path")
            .with_scriptlet(
                "vexes/test.star",
                indoc! {r#"
                    def init():
                        vex.add_trigger(
                            language='rust',
                            path=['mod_name_1/', 'mod_name_2/'],
                        )
                        vex.observe('open_file', on_query_match)

                    def on_query_match(event):
                        vex.warn(str(event.path))
                "#},
            )
            .with_source_file(
                "src/main.rs",
                indoc! {r#"
                    mod mod_name_1;
                    mod mod_name_2;
                    mod mod_name_3;

                    fn main() {
                        println!("{}", mod_name_1::func());
                    }
                "#},
            )
            .with_source_file(
                "src/mod_name_1/mod.rs",
                indoc! {r#"
                    fn func() -> &'static str {
                        "hello, world!"
                    }
                "#},
            )
            .with_source_file(
                "src/mod_name_2/mod.rs",
                indoc! {r#"
                    fn func() -> &'static str {
                        "hello, world!"
                    }
                "#},
            )
            .with_source_file(
                "src/mod_name_3/mod.rs",
                indoc! {r#"
                    fn func() -> &'static str {
                        "hello, world!"
                    }
                "#},
            )
            .try_run()
            .unwrap();
        assert_eq!(2, run_data.irritations.len());
    }

    #[test]
    fn path_interactions() {
        let run_data = VexTest::new("language-with-path")
            .with_scriptlet(
                "vexes/test.star",
                indoc! {r#"
                    def init():
                        vex.add_trigger(
                            language='rust',
                            path='mod_name/',
                        )
                        vex.observe('open_file', on_query_match)

                    def on_query_match(event):
                        vex.warn(str(event.path))
                "#},
            )
            .with_source_file(
                "src/main.rs",
                indoc! {r#"
                    mod mod_name;

                    fn main() {
                        println!("{}", mod_name::func());
                    }
                "#},
            )
            .with_source_file(
                "src/mod_name/mod.rs",
                indoc! {r#"
                    fn func() -> &'static str {
                        "hello, world!"
                    }
                "#},
            )
            .try_run()
            .unwrap();
        assert_eq!(1, run_data.irritations.len());

        let run_data = VexTest::new("query-with-path")
            .with_scriptlet(
                "vexes/test.star",
                indoc! {r#"
                    def init():
                        vex.add_trigger(
                            language='rust',
                            query='(binary_expression)',
                            path='mod_name/',
                        )
                        vex.observe('query_match', on_query_match)

                    def on_query_match(event):
                        vex.warn(str(event.path))
                "#},
            )
            .with_source_file(
                "src/main.rs",
                indoc! {r#"
                    mod mod_name;

                    fn main() {
                        let x = mod_name::func() + 3;
                        println!("{x}");
                    }
                "#},
            )
            .with_source_file(
                "src/mod_name/mod.rs",
                indoc! {r#"
                    fn func() -> i32 {
                        1 + 2
                    }
                "#},
            )
            .try_run()
            .unwrap();
        assert_eq!(1, run_data.irritations.len());
    }
}
