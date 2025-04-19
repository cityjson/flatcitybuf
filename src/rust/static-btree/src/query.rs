use crate::entry::Offset;
use crate::error::Error;
use crate::key::Key;
use crate::tree::StaticBTree;
use std::io::{Read, Seek};

/// Comparison operators supported by StaticBTree queries.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Comparison {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
}

impl<K: Key, R: Read + Seek> StaticBTree<K, R> {
    /// Execute a comparison query against the index and payload blocks.
    pub fn query(&mut self, cmp: Comparison, key: &K) -> Result<Vec<Offset>, Error> {
        // TODO: dispatch to find_eq, find_ne, etc., then collect offsets via payload chains
        unimplemented!("StaticBTree::query");
    }

    /// Exact match: collect record offsets for keys == target.
    pub fn find_eq(&mut self, key: &K) -> Result<Vec<Offset>, Error> {
        // TODO:
        // 1. Locate index entry for key (lower_bound_index)
        // 2. Read its block_ptr via read_entry
        // 3. Follow payload chain and return all offsets
        unimplemented!("StaticBTree::find_eq");
    }

    /// Not equal: union of record offsets for keys < and > target.
    pub fn find_ne(&mut self, key: &K) -> Result<Vec<Offset>, Error> {
        // TODO: combine payloads from find_lt and find_gt
        unimplemented!("StaticBTree::find_ne");
    }

    /// Greater than: all record offsets for keys > target.
    pub fn find_gt(&mut self, key: &K) -> Result<Vec<Offset>, Error> {
        // TODO:
        // 1. Determine starting index entry (upper_bound_index)
        // 2. Iterate index entries > target
        // 3. For each, follow payload chain and collect offsets
        unimplemented!("StaticBTree::find_gt");
    }

    /// Greater than or equal: offsets for keys >= target.
    pub fn find_ge(&mut self, key: &K) -> Result<Vec<Offset>, Error> {
        // TODO: similar to find_gt but include key == target
        unimplemented!("StaticBTree::find_ge");
    }

    /// Less than: offsets for keys < target.
    pub fn find_lt(&mut self, key: &K) -> Result<Vec<Offset>, Error> {
        // TODO: locate first >= key then iterate lower entries
        unimplemented!("StaticBTree::find_lt");
    }

    /// Less than or equal: offsets for keys <= target.
    pub fn find_le(&mut self, key: &K) -> Result<Vec<Offset>, Error> {
        // TODO: locate first > key then iterate lower entries
        unimplemented!("StaticBTree::find_le");
    }
}
