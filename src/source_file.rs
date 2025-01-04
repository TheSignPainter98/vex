use std::{fs, ops::Range};

use allocative::Allocative;
use camino::{Utf8Path, Utf8PathBuf};
use dupe::{Dupe, OptionDupedExt};
use log::{info, log_enabled};
use tree_sitter::{Node as TSNode, Parser, QueryCursor, Tree};
use walkdir::WalkDir;

use crate::{
    cli::MaxConcurrentFileLimit,
    context::{Context, LanguageData, Manifest},
    error::{Error, IOAction},
    ignore_markers::{IgnoreMarkers, LintIdFilter},
    language::Language,
    result::{RecoverableResult, Result},
    scriptlets::{Location, Node},
    source_path::SourcePath,
    trigger::FilePattern,
};

pub fn sources_in_dir(
    ctx: &Context,
    max_concurrent_files: MaxConcurrentFileLimit,
) -> Result<Vec<SourceFile>> {
    let ignores: Vec<_> = ctx
        .files
        .ignores
        .clone()
        .into_inner()
        .into_iter()
        .map(|ignore| ignore.compile())
        .collect::<Result<_>>()?;
    let allows: Vec<_> = ctx
        .files
        .allows
        .clone()
        .into_iter()
        .map(|allow| allow.compile())
        .collect::<Result<_>>()?;
    let associations = ctx.associations()?;

    let root = ctx.project_root.as_str();

    WalkDir::new(root)
        .follow_links(false)
        .follow_root_links(false)
        .max_open(max_concurrent_files.into())
        .into_iter()
        .filter_entry(|entry| {
            let entry_path = match Utf8Path::from_path(entry.path()) {
                Some(p) => p,
                _ => return false,
            };

            let is_root = entry_path == root;

            let is_hidden = entry_path
                .file_name()
                .is_some_and(|file_name| file_name.starts_with('.'));
            if is_hidden && !is_root {
                if log_enabled!(log::Level::Info) {
                    let dir_marker = if entry.file_type().is_dir() { "/" } else { "" };
                    info!("ignoring {entry_path}{dir_marker}: hidden");
                }
                return false;
            }

            let matches_any = |path, patterns: &[FilePattern]| {
                patterns.iter().any(|pattern| pattern.matches(path))
            };
            if matches_any(entry_path, &ignores) && !matches_any(entry_path, &allows) {
                if log_enabled!(log::Level::Info) {
                    let dir_marker = if entry.file_type().is_dir() { "/" } else { "" };
                    info!(
                        "ignoring {}{dir_marker}: matches ignore pattern",
                        entry_path.strip_prefix(root).unwrap_or(entry_path),
                    );
                }
                return false;
            }

            if !is_root
                && entry.file_type().is_dir()
                && entry_path.join(Manifest::FILE_NAME).exists()
            {
                if log_enabled!(log::Level::Info) {
                    let dir_marker = if entry.file_type().is_dir() { "/" } else { "" };
                    info!(
                        "ignoring {}{dir_marker}: contains vex project",
                        entry_path.strip_prefix(root).unwrap_or(entry_path),
                    );
                }
                return false;
            }
            true
        })
        .flatten()
        .filter(|entry| entry.file_type().is_file())
        .flat_map(|entry| Utf8PathBuf::from_path_buf(entry.path().to_owned()))
        .map(|entry_path| SourcePath::new(&entry_path, &ctx.project_root))
        .map(|source_path| {
            let language = associations.get_language(&source_path)?;
            Ok(SourceFile::new(source_path, language.duped()))
        })
        .collect()
}

#[derive(Debug)]
pub struct SourceFile {
    path: SourcePath,
    language: Option<Language>,
}

impl SourceFile {
    pub fn new(path: SourcePath, language: Option<Language>) -> Self {
        let path = path.dupe();
        Self { path, language }
    }

    pub fn path(&self) -> &SourcePath {
        &self.path
    }

    pub fn language(&self) -> Option<&Language> {
        self.language.as_ref()
    }

    pub fn parse(&self, ctx: &Context) -> Result<ParsedSourceFile> {
        if log_enabled!(log::Level::Info) {
            info!("parsing {}", self.path);
        }
        let language = self
            .language
            .as_ref()
            .ok_or_else(|| Error::NoParserForFile(self.path.pretty_path.dupe()))?;
        let content =
            fs::read_to_string(self.path.abs_path.as_str()).map_err(|cause| Error::IO {
                path: self.path.pretty_path.dupe(),
                action: IOAction::Read,
                cause,
            })?;

        let language_data = match ctx.language_data(language)? {
            Some(language_data) => language_data,
            None => return Err(Error::NoParserForLanguage(language.dupe())),
        };
        ParsedSourceFile::new_with_content(self.path.dupe(), content, language_data.dupe())
    }
}

#[derive(Clone, Debug, Allocative)]
pub struct ParsedSourceFile {
    pub path: SourcePath,
    pub content: String,
    #[allocative(skip)]
    pub language_data: LanguageData,
    #[allocative(skip)]
    pub tree: Tree,
}

