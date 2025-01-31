use std::cmp::Ordering;
use std::ops::Deref;

pub type Key<T: Ord> = T;

pub type ValueOffset = u64;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct KeyValue<T: Ord> {
    key: Key<T>,
    offset: Vec<ValueOffset>,
}

pub struct SortedIndex<T: Ord> {
    pub entries: Vec<KeyValue<T>>,
}

impl<T: Ord> Default for SortedIndex<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Ord> SortedIndex<T> {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn build_index(&mut self, mut data: Vec<KeyValue<T>>) {
        data.sort_by(|a, b| a.key.cmp(&b.key));
        self.entries = data;
    }

    pub fn get_offset(&self, key: &Key<T>) -> Option<&[ValueOffset]> {
        self.entries
            .binary_search_by_key(&key, |k| &k.key)
            .map(|i| self.entries[i].offset.as_slice())
            .ok()
    }
}
