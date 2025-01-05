use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use allocative::Allocative;
use dupe::Dupe;
use starlark::{collections::StarlarkHashValue, values::Trace};

use crate::query::Query;

#[derive(Debug, Allocative)]
pub struct QueryCacheForLanguage {
    cache: RwLock<HashMap<StarlarkHashValue, CachedQuery>>,
}

impl QueryCacheForLanguage {
    pub fn new() -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            cache: RwLock::new(HashMap::with_capacity(capacity)),
        }
    }

    pub fn put(&self, query_hash: StarlarkHashValue, query: Arc<Query>) {
        self.cache
            .write()
            .unwrap()
            .insert(query_hash, CachedQuery(query.dupe()));
    }

    pub fn get(&self, query_hash: StarlarkHashValue) -> Option<Arc<Query>> {
        self.cache
            .read()
            .unwrap()
            .get(&query_hash)
            .map(|q| q.0.dupe())
    }
}

unsafe impl<'v> Trace<'v> for &'v QueryCacheForLanguage {
    fn trace(&mut self, _tracer: &starlark::values::Tracer<'v>) {}
}

impl Default for QueryCacheForLanguage {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, Allocative)]
struct CachedQuery(#[allocative(skip)] Arc<Query>);

#[cfg(test)]
mod tests {
    use std::ptr;

    use starlark::values::Heap;

    use crate::context::{Context, Manifest};
    use crate::language::Language;

    use super::*;

    #[test]
    fn reparse_avoided() {
        let ctx = Context::new_with_manifest("test-path".into(), Manifest::default());

        let heap = Heap::new();
        let query_pair_1 = (Language::Rust, heap.alloc_str("(source_file) @file"));
        let query_pair_2 = (Language::Rust, heap.alloc_str("(binary_expression) @bin"));

        let parsed_query_pair_1_ptr = Arc::as_ptr(
            &ctx.language_data(&query_pair_1.0)
                .unwrap()
                .unwrap()
                .get_or_create_query(&query_pair_1.1)
                .unwrap(),
        );
        let parsed_query_pair_1_again_ptr = Arc::as_ptr(
            &ctx.language_data(&query_pair_1.0)
                .unwrap()
                .unwrap()
                .get_or_create_query(&query_pair_1.1)
                .unwrap(),
        );
        assert!(
            ptr::eq(parsed_query_pair_1_ptr, parsed_query_pair_1_again_ptr),
            "duplication not avoided"
        );

        let parsed_query_pair_2_ptr = Arc::as_ptr(
            &ctx.language_data(&query_pair_2.0)
                .unwrap()
                .unwrap()
                .get_or_create_query(&query_pair_2.1)
                .unwrap(),
        );
        assert!(
            !ptr::eq(parsed_query_pair_1_ptr, parsed_query_pair_2_ptr),
            "returned same query"
        );
    }

    #[test]
    fn same_query_different_language() {
        let ctx = Context::new_with_manifest("test-path".into(), Manifest::default());

        let heap = Heap::new();
        let query = heap.alloc_str("(source_file) @foo");
        let query_pair_1 = (Language::Rust, query);
        let query_pair_2 = (Language::Go, query);

        let parsed_query_pair_1_ptr = Arc::as_ptr(
            &ctx.language_data(&query_pair_1.0)
                .unwrap()
                .unwrap()
                .get_or_create_query(&query_pair_1.1)
                .unwrap(),
        );
        let parsed_query_pair_2_ptr = Arc::as_ptr(
            &ctx.language_data(&query_pair_2.0)
                .unwrap()
                .unwrap()
                .get_or_create_query(&query_pair_2.1)
                .unwrap(),
        );
        assert!(!ptr::eq(parsed_query_pair_1_ptr, parsed_query_pair_2_ptr));
    }
}
