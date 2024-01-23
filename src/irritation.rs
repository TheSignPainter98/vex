use std::{fmt::Display, sync::Arc};

use annotate_snippets::{Annotation, AnnotationType, Slice, Snippet};
use camino::Utf8Path;
use dupe::Dupe;
use tree_sitter::QueryMatch;

use crate::{logger, source_file::SourceFile, vex::Id};

#[derive(Debug, PartialEq, Eq, PartialOrd)]
pub struct Irritation {
    message: String,
    start_byte: usize,
    end_byte: usize,
    path: Arc<Utf8Path>,
}

impl Irritation {
    #[allow(unused)]
    fn new(vex_id: &Id, src_file: &SourceFile, nit: QueryMatch<'_, '_>) -> Self {
        // TODO(kcza): refactor to better suit new vex.warn
        let snippet = Snippet {
            title: Some(Annotation {
                id: Some(vex_id.as_str()),
                label: Some(vex_id.as_str()),
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
            path: src_file.path.dupe(),
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

impl Display for Irritation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.message.fmt(f)
    }
}