impl ParsedSourceFile {
    pub fn new_with_content(
        path: SourcePath,
        content: impl Into<String>,
        language_data: LanguageData,
    ) -> Result<Self> {
        let content = content.into();

        let tree = {
            let mut parser = Parser::new();
            parser.set_language(language_data.ts_language())?;
            let tree = parser
                .parse(&content, None)
                .expect("unexpected parser failure");

            fn find_error_node(root: TSNode<'_>) -> Option<TSNode<'_>> {
                if !root.has_error() {
                    return None;
                }

                let mut cursor = root.walk();
                loop {
                    let curr_node = cursor.node();
                    if curr_node.is_error() || curr_node.is_missing() {
                        break Some(curr_node);
                    }

                    if curr_node.has_error() {
                        assert!(cursor.goto_first_child());
                    } else {
                        assert!(cursor.goto_next_sibling());
                    }
                }
            }
            if let Some(node) = find_error_node(tree.root_node()) {
                return Err(Error::UnparseableAsLanguage {
                    path: path.pretty_path.dupe(),
                    language: language_data.language().dupe(),
                    location: Location::of(&node),
                });
            }
            tree
        };
        Ok(Self {
            path,
            content,
            tree,
            language_data,
        })
    }

    pub fn ignore_markers(&self) -> Result<IgnoreMarkers> {
        let mut builder = IgnoreMarkers::builder();

        let ignore_query = self.language_data.ignore_query();
        let marker_index = ignore_query
            .capture_index_for_name("marker")
            .expect("internal error: ignore query contains no 'marker' capture")
            as usize;
        QueryCursor::new()
            .matches(ignore_query, self.tree.root_node(), self.content.as_bytes())
            .map(|qmatch| qmatch.captures)
            .inspect(|qcaps| {
                debug_assert!(!qcaps.is_empty());
                if qcaps.len() == 1 {
                    let marker_node = qcaps[0].node;
                    crate::warn!(
                        "{}:{} ignore marker not associated with any block",
                        self.path.pretty_path,
                        Location::of(&Node::new(marker_node, self)),
                    )
                }
            })
            .map(|qcaps| {
                let byte_range = {
                    let start = qcaps
                        .iter()
                        .map(|qcap| qcap.node.byte_range().start)
                        .min()
                        .expect("internal error: ignore query captured nothing");
                    let end = qcaps
                        .iter()
                        .map(|qcap| qcap.node.byte_range().end)
                        .max()
                        .expect("internal error: ignore query captured nothing");
                    start..end
                };
                let filter = {
                    const IGNORE_MARKER: &str = "vex:ignore";

                    let node = qcaps[marker_index].node;
                    let raw_text = node.utf8_text(self.content.as_bytes()).unwrap();
                    let ids_start_index = raw_text
                        .find(IGNORE_MARKER)
                        .expect("vex:ignore not present in ignore marker")
                        + IGNORE_MARKER.len();
                    let raw_parts = raw_text[ids_start_index..]
                        .split(',')
                        .map(|raw_part| raw_part.trim());
                    let filter = match LintIdFilter::try_from_iter(raw_parts) {
                        RecoverableResult::Ok(filter) => filter,
                        RecoverableResult::Recovered(filter, errs) => {
                            for err in errs {
                                crate::warn!(
                                    "{}:{}: {}",
                                    self.path,
                                    Location::of(&Node::new(node, self)),
                                    err
                                );
                            }
                            filter
                        }
                        RecoverableResult::Err(err) => return Err(err),
                    };
                    if filter.is_empty() {
                        crate::warn!(
                            "{}:{}: no vex ids specified",
                            self.path,
                            Location::of(&Node::new(node, self)),
                        )
                    }
                    filter
                };
                Ok((byte_range, filter))
            })
            .try_for_each(|ignore_spec| {
                let (byte_range, filter) = ignore_spec?;
                builder.add(byte_range, filter);
                Ok::<_, Error>(())
            })?;

        Ok(builder.build())
    }

    pub fn full_lines_range(&self, range: Range<usize>) -> Range<usize> {
        let (start, end) = (range.start, range.end);

        let start = self.content[..start]
            .rfind(['\n', '\r'])
            .map(|i| i + 1)
            .unwrap_or_default();
        let end = self.content[end..]
            .find(['\n', '\r'])
            .map(|i| i + end)
            .unwrap_or(self.content.len());
        start..end
    }
}

impl PartialEq for ParsedSourceFile {
    fn eq(&self, other: &Self) -> bool {
        (&self.path, &self.content, &self.language_data.language())
            == (&other.path, &other.content, &other.language_data.language())
    }
}

impl Eq for ParsedSourceFile {}

#[cfg(test)]
mod tests {
    use std::{fs::File, io::Write};

    use indoc::indoc;

    use crate::id::LintId;

    use super::*;

