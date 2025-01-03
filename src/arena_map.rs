use std::{
    borrow::Borrow,
    collections::{btree_map::Entry, BTreeMap},
    mem,
    ops::Index,
    ptr,
    sync::RwLock,
};

use crate::result::Result;

#[derive(Debug)]
pub struct ArenaMap<K, V> {
    indices: RwLock<BTreeMap<K, usize>>,
    values: RwLock<Arena<V>>,
}

impl<K, V> ArenaMap<K, V> {
    pub fn new() -> Self {
        Self {
            indices: RwLock::new(BTreeMap::new()),
            values: RwLock::new(Arena::new()),
        }
    }
}

impl<K: Ord + Clone, V> ArenaMap<K, V> {
    pub fn get(&self, key: impl Borrow<K>) -> Option<&V> {
        let key = key.borrow();

        let indices = self.indices.read().unwrap();
        let values = self.values.read().unwrap();

        let index = indices.get(key)?;
        let value = &values[*index];
        // SAFETY: *value is never moved.
        Some(unsafe { &*ptr::from_ref(value) })
    }

    pub fn get_or_init(&self, key: &K, init: impl FnOnce() -> Result<V>) -> Result<&V> {
        if let Some(v) = self.get(key) {
            return Ok(v);
        }

        let value = init()?;

        let mut indices = self.indices.write().unwrap();
        let mut values = self.values.write().unwrap();

        let num_indices = indices.len();
        match indices.entry(key.borrow().clone()) {
            Entry::Occupied(oe) => {
                // In this case, another thread has already inserted a V into
                // this entry, so that can be safely returned.

                let value = &values[*oe.get()];
                // SAFETY: *value is never moved.
                Ok(unsafe { &*ptr::from_ref(value) })
            }
            Entry::Vacant(ve) => {
                ve.insert(num_indices);

                let value = values.alloc(value);
                // SAFETY: *value is never moved.
                Ok(unsafe { &*ptr::from_ref(value) })
            }
        }
    }
}

#[derive(Debug)]
struct Arena<T> {
    current_chunk: Vec<T>,
    older_chunks: Vec<Vec<T>>,
}

impl<T> Arena<T> {
    const CHUNK_SIZE: usize = 16;

    fn new() -> Self {
        Self {
            current_chunk: Vec::with_capacity(Self::CHUNK_SIZE),
            older_chunks: Vec::new(),
        }
    }

    fn get(&self, index: usize) -> Option<&T> {
        let num_older_entries = self.older_chunks.len() * Self::CHUNK_SIZE;
        if index < num_older_entries {
            return Some(&self.older_chunks[index / Self::CHUNK_SIZE][index % Self::CHUNK_SIZE]);
        }

        if index - num_older_entries < self.current_chunk.len() {
            return Some(&self.current_chunk[index - num_older_entries]);
        }

        None
    }

    fn alloc(&mut self, t: T) -> &T {
        if self.current_chunk.len() >= self.current_chunk.capacity() {
            let old_current_chunk = mem::replace(
                &mut self.current_chunk,
                Vec::with_capacity(Self::CHUNK_SIZE),
            );
            self.older_chunks.push(old_current_chunk);
        }

        self.current_chunk.push(t);
        debug_assert_eq!(self.current_chunk.capacity(), Self::CHUNK_SIZE);
        self.current_chunk
            .last()
            .expect("internal error: current chunk empty")
    }
}

impl<T> Index<usize> for Arena<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        self.get(index)
            .expect("internal error: index out of bounds")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn typical_usage() {
        let arena_map: ArenaMap<&str, &str> = ArenaMap::new();
        assert_eq!(arena_map.get("absent"), None);
        assert_eq!(
            arena_map.get_or_init(&"new", || Ok("value")).unwrap(),
            &"value"
        );
        assert_eq!(arena_map.get("new").unwrap(), &"value")
    }

    #[test]
    fn high_load() {
        let arena_map: ArenaMap<i32, i32> = ArenaMap::new();
        for i in 1..(Arena::<i32>::CHUNK_SIZE as i32) * 25 {
            arena_map.get_or_init(&i, || Ok(-i)).unwrap();
        }
        for i in 1..(Arena::<i32>::CHUNK_SIZE as i32) * 25 {
            assert_eq!(arena_map.get(i).copied(), Some(-i));
        }
    }
}
