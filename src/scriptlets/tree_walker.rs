use allocative::Allocative;
use derive_new::new;
use starlark::values::{NoSerialize, ProvidesStaticType};
use tree_sitter::TreeCursor;

#[derive(new, Clone, ProvidesStaticType, NoSerialize, Allocative)]
pub struct TreeWalker<'v>(#[allocative(skip)] TreeCursor<'v>);
