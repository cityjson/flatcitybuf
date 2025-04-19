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
    /// Generic query using a comparison operator and a key.
    pub fn query(&mut self, cmp: Comparison, key: &K) -> Result<Vec<Offset>, Error> {
        match cmp {
            Comparison::Eq => self.find_eq(key),
            Comparison::Ne => self.find_ne(key),
            Comparison::Gt => self.find_gt(key),
            Comparison::Ge => self.find_ge(key),
            Comparison::Lt => self.find_lt(key),
            Comparison::Le => self.find_le(key),
        }
    }

    /// Exact match: all offsets where key == target.
    pub fn find_eq(&mut self, target: &K) -> Result<Vec<Offset>, Error> {
        // Call lower_bound to get the position of the first entry ≥ target
        let idx = self.lower_bound_index(target)?;
        
        // Check if we found a valid entry in the leaf layer
        let leaf_offset = self.leaf_offset();
        let total_entries = leaf_offset + self.len();
        
        if idx >= total_entries {
            // No keys ≥ target
            return Ok(Vec::new());
        }
        
        // Check if this is an exact match
        let entry = self.read_entry(idx)?;
        if &entry.key != target {
            // Not an exact match
            return Ok(Vec::new());
        }
        
        // Gather all duplicates
        let mut result = Vec::new();
        result.push(entry.offset);
        
        // Look left for more duplicates if we're not at the beginning
        if idx > leaf_offset {
            let mut left_idx = idx - 1;
            while left_idx >= leaf_offset {
                let ent = self.read_entry(left_idx)?;
                if &ent.key == target {
                    result.insert(0, ent.offset);
                    if left_idx == leaf_offset {
                        break;
                    }
                    left_idx -= 1;
                } else {
                    break;
                }
            }
        }
        
        // Look right for more duplicates
        let mut right_idx = idx + 1;
        while right_idx < total_entries {
            let ent = self.read_entry(right_idx)?;
            if &ent.key == target {
                result.push(ent.offset);
                right_idx += 1;
            } else {
                break;
            }
        }
        
        Ok(result)
    }

    /// First position >= target (alias for lower_bound), returns the offset of the first entry >= target.
    pub fn find_ge(&mut self, target: &K) -> Result<Vec<Offset>, Error> {
        let idx = self.lower_bound_index(target)?;
        let total = self.leaf_offset() + self.len();
        if idx >= total {
            Ok(Vec::new())
        } else {
            let e = self.read_entry(idx)?;
            Ok(vec![e.offset])
        }
    }

    /// First position > target, returns at most one offset.
    pub fn find_gt(&mut self, target: &K) -> Result<Vec<Offset>, Error> {
        let idx = self.upper_bound_index(target)?;
        let total = self.leaf_offset() + self.len();
        if idx >= total {
            Ok(Vec::new())
        } else {
            let e = self.read_entry(idx)?;
            Ok(vec![e.offset])
        }
    }

    /// All offsets <= target.
    pub fn find_le(&mut self, target: &K) -> Result<Vec<Offset>, Error> {
        // find first > target
        let end = self.upper_bound_index(target)?;
        let leaf_start = self.leaf_offset();
        let mut result = Vec::new();
        for i in leaf_start..end {
            let e = self.read_entry(i)?;
            result.push(e.offset);
        }
        Ok(result)
    }

    /// All offsets < target.
    pub fn find_lt(&mut self, target: &K) -> Result<Vec<Offset>, Error> {
        let end = self.lower_bound_index(target)?;
        let leaf_start = self.leaf_offset();
        let mut result = Vec::new();
        for i in leaf_start..end {
            let e = self.read_entry(i)?;
            result.push(e.offset);
        }
        Ok(result)
    }

    /// Offsets not equal to target (concatenate < and >).
    pub fn find_ne(&mut self, target: &K) -> Result<Vec<Offset>, Error> {
        let mut res = self.find_lt(target)?;
        res.extend(self.find_gt(target)?);
        Ok(res)
    }
}