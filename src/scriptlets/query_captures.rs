use std::{fmt::Display, ops::Deref};

use allocative::Allocative;
use derive_new::new;
use dupe::Dupe;
use starlark::values::{
    none::NoneType, string::StarlarkStr, AllocValue, Demand, Freeze, Heap, NoSerialize,
    ProvidesStaticType, StarlarkValue, Trace, Value, ValueError,
};
use starlark_derive::starlark_value;
use tree_sitter::{Node as TSNode, Query, QueryMatch as TSQueryMatch};

#[derive(new, Clone, Debug, ProvidesStaticType, NoSerialize, Allocative, Dupe)]
pub struct QueryCaptures<'v> {
    #[allocative(skip)]
    query: &'v Query,

    #[allocative(skip)]
    pub query_match: &'v TSQueryMatch<'v, 'v>,
}

unsafe impl<'v> Trace<'v> for QueryCaptures<'v> {
    fn trace(&mut self, _tracer: &starlark::values::Tracer<'v>) {}
}

#[starlark_value(type = "QueryCaptures")]
impl<'v> StarlarkValue<'v> for QueryCaptures<'v> {
    fn provide(&'v self, demand: &mut Demand<'_, 'v>) {
        demand.provide_value(self)
    }

    fn is_in(&self, other: Value<'v>) -> anyhow::Result<bool> {
        let Some(name) = other.unpack_starlark_str() else {
            return Ok(false);
        };
        Ok(self.query.capture_index_for_name(name).is_some())
    }

    fn at(&self, index: Value<'v>, heap: &'v Heap) -> anyhow::Result<Value<'v>> {
        let Some(name) = index.unpack_starlark_str().map(StarlarkStr::as_str) else {
            return ValueError::unsupported_with(self, "[]", index);
        };
        let Some(idx) = self.query.capture_index_for_name(name) else {
            return Err(ValueError::KeyNotFound(name.into()).into());
        };
        let node = Node(&self.query_match.captures[idx as usize].node);
        Ok(heap.alloc(node))
    }
}

impl<'v> AllocValue<'v> for QueryCaptures<'v> {
    fn alloc_value(self, heap: &'v starlark::values::Heap) -> starlark::values::Value<'v> {
        heap.alloc_complex(self)
    }
}

impl Freeze for QueryCaptures<'_> {
    type Frozen = NoneType;

    fn freeze(self, _freezer: &starlark::values::Freezer) -> anyhow::Result<Self::Frozen> {
        panic!("{} should never get frozen", Self::TYPE);
    }
}

impl Display for QueryCaptures<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <Self as StarlarkValue>::TYPE.fmt(f)
    }
}

#[derive(Clone, Debug, ProvidesStaticType, NoSerialize, Allocative, Dupe)]
struct Node<'v>(#[allocative(skip)] &'v TSNode<'v>);

unsafe impl<'v> Trace<'v> for Node<'v> {
    fn trace(&mut self, _tracer: &starlark::values::Tracer<'v>) {}
}

impl<'v> Deref for Node<'v> {
    type Target = TSNode<'v>;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

#[starlark_value(type = "Node")]
impl<'v> StarlarkValue<'v> for Node<'v> {
    fn provide(&'v self, demand: &mut Demand<'_, 'v>) {
        demand.provide_value(self)
    }
}

impl<'v> AllocValue<'v> for Node<'v> {
    fn alloc_value(self, heap: &'v Heap) -> Value<'v> {
        heap.alloc_complex(self)
    }
}

impl Freeze for Node<'_> {
    type Frozen = NoneType;

    fn freeze(self, _freezer: &starlark::values::Freezer) -> anyhow::Result<Self::Frozen> {
        panic!("{} should never get frozen", Self::TYPE);
    }
}

impl Display for Node<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.to_sexp().fmt(f)
    }
}
