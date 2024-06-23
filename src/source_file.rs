use std::fs;

use allocative::Allocative;
use dupe::Dupe;
use log::{info, log_enabled, warn};
use tree_sitter::{Parser, QueryCursor, Tree};

use crate::{
    error::{Error, IOAction},
    ignore_markers::{IgnoreMarkers, NewVexIdFilterOpts, VexIdFilter},
    result::Result,
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
            if tree.root_node().has_error() {
                return Err(Error::UnparseableAsLanguage {
                    path: path.pretty_path.dupe(),
                    language,
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
                if qcaps.len() == 1 && log_enabled!(log::Level::Warn) {
                    let marker_node = qcaps[0].node;
                    warn!(
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
                    let node = qcaps[marker_index].node;
                    let mut raw_parts = node
                        .utf8_text(self.content.as_bytes())
                        .unwrap()
                        .split_whitespace()
                        .skip(2);
                    let Some(raw) = raw_parts.next() else {
                        return Err(Error::NoVexIds {
                            file: self.path.pretty_path.dupe(),
                            location: Location::of(&Node::new(node, self)),
                        });
                    };
                    VexIdFilter::new(
                        raw,
                        NewVexIdFilterOpts {
                            path: &self.path.pretty_path,
                            location: Location::of(&Node::new(node, self)),
                        },
                    )
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

    use crate::{source_path::PrettyPath, vex::id::VexId};

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
        let vex1 = VexId::new(PrettyPath::new("some/specific/lint.star".into()));
        let vex2 = VexId::new(PrettyPath::new("some/other/lint.star".into()));
        let source_file = ParsedSourceFile::new_with_content(
            SourcePath::new_in("src/main.rs".into(), "".into()),
            indoc! {r#"
                fn main() {
                    // vex:ignore some/specific/lint,some/other/lint
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
        assert_eq!(&specific_ids[..], [vex1, vex2]);
    }

    #[test]
    fn invalid_ignore_markers() {
        let source_file = ParsedSourceFile::new_with_content(
            SourcePath::new_in("src/main.rs".into(), "".into()),
            indoc! {r#"
                fn main() {
                    // vex:ignore i/do/not/exist
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
        let err = source_file.ignore_markers().unwrap_err();
        assert!(
            err.to_string().contains("no vex ids specified"),
            "incorrect error, got {err}"
        );
    }
}
