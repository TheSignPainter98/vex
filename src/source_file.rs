use std::{fs, ops::Range};

use allocative::Allocative;
use dupe::Dupe;
use log::{info, log_enabled};
use tree_sitter::{Node as TSNode, Parser, QueryCursor, Tree};

use crate::{
    error::{Error, IOAction},
    ignore_markers::{IgnoreMarkers, VexIdFilter},
    result::{RecoverableResult, Result},
    scriptlets::{Location, Node},
    source_path::SourcePath,
    supported_language::SupportedLanguage,
};

#[derive(Debug)]
pub struct SourceFile {
    path: SourcePath,
    language: Option<SupportedLanguage>,
}

impl SourceFile {
    pub fn new(path: SourcePath, language: Option<SupportedLanguage>) -> Self {
        let path = path.dupe();
        Self { path, language }
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
                    language,
                    location: Location::of(&node),
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

    pub fn ignore_markers(&self) -> Result<IgnoreMarkers> {
        let mut builder = IgnoreMarkers::builder();

        let ignore_query = self.language.ignore_query();
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
                    let filter = match VexIdFilter::try_from_iter(raw_parts) {
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
        (&self.path, &self.content, self.language) == (&other.path, &other.content, other.language)
    }
}

impl Eq for ParsedSourceFile {}

#[cfg(test)]
mod test {
    use indoc::indoc;

    use crate::vex::id::VexId;

    use super::*;

    #[test]
    fn general_ignore_markers() {
        let source_file = ParsedSourceFile::new_with_content(
            SourcePath::new_in("src/main.rs".into(), "".into()),
            indoc! {r#"
                fn main() {
                    // vex:ignore *
                    let x = 10;
                }
            "#},
            SupportedLanguage::Rust,
        )
        .unwrap();
        let ignore_markers = source_file.ignore_markers().unwrap();
        let markers: Vec<_> = ignore_markers.markers().collect();
        let [marker] = &markers[..] else {
            panic!("incorrect markers")
        };
        assert!(
            matches!(marker.filter(), VexIdFilter::All),
            "incorrect marker, got {marker:?}"
        )
    }

    #[test]
    fn specific_ignore_markers() {
        let id1 = VexId::try_from("some-lint".to_string()).unwrap();
        let id2 = VexId::try_from("some-other-lint".to_string()).unwrap();
        let id3 = VexId::try_from("some-different-lint".to_string()).unwrap();
        let source_file = ParsedSourceFile::new_with_content(
            SourcePath::new_in("src/main.rs".into(), "".into()),
            indoc! {r#"
                fn main() {
                    // vex:ignore some-lint,some-other-lint, some-different-lint
                    let x = 10;
                }
            "#},
            SupportedLanguage::Rust,
        )
        .unwrap();
        let ignore_markers = source_file.ignore_markers().unwrap();
        let markers: Vec<_> = ignore_markers.markers().collect();
        let [marker] = &markers[..] else {
            panic!("incorrect markers")
        };
        let specific_ids = match marker.filter() {
            VexIdFilter::All => panic!("incorrect marker, got {marker:?}"),
            VexIdFilter::Specific(ids) => ids,
        };
        assert_eq!(&specific_ids[..], [id1, id2, id3]);
    }

    #[test]
    fn invalid_ignore_markers() {
        let source_file = ParsedSourceFile::new_with_content(
            SourcePath::new_in("src/main.rs".into(), "".into()),
            indoc! {r#"
                fn main() {
                    // vex:ignore i/am/invalid
                    let x = 10;
                }
            "#},
            SupportedLanguage::Rust,
        )
        .unwrap();
        let ignore_markers = source_file.ignore_markers().unwrap();
        let markers: Vec<_> = ignore_markers.markers().collect();
        let [marker] = &markers[..] else {
            panic!("incorrect markers")
        };
        let specific_ids = match marker.filter() {
            VexIdFilter::All => panic!("incorrect marker, got {marker:?}"),
            VexIdFilter::Specific(ids) => ids,
        };
        assert!(specific_ids.is_empty());
    }

    #[test]
    fn missing_ignore_markers() {
        let source_file = ParsedSourceFile::new_with_content(
            SourcePath::new_in("src/main.rs".into(), "".into()),
            indoc! {r#"
                fn main() {
                    // vex:ignore
                    let x = 10;
                }
            "#},
            SupportedLanguage::Rust,
        )
        .unwrap();
        let ignore_markers = source_file.ignore_markers().unwrap();
        let markers: Vec<_> = ignore_markers.markers().collect();
        let [marker] = &markers[..] else {
            panic!("incorrect markers");
        };
        match marker.filter() {
            VexIdFilter::Specific(ids) => assert!(ids.is_empty()),
            _ => panic!("unexpected filter in marker: {marker:?}"),
        }
    }
}
