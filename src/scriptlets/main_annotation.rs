use std::fmt::Display;

use allocative::Allocative;
use dupe::Dupe;
use starlark::values::{StarlarkValue, UnpackValue, Value};
use starlark_derive::{starlark_value, NoSerialize, ProvidesStaticType};

use crate::{scriptlets::Node, source_path::PrettyPath};

#[derive(Debug, PartialEq, Eq, ProvidesStaticType, NoSerialize, Allocative)]
pub enum MainAnnotation<'v> {
    Path(PrettyPath),
    Node {
        node: Node<'v>,
        #[allocative(skip)]
        label: Option<&'v str>,
    },
}

impl<'v> MainAnnotation<'v> {
    pub fn node(&self) -> Option<&Node<'v>> {
        match self {
            Self::Path(_) => None,
            Self::Node { node, .. } => Some(node),
        }
    }

    pub fn pretty_path(&self) -> &PrettyPath {
        match self {
            Self::Path(p) => p,
            Self::Node { node, .. } => &node.source_file.path.pretty_path,
        }
    }
}

impl<'v> Display for MainAnnotation<'v> {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        unimplemented!("unused, required to satisfy trait bound")
    }
}

#[starlark_value(type = "Path|(Node, str)")]
impl<'v> StarlarkValue<'v> for MainAnnotation<'v> {}

impl<'v> UnpackValue<'v> for MainAnnotation<'v> {
    fn unpack_value(value: Value<'v>) -> Option<Self> {
        if let Some(annot) = value.request_value::<&PrettyPath>() {
            Some(Self::Path(annot.dupe()))
        } else if let Some((node, label)) = <(Node<'_>, &str)>::unpack_value(value) {
            let label = Some(label);
            Some(Self::Node { node, label })
        } else if let Some(node) = Node::unpack_value(value) {
            let label = None;
            Some(Self::Node { node, label })
        } else {
            None
        }
    }
}
