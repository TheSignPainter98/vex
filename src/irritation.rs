use std::{cmp::Ordering, fmt::Display, ops::Range};

use allocative::Allocative;
use annotate_snippets::{Annotation, AnnotationType, Slice, Snippet, SourceAnnotation};
use dupe::Dupe;
use serde::Serialize;

use crate::{logger, scriptlets::Node, source_path::PrettyPath};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Allocative, Serialize)]
#[non_exhaustive]
pub struct Irritation {
    code_source: Option<IrritationSource>,
    vex_path: PrettyPath,
    other_code_sources: Vec<IrritationSource>,
    extra_info_present: bool,
    pub(crate) rendered: String,
}

#[derive(Debug, PartialEq, Eq, Allocative, Serialize)]
pub struct IrritationSource {
    path: PrettyPath,
    #[allocative(skip)]
    byte_range: Range<usize>,
}

impl IrritationSource {
    fn at(node: &Node<'_>) -> Self {
        Self {
            path: node.source_file.path.pretty_path.dupe(),
            byte_range: node.byte_range(),
        }
    }
}

impl Ord for IrritationSource {
    fn cmp(&self, other: &Self) -> Ordering {
        (&self.path, self.byte_range.start, self.byte_range.end).cmp(&(
            &other.path,
            other.byte_range.start,
            other.byte_range.end,
        ))
    }
}

impl PartialOrd for IrritationSource {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Display for Irritation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.rendered.fmt(f)
    }
}

pub struct IrritationRenderer<'v> {
    vex_path: PrettyPath,
    message: &'v str,
    source: Option<(Node<'v>, &'v str)>,
    show_also: Vec<(Node<'v>, &'v str)>,
    extra_info: Option<&'v str>,
}

impl<'v> IrritationRenderer<'v> {
    pub fn new(vex_path: PrettyPath, message: &'v str) -> Self {
        Self {
            vex_path,
            message,
            source: None,
            show_also: Vec::with_capacity(0),
            extra_info: None,
        }
    }

    pub fn set_source(&mut self, at: (Node<'v>, &'v str)) {
        self.source = Some(at);
    }

    pub fn set_show_also(&mut self, show_also: Vec<(Node<'v>, &'v str)>) {
        self.show_also = show_also;
    }

    pub fn set_extra_info(&mut self, extra_info: &'v str) {
        self.extra_info = Some(extra_info);
    }

    pub fn render(self) -> Irritation {
        let Self {
            vex_path,
            source,
            message,
            show_also,
            extra_info,
        } = self;

        // TODO(kcza): allow source and show_alsos to be in separate files.

        let file_name = source
            .as_ref()
            .map(|source| source.0.source_file.path.pretty_path.as_str());
        let snippet = Snippet {
            title: Some(Annotation {
                id: Some(vex_path.file_stem().expect("vex has no file stem")),
                label: Some(message),
                annotation_type: AnnotationType::Warning,
            }),
            slices: source
                .iter()
                .map(|(node, label)| {
                    let range = Self::relevant_range(node);
                    Slice {
                        source: &node.source_file.content[range.start..range.end],
                        // overhead!
                        line_start: 1 + node.start_position().row,
                        origin: Some(file_name.as_ref().unwrap()),
                        annotations: [SourceAnnotation {
                            range: (
                                node.start_byte() - range.start,
                                node.end_byte() - range.start,
                            ),
                            label,
                            annotation_type: AnnotationType::Warning,
                        }]
                        .into_iter()
                        .chain(show_also.iter().map(|(node, label)| SourceAnnotation {
                            range: (
                                node.start_byte() - range.start,
                                node.end_byte() - range.start,
                            ),
                            label,
                            annotation_type: AnnotationType::Info,
                        }))
                        .collect(),
                        fold: true,
                    }
                })
                .collect(),
            footer: extra_info
                .into_iter()
                .map(|extra_info| Annotation {
                    id: None,
                    label: Some(extra_info),
                    annotation_type: AnnotationType::Info,
                })
                .collect(),
        };

        let code_source = source.map(|(node, _)| IrritationSource::at(&node));
        let other_code_sources = show_also
            .iter()
            .map(|(node, _)| IrritationSource::at(node))
            .collect();
        let extra_info_present = extra_info.is_some();
        let rendered = logger::render_snippet(snippet);
        Irritation {
            vex_path,
            code_source,
            other_code_sources,
            extra_info_present,
            rendered,
        }
    }

    fn relevant_range(node: &Node<'v>) -> Range<usize> {
        let Range { start, end } = node.byte_range();
        let content = &node.source_file.content;
        let start = content[..start]
            .rfind(['\n', '\r'])
            .map(|i| i + 1)
            .unwrap_or_default();
        let end = content[end..]
            .find(['\n', '\r'])
            .map(|i| i + end)
            .unwrap_or(content.len());
        start..end
    }
}