    #[test]
    fn directory_walking() {
        let tempdir = tempfile::tempdir().unwrap();
        let tempdir_path = Utf8PathBuf::try_from(tempdir.path().to_owned()).unwrap();

        let manifest_content: &str = indoc! {r#"
            [vex]
            version = "1"

            [files]
            ignore = [ "to-ignore", "to-allow" ]
            allow = [ "to-allow" ]
        "#};
        let files = [
            ("vex.toml", manifest_content),
            ("to-ignore", "ignored content"),
            ("to-allow", "allowed content"),
            ("sub-project/vex.toml", manifest_content),
            ("sub-project/sub-project-file", "sub-project-content"),
        ];
        for (path, content) in files {
            let abs_path = tempdir_path.join(path);
            fs::create_dir_all(abs_path.parent().unwrap()).unwrap();
            File::create(abs_path)
                .unwrap()
                .write_all(content.as_bytes())
                .unwrap();
        }

        let ctx = Context::acquire_in(&tempdir_path).unwrap();
        let sources = sources_in_dir(&ctx, MaxConcurrentFileLimit::new(1)).unwrap();
        let returned_paths = {
            let mut returned_paths: Vec<_> = sources
                .iter()
                .map(|source_file| source_file.path().pretty_path.as_str())
                .collect();
            returned_paths.sort();
            returned_paths
        };

        let expected_paths = ["to-allow", "vex.toml"];
        assert_eq!(returned_paths, expected_paths);
    }

    #[test]
    fn general_ignore_markers() {
        let ctx = Context::new_with_manifest("test-path".into(), Manifest::default());
        let source_file = ParsedSourceFile::new_with_content(
            SourcePath::new_in("src/main.rs".into(), "".into()),
            indoc! {r#"
                fn main() {
                    // vex:ignore *
                    let x = 10;
                }
            "#},
            ctx.language_data(&Language::Rust).unwrap().unwrap().dupe(),
        )
        .unwrap();
        let ignore_markers = source_file.ignore_markers().unwrap();
        let markers: Vec<_> = ignore_markers.markers().collect();
        let [marker] = &markers[..] else {
            panic!("incorrect markers")
        };
        assert!(
            matches!(marker.filter(), LintIdFilter::All),
            "incorrect marker, got {marker:?}"
        )
    }

    #[test]
    fn specific_ignore_markers() {
        let ctx = Context::new_with_manifest("test-path".into(), Manifest::default());

        let id1 = LintId::try_from("some-lint".to_string()).unwrap();
        let id2 = LintId::try_from("some-other-lint".to_string()).unwrap();
        let id3 = LintId::try_from("some-different-lint".to_string()).unwrap();
        let source_file = ParsedSourceFile::new_with_content(
            SourcePath::new_in("src/main.rs".into(), "".into()),
            indoc! {r#"
                fn main() {
                    // vex:ignore some-lint,some-other-lint, some-different-lint
                    let x = 10;
                }
            "#},
            ctx.language_data(&Language::Rust).unwrap().unwrap().dupe(),
        )
        .unwrap();
        let ignore_markers = source_file.ignore_markers().unwrap();
        let markers: Vec<_> = ignore_markers.markers().collect();
        let [marker] = &markers[..] else {
            panic!("incorrect markers")
        };
        let specific_ids = match marker.filter() {
            LintIdFilter::All => panic!("incorrect marker, got {marker:?}"),
            LintIdFilter::Specific(ids) => ids,
        };
        assert_eq!(&specific_ids[..], [id1, id2, id3]);
    }

    #[test]
    fn invalid_ignore_markers() {
        let ctx = Context::new_with_manifest("test-path".into(), Manifest::default());

        let source_file = ParsedSourceFile::new_with_content(
            SourcePath::new_in("src/main.rs".into(), "".into()),
            indoc! {r#"
                fn main() {
                    // vex:ignore i/am/invalid
                    let x = 10;
                }
            "#},
            ctx.language_data(&Language::Rust).unwrap().unwrap().dupe(),
        )
        .unwrap();
        let ignore_markers = source_file.ignore_markers().unwrap();
        let markers: Vec<_> = ignore_markers.markers().collect();
        let [marker] = &markers[..] else {
            panic!("incorrect markers")
        };
        let specific_ids = match marker.filter() {
            LintIdFilter::All => panic!("incorrect marker, got {marker:?}"),
            LintIdFilter::Specific(ids) => ids,
        };
        assert!(specific_ids.is_empty());
    }

    #[test]
    fn missing_ignore_markers() {
        let ctx = Context::new_with_manifest("test-path".into(), Manifest::default());

        let source_file = ParsedSourceFile::new_with_content(
            SourcePath::new_in("src/main.rs".into(), "".into()),
            indoc! {r#"
                fn main() {
                    // vex:ignore
                    let x = 10;
                }
            "#},
            ctx.language_data(&Language::Rust).unwrap().unwrap().dupe(),
        )
        .unwrap();
        let ignore_markers = source_file.ignore_markers().unwrap();
        let markers: Vec<_> = ignore_markers.markers().collect();
        let [marker] = &markers[..] else {
            panic!("incorrect markers");
        };
        match marker.filter() {
            LintIdFilter::Specific(ids) => assert!(ids.is_empty()),
            _ => panic!("unexpected filter in marker: {marker:?}"),
        }
    }
}
