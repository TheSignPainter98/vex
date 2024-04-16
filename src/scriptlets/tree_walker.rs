use std::fmt::{Debug, Display};

use allocative::Allocative;
use derive_new::new;
use starlark::values::{
    AllocValue, Heap, NoSerialize, ProvidesStaticType, StarlarkValue, Trace, Value,
};
use starlark_derive::starlark_value;
use tree_sitter::TreeCursor;

#[derive(new, Clone, ProvidesStaticType, NoSerialize, Allocative)]
pub struct TreeWalker<'v>(#[allocative(skip)] TreeCursor<'v>);

#[starlark_value(type = "TreeWalker")]
impl<'v> StarlarkValue<'v> for TreeWalker<'v> {}

impl Debug for TreeWalker<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <Self as Display>::fmt(self, f)
    }
}

unsafe impl<'v> Trace<'v> for TreeWalker<'v> {
    fn trace(&mut self, _tracer: &starlark::values::Tracer<'v>) {}
}

impl Display for TreeWalker<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", Self::TYPE)
    }
}

impl<'v> AllocValue<'v> for TreeWalker<'v> {
    fn alloc_value(self, heap: &'v Heap) -> Value<'v> {
        heap.alloc_complex_no_freeze(self)
    }
}
