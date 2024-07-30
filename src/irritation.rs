use std::{cmp::Ordering, fmt::Display, iter, ops::Range};

use allocative::Allocative;
use annotate_snippets::{Annotation, AnnotationType, Slice, Snippet, SourceAnnotation};
use dupe::Dupe;
use serde::Serialize;
use starlark::values::{list::AllocList, AllocValue, Heap, StarlarkValue, Value};
use starlark_derive::{starlark_attrs, starlark_value, ProvidesStaticType, StarlarkAttrs};

use crate::{
    logger,
    scriptlets::{main_annotation::MainAnnotation, Location, Node},
    source_path::PrettyPath,
    vex::id::PrettyVexId,
};

#[derive(Debug, Clone, PartialEq, Eq, Allocative, Serialize, ProvidesStaticType)]
#[non_exhaustive]
pub struct Irritation {
    pub(crate) rendered: String,
    pretty_vex_id: PrettyVexId,
    message: String,
    source: Option<(IrritationSource, Option<String>)>,
    show_also: Vec<(IrritationSource, String)>,
    info: Option<String>,
}

impl Irritation {
    const PRETTY_VEX_ID_ATTR_NAME: &'static str = "vex_id";
    const MESSAGE_ATTR_NAME: &'static str = "message";
    const SOURCE_ATTR_NAME: &'static str = "at";
    const SHOW_ALSO_ATTR_NAME: &'static str = "show_also";
    const INFO_ATTR_NAME: &'static str = "info";
}

impl Ord for Irritation {
    fn cmp(&self, other: &Self) -> Ordering {
        let Self {
            source,
            pretty_vex_id,
            show_also,
            info,
            message,
            rendered: _,
        } = self;

        fn loc<S, T>(annot: &(S, T)) -> &S {
            let (loc, _) = annot;
            loc
        }
        fn label<S, T>(annot: &(S, T)) -> &T {
            let (_, label) = annot;
            label
        }
        return (
            source.as_ref().map(loc),
            pretty_vex_id,
            ComparableIterator(show_also.iter().map(loc)),
            info,
            source.as_ref().map(label),
            ComparableIterator(show_also.iter().map(label)),
            message,
        )
            .cmp(&(
                other.source.as_ref().map(loc),
                &other.pretty_vex_id,
                ComparableIterator(other.show_also.iter().map(loc)),
                &other.info,
                other.source.as_ref().map(label),
                ComparableIterator(other.show_also.iter().map(label)),
                &other.message,
            ));

        // ComparableIterator implements Ord on the lexicographic order of its contents.
        #[derive(Clone)]
        struct ComparableIterator<I>(I);

        impl<I, T> Ord for ComparableIterator<I>
        where
            I: Iterator<Item = T> + Clone,
            T: Ord,
        {
            fn cmp(&self, other: &Self) -> Ordering {
                self.0.clone().cmp(other.0.clone())
            }
        }

        impl<I, T> PartialOrd for ComparableIterator<I>
        where
            I: Iterator<Item = T> + Clone,
            T: Ord,
        {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }

        impl<I, T> Eq for ComparableIterator<I>
        where
            I: Iterator<Item = T> + Clone,
            T: Eq,
        {
        }

        impl<I, T> PartialEq for ComparableIterator<I>
        where
            I: Iterator<Item = T> + Clone,
            T: Eq,
        {
            fn eq(&self, other: &Self) -> bool {
                self.0.to_owned().eq(other.0.clone())
            }
        }
    }
}

impl PartialOrd<Self> for Irritation {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[starlark_value(type = "Irritation")]
impl<'v> StarlarkValue<'v> for Irritation {
    fn dir_attr(&self) -> Vec<String> {
        [
            Self::PRETTY_VEX_ID_ATTR_NAME,
            Self::MESSAGE_ATTR_NAME,
            Self::SOURCE_ATTR_NAME,
            Self::SHOW_ALSO_ATTR_NAME,
            Self::INFO_ATTR_NAME,
        ]
        .into_iter()
        .map(Into::into)
        .collect()
    }

    fn get_attr(&self, attr: &str, heap: &'v Heap) -> Option<Value<'v>> {
        match attr {
            Self::PRETTY_VEX_ID_ATTR_NAME => Some(heap.alloc(self.pretty_vex_id.to_string())),
            Self::MESSAGE_ATTR_NAME => Some(heap.alloc(&self.message)),
            Self::SOURCE_ATTR_NAME => Some(
                self.source
                    .clone()
                    .map(|(src, label)| {
                        let label_value =
                            label.map(|l| heap.alloc(l)).unwrap_or_else(Value::new_none);
                        heap.alloc((src, label_value))
                    })
                    .unwrap_or_else(Value::new_none),
            ),
            Self::SHOW_ALSO_ATTR_NAME => {
                Some(heap.alloc(AllocList(self.show_also.iter().cloned())))
            }
            Self::INFO_ATTR_NAME => Some(
                self.info
                    .clone()
                    .map(|info| heap.alloc(info))
                    .unwrap_or_else(Value::new_none),
            ),
            _ => None,
        }
    }

    fn has_attr(&self, attr: &str, _heap: &'v Heap) -> bool {
        [
            Self::PRETTY_VEX_ID_ATTR_NAME,
            Self::MESSAGE_ATTR_NAME,
            Self::SOURCE_ATTR_NAME,
            Self::SHOW_ALSO_ATTR_NAME,
            Self::INFO_ATTR_NAME,
        ]
        .contains(&attr)
    }
}

impl<'v> AllocValue<'v> for Irritation {
    fn alloc_value(self, heap: &'v Heap) -> Value<'v> {
        heap.alloc_simple(self)
    }
}

impl Display for Irritation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.rendered.fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Allocative, Serialize, StarlarkAttrs, ProvidesStaticType)]
pub struct IrritationSource {
    path: PrettyPath,
    #[starlark(skip)]
    #[allocative(skip)]
    byte_range: Range<usize>,
    location: Location,
}

impl IrritationSource {
    fn at(node: &Node<'_>) -> Self {
        Self {
            path: node.source_file.path.pretty_path.dupe(),
            byte_range: node.byte_range(),
            location: Location::of(node),
        }
    }

    fn whole_file(path: PrettyPath) -> Self {
        Self {
            path,
            byte_range: 0..0,
            location: Location::start_of_file(),
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

impl Dupe for IrritationSource {
    // Fields:
    // .path: Dupe
    // .byte_range: !Dupe but cheap
    // .location: Dupe
}

#[starlark_value(type = "IrritationSource")]
impl<'v> StarlarkValue<'v> for IrritationSource {
    starlark_attrs!();
}

impl<'v> AllocValue<'v> for IrritationSource {
    fn alloc_value(self, heap: &'v Heap) -> Value<'v> {
        heap.alloc_simple(self)
    }
}

impl Display for IrritationSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self { path, location, .. } = self;
        write!(f, "{path}:{location}")
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

        let rendered = logger::render_snippet(snippet);
        let message = message.to_string();
        let source = source.map(|source| match source {
            MainAnnotation::Path { path, label } => (
                IrritationSource::whole_file(path.dupe()),
                label.map(|l| l.to_string()),
            ),
            MainAnnotation::Node { node, label } => {
                (IrritationSource::at(&node), label.map(|l| l.to_string()))
            }
        });
        let show_also = show_also
            .into_iter()
            .map(|(node, label)| (IrritationSource::at(&node), label.to_string()))
            .collect();
        let info = info.map(|e| e.to_string());
        Irritation {
            rendered,
            pretty_vex_id,
            message,
            source,
            show_also,
            info,
        }
    }
}
