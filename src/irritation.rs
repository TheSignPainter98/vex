use std::{cmp::Ordering, fmt::Display, iter, ops::Range};

use allocative::Allocative;
use annotate_snippets::{Annotation, AnnotationType, Slice, Snippet, SourceAnnotation};
use dupe::Dupe;
use serde::Serialize;

use crate::{
    logger,
    scriptlets::{main_annotation::MainAnnotation, Node},
    source_path::PrettyPath,
    vex::id::PrettyVexId,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Allocative, Serialize)]
#[non_exhaustive]
pub struct Irritation {
    code_source: Option<IrritationSource>,
    pretty_vex_id: PrettyVexId,
    other_code_sources: Vec<IrritationSource>,
    info_present: bool,
    pub(crate) rendered: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Allocative, Serialize)]
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

    fn whole_file(path: PrettyPath) -> Self {
        Self {
            path,
            byte_range: 0..0,
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
    pretty_vex_id: PrettyVexId,
    message: &'v str,
    source: Option<MainAnnotation<'v>>,
    show_also: Vec<(Node<'v>, &'v str)>,
    info: Option<&'v str>,
}

impl<'v> IrritationRenderer<'v> {
    pub fn new(pretty_vex_id: PrettyVexId, message: &'v str) -> Self {
        Self {
            pretty_vex_id,
            message,
            source: None,
            show_also: Vec::with_capacity(0),
            info: None,
        }
    }

    pub fn set_source(&mut self, source: MainAnnotation<'v>) {
        self.source = Some(source);
    }

    pub fn set_show_also(&mut self, show_also: Vec<(Node<'v>, &'v str)>) {
        self.show_also = show_also;
    }

    pub fn set_info(&mut self, info: &'v str) {
        self.info = Some(info);
    }

    pub fn render(self) -> Irritation {
        let Self {
            pretty_vex_id,
            source,
            message,
            show_also,
            info,
        } = self;

        let file_name = source.as_ref().map(|source| source.pretty_path().as_str());
        let snippet = Snippet {
            title: Some(Annotation {
                id: Some(pretty_vex_id.as_str()),
                label: Some(message),
                annotation_type: AnnotationType::Warning,
            }),
            slices: source
                .iter()
                .map(|annot| match annot {
                    MainAnnotation::Path { path, label } => Slice {
                        source: "...",
                        line_start: 1,
                        origin: Some(path.as_str()),
                        annotations: vec![SourceAnnotation {
                            range: (0, 1),
                            label: label.unwrap_or_default(),
                            annotation_type: AnnotationType::Warning,
                        }],
                        fold: false,
                    },
                    MainAnnotation::Node { node, label } => {
                        let range = {
                            let start = iter::once(node)
                                .chain(show_also.iter().map(|(node, _)| node))
                                .map(|node| node.byte_range().start)
                                .min()
                                .unwrap();
                            let end = iter::once(node)
                                .chain(show_also.iter().map(|(node, _)| node))
                                .map(|node| node.byte_range().end)
                                .max()
                                .unwrap();
                            node.source_file.full_lines_range(start..end)
                        };
                        Slice {
                            source: &node.source_file.content[range.start..range.end],
                            line_start: 1 + node.start_position().row,
                            origin: file_name,
                            annotations: [SourceAnnotation {
                                range: (
                                    node.start_byte() - range.start,
                                    node.end_byte() - range.start,
                                ),
                                label: label.unwrap_or_default(),
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
                    }
                })
                .collect(),
            footer: info
                .into_iter()
                .map(|info| Annotation {
                    id: None,
                    label: Some(info),
                    annotation_type: AnnotationType::Info,
                })
                .collect(),
        };

        let code_source = source.as_ref().map(|source| match source {
            MainAnnotation::Path { path, .. } => IrritationSource::whole_file(path.dupe()),
            MainAnnotation::Node { node, .. } => IrritationSource::at(node),
        });
        let other_code_sources = show_also
            .iter()
            .map(|(node, _)| IrritationSource::at(node))
            .collect();
        let info_present = info.is_some();
        let rendered = logger::render_snippet(snippet);
        Irritation {
            pretty_vex_id,
            code_source,
            other_code_sources,
            info_present,
            rendered,
        }
    }
}
