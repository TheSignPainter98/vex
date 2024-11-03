use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use allocative::Allocative;
use dupe::Dupe;
use starlark::{
    collections::StarlarkHashValue,
    values::{StringValue, Trace},
};

use crate::{query::Query, result::Result, supported_language::SupportedLanguage};

#[derive(Debug, Allocative)]
pub struct QueryCache {
    cache: RwLock<HashMap<(SupportedLanguage, StarlarkHashValue), CachedQuery>>,
}

impl QueryCache {
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

    pub fn get_or_create(
        &self,
        language: SupportedLanguage,
        raw_query: StringValue<'_>,
    ) -> Result<Arc<Query>> {
        let query_hash = raw_query.get_hashed().hash(); // This hash value is only 32 bits long.

        if let Some(cached_query) = self
            .cache
            .read()
            .expect("internal error: cache lock poisoned")
            .get(&(language, query_hash))
        {
            return Ok(cached_query.0.dupe());
        }

        let query = Arc::new(Query::new(language, &raw_query)?);
        self.cache
            .write()
            .expect("internal error: cache lock poisoned")
            .insert((language, query_hash), CachedQuery(query.dupe()));
        Ok(query)
    }
}

unsafe impl<'v> Trace<'v> for &'v QueryCache {
    fn trace(&mut self, _tracer: &starlark::values::Tracer<'v>) {}
}

impl Default for QueryCache {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, Allocative)]
struct CachedQuery(#[allocative(skip)] Arc<Query>);

#[cfg(test)]
mod test {
    use std::ptr;

    use starlark::values::Heap;

    use super::*;

    #[test]
    fn reparse_avoided() {
        let heap = Heap::new();
        let query_pair_1 = (
            SupportedLanguage::Rust,
            heap.alloc_str("(source_file) @file"),
        );
        let query_pair_2 = (
            SupportedLanguage::Rust,
            heap.alloc_str("(binary_expression) @bin"),
        );
        let cache = QueryCache::with_capacity(2);

        let parsed_query_pair_1_ptr =
            Arc::as_ptr(&cache.get_or_create(query_pair_1.0, query_pair_1.1).unwrap());
        let parsed_query_pair_1_again_ptr =
            Arc::as_ptr(&cache.get_or_create(query_pair_1.0, query_pair_1.1).unwrap());
        assert!(
            ptr::eq(parsed_query_pair_1_ptr, parsed_query_pair_1_again_ptr),
            "duplication not avoided"
        );

        let parsed_query_pair_2_ptr =
            Arc::as_ptr(&cache.get_or_create(query_pair_2.0, query_pair_2.1).unwrap());
        assert!(
            !ptr::eq(parsed_query_pair_1_ptr, parsed_query_pair_2_ptr),
            "returned same query"
        );
    }

    #[test]
    fn same_query_different_language() {
        let heap = Heap::new();
        let query = heap.alloc_str("(source_file) @foo");
        let query_pair_1 = (SupportedLanguage::Rust, query);
        let query_pair_2 = (SupportedLanguage::Go, query);
        let cache = QueryCache::with_capacity(2);

        let parsed_query_pair_1_ptr =
            Arc::as_ptr(&cache.get_or_create(query_pair_1.0, query_pair_1.1).unwrap());
        let parsed_query_pair_2_ptr =
            Arc::as_ptr(&cache.get_or_create(query_pair_2.0, query_pair_2.1).unwrap());
        assert!(!ptr::eq(parsed_query_pair_1_ptr, parsed_query_pair_2_ptr));
    }
}
