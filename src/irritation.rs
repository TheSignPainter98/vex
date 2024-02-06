use std::fmt::Display;

use annotate_snippets::{Annotation, AnnotationType, Slice, Snippet};
use dupe::Dupe;
use tree_sitter::QueryMatch;

use crate::{logger, source_file::SourceFile, source_path::PrettyPath};

#[derive(Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Irritation {
    pub vex_path: PrettyPath,
    pub message: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub path: PrettyPath,
}

impl Irritation {
    #[allow(unused)]
    fn new(path: PrettyPath, src_file: &SourceFile, nit: QueryMatch<'_, '_>) -> Self {
        // TODO(kcza): refactor to better suit new vex.warn
        let snippet = Snippet {
            title: Some(Annotation {
                id: Some(path.as_str()),
                label: Some(path.as_str()),
                annotation_type: AnnotationType::Warning,
            }),
            footer: Vec::with_capacity(0), // TODO(kcza): is vec![] a good
            slices: nit
                .captures
                .iter()
                .map(|capture| {
                    let node = capture.node;
                    let range = node.range();
                    Slice {
                        source: &src_file.content[range.start_byte..range.end_byte],
                        line_start: range.start_point.row,
                        origin: Some(src_file.path.as_str()),
                        annotations: vec![], // TODO(kcza): figure out how to
                        fold: true,
                    }
                })
                .collect(),
        };
        Self {
            vex_path: path.dupe(),
            message: logger::render_snippet(snippet),
            start_byte: nit
                .captures
                .iter()
                .map(|cap| cap.node.start_byte())
                .min()
                .unwrap_or(0),
            end_byte: nit
                .captures
                .iter()
                .map(|cap| cap.node.end_byte())
                .max()
                .unwrap_or(usize::MAX),
            path: src_file.path.pretty_path.dupe(),
        }
    }
}

impl Ord for Irritation {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (&self.path, self.start_byte, self.end_byte).cmp(&(
            &other.path,
            other.start_byte,
            other.end_byte,
        ))
    }
}

impl PartialOrd for Irritation {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Display for Irritation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.message.fmt(f)
    }
}
