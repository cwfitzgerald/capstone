use fnv::FnvBuildHasher;
use indexmap::map::IndexMap;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Central datastructure to a lot of rend3's inner workings. Wraps an IndexMap with convenience functions
/// as well as allows a way to "allocate" handles from a shared reference.
#[derive(Debug)]
pub struct ResourceRegistry<T> {
    mapping: IndexMap<usize, T, FnvBuildHasher>,
    current_idx: AtomicUsize,
}
impl<T> ResourceRegistry<T> {
    /// Create a new resource registry.
    pub fn new() -> Self {
        Self {
            mapping: IndexMap::with_hasher(FnvBuildHasher::default()),
            current_idx: AtomicUsize::new(0),
        }
    }

    /// Allocate a handle to later insert into the registry. Guaranteed unique.
    pub fn allocate(&self) -> usize {
        self.current_idx.fetch_add(1, Ordering::Relaxed)
    }

    /// Searches for the given in the data structure. If it doesn't exist, allocates
    /// a new handle. If it does, returns that handle.
    pub fn allocate_or_reuse(&self, handle: usize) -> usize {
        if self.mapping.contains_key(&handle) {
            handle
        } else {
            self.allocate()
        }
    }

    /// Inserts a handle/value pair. If the key exists, the value is updated.
    ///
    /// Returns the index corresponding to the key.
    pub fn insert(&mut self, handle: usize, data: T) -> usize {
        self.mapping.insert_full(handle, data).0
    }

    /// Removes a handle and it's value. Panics on invalid handle.
    ///
    /// Returns a tuple of the index into the array and the handle. It is part of the external api
    /// that this uses `IndexMap::swap_remove_full` and you may rely on this fact.
    pub fn remove(&mut self, handle: usize) -> (usize, T) {
        let (index, _key, value) = self.mapping.swap_remove_full(&handle).expect("Invalid handle");
        (index, value)
    }

    /// Returns an iterator over tuples of references to handles and value.
    pub fn iter(&self) -> impl Iterator<Item = (&usize, &T)> {
        self.mapping.iter()
    }

    /// Returns an iterator over shared value references.
    pub fn values(&self) -> impl Iterator<Item = &T> {
        self.mapping.values()
    }

    /// Returns an iterator over unique value references.
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.mapping.values_mut()
    }

    /// Looks up a handle and returns a shared reference to the value.
    pub fn get(&self, handle: usize) -> &T {
        self.mapping.get(&handle).unwrap()
    }

    /// Looks up a handle and returns a shared reference to the value if the handle exists.
    pub fn try_get(&self, handle: usize) -> Option<&T> {
        self.mapping.get(&handle)
    }

    /// Looks up a handle and returns a unique reference to the value.
    pub fn get_mut(&mut self, handle: usize) -> &mut T {
        self.mapping.get_mut(&handle).unwrap()
    }

    /// Looks up a handle and returns it's index into the array.
    pub fn get_index_of(&self, handle: usize) -> usize {
        self.mapping.get_index_of(&handle).unwrap()
    }

    /// Returns the amount of items currently in the registry.
    pub fn count(&self) -> usize {
        self.mapping.len()
    }
}
