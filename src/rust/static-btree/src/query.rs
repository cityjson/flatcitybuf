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
// Test comparison operators using a small B+Tree with duplicates
#[cfg(test)]
mod tests {
    use super::*;
    use crate::StaticBTreeBuilder;
    use std::io::Cursor;

    fn make_tree() -> StaticBTree<u32, Cursor<Vec<u8>>> {
        let mut builder = StaticBTreeBuilder::<u32>::new(2);
        builder.push(1, 10);
        builder.push(2, 20);
        builder.push(2, 21);
        builder.push(2, 22);
        builder.push(3, 30);
        let data = builder.build().unwrap();
        let cursor = Cursor::new(data);
        StaticBTree::new(cursor, 2, 3).unwrap()
    }

    #[test]
    fn test_find_eq() {
        let mut tree = make_tree();
        assert_eq!(tree.find_eq(&2).unwrap(), vec![20, 21, 22]);
        assert_eq!(tree.find_eq(&4).unwrap(), Vec::<u64>::new());
    }

    #[test]
    fn test_find_ne() {
        let mut tree = make_tree();
        assert_eq!(tree.find_ne(&2).unwrap(), vec![10, 30]);
    }

    #[test]
    fn test_find_gt_ge() {
        let mut tree = make_tree();
        assert_eq!(tree.find_gt(&2).unwrap(), vec![30]);
        assert_eq!(tree.find_ge(&2).unwrap(), vec![20, 21, 22, 30]);
    }

    #[test]
    fn test_find_lt_le() {
        let mut tree = make_tree();
        assert_eq!(tree.find_lt(&2).unwrap(), vec![10]);
        assert_eq!(tree.find_le(&2).unwrap(), vec![10, 20, 21, 22]);
    }

    #[test]
    fn test_query_dispatch() {
        let mut tree = make_tree();
        assert_eq!(tree.query(Comparison::Eq, &3).unwrap(), vec![30]);
        assert_eq!(tree.query(Comparison::Ne, &1).unwrap(), vec![20, 21, 22, 30]);
        assert_eq!(tree.query(Comparison::Gt, &1).unwrap(), vec![20, 21, 22, 30]);
        assert_eq!(tree.query(Comparison::Lt, &3).unwrap(), vec![10, 20, 21, 22]);
    }
    #[test]
    fn test_float_and_string_keys() {
        use ordered_float::OrderedFloat;
        use crate::key::FixedStringKey;
        // float keys
        let mut fb = StaticBTreeBuilder::<OrderedFloat<f32>>::new(3);
        fb.push(OrderedFloat(2.0), 200);
        fb.push(OrderedFloat(1.0), 100);
        fb.push(OrderedFloat(2.0), 201);
        let data = fb.build().unwrap();
        let cursor = Cursor::new(data);
        let mut ft = StaticBTree::<OrderedFloat<f32>, _>::new(cursor, 3, 2).unwrap();
        assert_eq!(ft.find_eq(&OrderedFloat(2.0)).unwrap(), vec![200, 201]);
        assert_eq!(ft.find_lt(&OrderedFloat(2.0)).unwrap(), vec![100]);
        assert_eq!(ft.find_gt(&OrderedFloat(1.0)).unwrap(), vec![200, 201]);
        // string keys
        let mut sb = StaticBTreeBuilder::<FixedStringKey<4>>::new(2);
        sb.push(FixedStringKey::<4>::from_str("aa"), 1);
        sb.push(FixedStringKey::<4>::from_str("bb"), 2);
        sb.push(FixedStringKey::<4>::from_str("aa"), 3);
        let data2 = sb.build().unwrap();
        let cursor2 = Cursor::new(data2);
        let mut st = StaticBTree::<FixedStringKey<4>, _>::new(cursor2, 2, 2).unwrap();
        assert_eq!(st.find_eq(&FixedStringKey::from_str("aa")).unwrap(), vec![1, 3]);
        assert_eq!(st.find_ne(&FixedStringKey::from_str("aa")).unwrap(), vec![2]);
    }
    #[test]
    fn test_empty_and_not_found() {
        let mut b = StaticBTreeBuilder::<u32>::new(3);
        b.push(1, 1);
        b.push(2, 2);
        let data = b.build().unwrap();
        let cursor = Cursor::new(data);
        let mut t = StaticBTree::<u32, _>::new(cursor, 3, 2).unwrap();
        // not found cases
        assert!(t.find_eq(&3).unwrap().is_empty());
        assert!(t.find_lt(&1).unwrap().is_empty());
        assert!(t.find_le(&0).unwrap().is_empty());
        assert!(t.find_gt(&2).unwrap().is_empty());
        assert!(t.find_ge(&3).unwrap().is_empty());
        // non-eq ne
        assert_eq!(t.find_ne(&1).unwrap(), vec![2]);
    }
}

