use std::fmt::Display;

use allocative::Allocative;
use starlark::values::{StarlarkValue, UnpackValue, Value};
use starlark_derive::{starlark_value, NoSerialize, ProvidesStaticType};

use crate::{scriptlets::Node, source_path::PrettyPath};

#[derive(Debug, PartialEq, Eq, ProvidesStaticType, NoSerialize, Allocative)]
pub enum MainAnnotation<'v> {
    Path {
        path: PrettyPath,

        #[allocative(skip)]
        label: Option<&'v str>,
    },
    Node {
        node: Node<'v>,

        #[allocative(skip)]
        label: Option<&'v str>,
    },
}

impl<'v> MainAnnotation<'v> {
    pub fn node(&self) -> Option<&Node<'v>> {
        match self {
            Self::Path { .. } => None,
            Self::Node { node, .. } => Some(node),
        }
    }

    pub fn pretty_path(&self) -> &PrettyPath {
        match self {
            Self::Path { path, .. } => path,
            Self::Node { node, .. } => &node.source_file.path.pretty_path,
        }
    }
}

impl<'v> Display for MainAnnotation<'v> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

#[starlark_value(type = "Path|(Node, str)")]
impl<'v> StarlarkValue<'v> for MainAnnotation<'v> {}

impl<'v> UnpackValue<'v> for MainAnnotation<'v> {
    fn unpack_value(value: Value<'v>) -> Option<Self> {
        #[allow(clippy::manual_map)]
        if let Some((path, label)) = <(PrettyPath, &str)>::unpack_value(value) {
            Some(Self::Path {
                path,
                label: Some(label),
            })
        } else if let Some(path) = PrettyPath::unpack_value(value) {
            Some(Self::Path { path, label: None })
        } else if let Some((node, label)) = <(Node<'_>, &str)>::unpack_value(value) {
            Some(Self::Node {
                node,
                label: Some(label),
            })
        } else if let Some(node) = Node::unpack_value(value) {
            Some(Self::Node { node, label: None })
        } else {
            None
        }
    }
}