impl<K: Key, R: Read + Seek> StaticBTree<K, R> {
    /// Execute a comparison query against the index and payload blocks.
    /// Execute a comparison query against the index and payload blocks.
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

    /// Exact match: collect record offsets for keys == target.
    pub fn find_eq(&mut self, key: &K) -> Result<Vec<Offset>, Error> {
        // TODO:
        // 1. Locate index entry for key (lower_bound_index)
        // 2. Read its block_ptr via read_entry
        // 3. Follow payload chain and return all offsets
        // Locate the first occurrence
        let idx = self.lower_bound_index(key)?;
        // Read entry and verify key equality
        let entry = self.read_entry(idx)?;
        if &entry.key != key {
            return Ok(Vec::new());
        }
        // Load all payload offsets for this key
        self.read_all_offsets(entry.offset)
    }

    /// Not equal: union of record offsets for keys < and > target.
    pub fn find_ne(&mut self, key: &K) -> Result<Vec<Offset>, Error> {
        // TODO: combine payloads from find_lt and find_gt
        let mut result = Vec::new();
        // offsets for keys < target
        result.extend(self.find_lt(key)?);
        // offsets for keys > target
        result.extend(self.find_gt(key)?);
        Ok(result)
    }

    /// Greater than: all record offsets for keys > target.
    pub fn find_gt(&mut self, key: &K) -> Result<Vec<Offset>, Error> {
        // TODO:
        // 1. Determine starting index entry (upper_bound_index)
        // 2. Iterate index entries > target
        // 3. For each, follow payload chain and collect offsets
        let mut result = Vec::new();
        let start = self.upper_bound_index(key)?;
        let total = self.len();
        if start >= total {
            return Ok(result);
        }
        let b = self.branching_factor();
        let first_node = start / b;
        let last_node = (total - 1) / b;
        let mut in_node = start % b;
        for node_idx in first_node..=last_node {
            let entries = self.read_node(0, node_idx)?;
            let start_j = if node_idx == first_node { in_node } else { 0 };
            let end_j = if node_idx == last_node { total - node_idx * b } else { b };
            for j in start_j..end_j {
                result.extend(self.read_all_offsets(entries[j].offset)?);
            }
        }
        Ok(result)
    }

    /// Greater than or equal: offsets for keys >= target.
    pub fn find_ge(&mut self, key: &K) -> Result<Vec<Offset>, Error> {
        // TODO: similar to find_gt but include key == target
        let mut result = Vec::new();
        let start = self.lower_bound_index(key)?;
        let total = self.len();
        if start >= total {
            return Ok(result);
        }
        let b = self.branching_factor();
        let first_node = start / b;
        let last_node = (total - 1) / b;
        let mut in_node = start % b;
        for node_idx in first_node..=last_node {
            let entries = self.read_node(0, node_idx)?;
            let start_j = if node_idx == first_node { in_node } else { 0 };
            let end_j = if node_idx == last_node { total - node_idx * b } else { b };
            for j in start_j..end_j {
                result.extend(self.read_all_offsets(entries[j].offset)?);
            }
        }
        Ok(result)
    }

    /// Less than: offsets for keys < target.
    pub fn find_lt(&mut self, key: &K) -> Result<Vec<Offset>, Error> {
        // TODO: locate first >= key then iterate lower entries
        let mut result = Vec::new();
        let end = self.lower_bound_index(key)?;
        if end == 0 {
            return Ok(result);
        }
        let b = self.branching_factor();
        let last_node = (end - 1) / b;
        for node_idx in 0..=last_node {
            let entries = self.read_node(0, node_idx)?;
            let start_j = 0;
            let end_j = if node_idx == last_node { end - node_idx * b } else { b };
            for j in start_j..end_j {
                result.extend(self.read_all_offsets(entries[j].offset)?);
            }
        }
        Ok(result)
    }

    /// Less than or equal: offsets for keys <= target.
    pub fn find_le(&mut self, key: &K) -> Result<Vec<Offset>, Error> {
        // TODO: locate first > key then iterate lower entries
        let mut result = Vec::new();
        let end = self.upper_bound_index(key)?;
        if end == 0 {
            return Ok(result);
        }
        let b = self.branching_factor();
        let last_node = (end - 1) / b;
        for node_idx in 0..=last_node {
            let entries = self.read_node(0, node_idx)?;
            let start_j = 0;
            let end_j = if node_idx == last_node { end - node_idx * b } else { b };
            for j in start_j..end_j {
                result.extend(self.read_all_offsets(entries[j].offset)?);
            }
        }
        Ok(result)
    }
}
