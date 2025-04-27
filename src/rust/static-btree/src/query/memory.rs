use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom, Write};

use crate::entry::Entry;
use crate::error::{Error, Result};
use crate::key::Key;
use crate::query::types::{Operator, Query, QueryCondition, SearchIndex};
use crate::stree::Stree;

/// In-memory index implementation that wraps the Stree structure
// NOTE: This can be type alias for Stree later
pub struct MemoryIndex<K: Key> {
    /// The underlying static B-tree
    stree: Stree<K>,
}

impl<K: Key> MemoryIndex<K> {
    /// Create a new memory index from an existing Stree
    pub fn new(mut data: impl Read, num_items: usize, branching_factor: u16) -> Result<Self> {
        let stree = Stree::from_buf(&mut data, num_items, branching_factor)?;

        Ok(Self { stree })
    }

    /// Build a memory index from a collection of entries
    pub fn build(entries: &[Entry<K>], branching_factor: u16) -> Result<Self> {
        let stree = Stree::build(entries, branching_factor)?;

        Ok(Self { stree })
    }

    pub fn from_buf(mut data: impl Read, num_items: usize, branching_factor: u16) -> Result<Self> {
        let stree = Stree::from_buf(&mut data, num_items, branching_factor)?;

        Ok(Self { stree })
    }

    pub fn num_items(&self) -> usize {
        self.stree.num_items()
    }

    fn serialize(&self, out: &mut impl Write) -> Result<()> {
        self.stree.stream_write(out)
    }
}

impl<K: Key> SearchIndex<K> for MemoryIndex<K> {
    fn find_exact(&self, key: K) -> Result<Vec<u64>> {
        let results = self.stree.find_exact(key)?;
        Ok(results.into_iter().map(|item| item.offset as u64).collect())
    }

    fn find_range(&self, start: Option<K>, end: Option<K>) -> Result<Vec<u64>> {
        match (start, end) {
            (Some(start_key), Some(end_key)) => {
                let results = self.stree.find_range(start_key, end_key)?;
                Ok(results.into_iter().map(|item| item.offset as u64).collect())
            }
            (Some(start_key), None) => {
                // Find all items >= start_key
                let results = self.stree.find_range(start_key, K::max_value())?;
                Ok(results.into_iter().map(|item| item.offset as u64).collect())
            }
            (None, Some(end_key)) => {
                // Find all items <= end_key
                let results = self.stree.find_range(K::min_value(), end_key)?;
                Ok(results.into_iter().map(|item| item.offset as u64).collect())
            }
            (None, None) => {
                return Err(Error::QueryError(
                    "find_range requires at least one bound".to_string(),
                ));
            }
        }
    }
}

/// Container for multiple in-memory indices
pub struct MemoryMultiIndex<K: Key> {
    /// Map of field names to corresponding indices
    indices: HashMap<String, MemoryIndex<K>>,
}

impl<K: Key> MemoryMultiIndex<K> {
    /// Create a new empty multi-index
    pub fn new() -> Self {
        Self {
            indices: HashMap::new(),
        }
    }

    /// Add an index for a specific field
    pub fn add_index(&mut self, field: String, index: MemoryIndex<K>) {
        self.indices.insert(field, index);
    }

    /// (field, start, num_items, branching_factor)
    pub fn from_buf(
        mut data: impl Read + Seek,
        indices: Vec<(String, usize, usize, u16)>,
    ) -> Result<Self> {
        let mut multi_index = Self::new();
        for (field, start, num_items, branching_factor) in indices.iter() {
            data.seek(SeekFrom::Start(*start as u64))?;
            let index = MemoryIndex::from_buf(&mut data, *num_items, *branching_factor)?;
            multi_index.add_index(field.clone(), index);
        }
        Ok(multi_index)
    }

    /// Get an index by field name
    pub fn get_index(&self, field: &str) -> Option<&MemoryIndex<K>> {
        self.indices.get(field)
    }

    /// Execute a query
    pub fn query(&self, query: &Query<K>) -> Result<Vec<u64>> {
        if query.conditions.is_empty() {
            return Err(Error::QueryError("query cannot be empty".to_string()));
        }

        // Process the first condition to initialize the result set
        let first_condition = &query.conditions[0];
        let mut result_set = self.process_condition(first_condition)?;

        // Process remaining conditions with set intersection
        for condition in &query.conditions[1..] {
            let condition_results = self.process_condition(condition)?;

            // Perform intersection (AND logic)
            result_set.retain(|offset| condition_results.contains(offset));

            // If result set is empty, we can short-circuit
            if result_set.is_empty() {
                break;
            }
        }

        Ok(result_set)
    }

    // Helper method to process a single condition
    fn process_condition(&self, condition: &QueryCondition<K>) -> Result<Vec<u64>> {
        let index = self.indices.get(&condition.field).ok_or_else(|| {
            Error::QueryError(format!("no index found for field '{}'", condition.field))
        })?;

        match condition.operator {
            Operator::Eq => index.find_exact(condition.key.clone()),
            Operator::Ne => {
                // Return all items except those matching the key
                let all_items = index.find_range(Some(K::min_value()), Some(K::max_value()))?;
                let matching_items = index.find_exact(condition.key.clone())?;

                Ok(all_items
                    .into_iter()
                    .filter(|item| !matching_items.contains(item))
                    .collect())
            }
            Operator::Gt => index
                .find_range(Some(condition.key.clone()), None)
                .and_then(|mut results| {
                    // Remove exact matches
                    let exact_matches = index.find_exact(condition.key.clone())?;
                    results.retain(|item| !exact_matches.contains(item));
                    Ok(results)
                }),
            Operator::Lt => index
                .find_range(None, Some(condition.key.clone()))
                .and_then(|mut results| {
                    // Remove exact matches
                    let exact_matches = index.find_exact(condition.key.clone())?;
                    results.retain(|item| !exact_matches.contains(item));
                    Ok(results)
                }),
            Operator::Ge => index.find_range(Some(condition.key.clone()), None),
            Operator::Le => index.find_range(None, Some(condition.key.clone())),
        }
    }
}

impl<K: Key> Default for MemoryMultiIndex<K> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::Entry;

    #[test]
    fn test_memory_index_build_and_find_exact() -> Result<()> {
        let entries = vec![
            Entry::new(10, 100),
            Entry::new(20, 200),
            Entry::new(30, 300),
            Entry::new(40, 400),
        ];

        let index = MemoryIndex::build(&entries, 4)?;

        // Test exact match
        let results = index.find_exact(20)?;
        assert_eq!(results, vec![200]);

        // Test non-existent key
        let results = index.find_exact(25)?;
        assert!(results.is_empty());

        Ok(())
    }

    #[test]
    fn test_memory_index_find_range() -> Result<()> {
        let entries = vec![
            Entry::new(10, 100),
            Entry::new(20, 200),
            Entry::new(30, 300),
            Entry::new(40, 400),
        ];

        let index = MemoryIndex::build(&entries, 4)?;

        // Test inclusive range
        let results = index.find_range(Some(20), Some(30))?;
        assert_eq!(results, vec![200, 300]);

        // Test range with only start bound
        let results = index.find_range(Some(30), None)?;
        assert_eq!(results, vec![300, 400]);

        // Test range with only end bound
        let results = index.find_range(None, Some(20))?;
        assert_eq!(results, vec![100, 200]);

        // Test empty range
        let results = index.find_range(Some(35), Some(38))?;
        assert!(results.is_empty());

        Ok(())
    }

    #[test]
    fn test_memory_multi_index_query() -> Result<()> {
        // Create entries for "age" field
        let age_entries = vec![
            Entry::new(20, 1), // id=1, age=20
            Entry::new(30, 2), // id=2, age=30
            Entry::new(25, 3), // id=3, age=25
            Entry::new(40, 4), // id=4, age=40
        ];

        // Create entries for "score" field
        let score_entries = vec![
            Entry::new(85, 1), // id=1, score=85
            Entry::new(90, 2), // id=2, score=90
            Entry::new(75, 3), // id=3, score=75
            Entry::new(95, 4), // id=4, score=95
        ];

        // Build indices
        let age_index = MemoryIndex::build(&age_entries, 4)?;
        let score_index = MemoryIndex::build(&score_entries, 4)?;

        // Create multi-index
        let mut multi_index = MemoryMultiIndex::new();
        multi_index.add_index("age".to_string(), age_index);
        multi_index.add_index("score".to_string(), score_index);

        // Query: age >= 25 AND score >= 85
        let mut query = Query::new();
        query.add_condition("age".to_string(), Operator::Ge, 25);
        query.add_condition("score".to_string(), Operator::Ge, 85);

        let results = multi_index.query(&query)?;
        assert_eq!(results, vec![2, 4]); // Matches id=2 and id=4

        // Query: age < 30 AND score < 90
        let mut query = Query::new();
        query.add_condition("age".to_string(), Operator::Lt, 30);
        query.add_condition("score".to_string(), Operator::Lt, 90);

        let results = multi_index.query(&query)?;
        assert_eq!(results, vec![1, 3]); // Matches id=1 and id=3

        // Query: age = 40 AND score = 95
        let mut query = Query::new();
        query.add_condition("age".to_string(), Operator::Eq, 40);
        query.add_condition("score".to_string(), Operator::Eq, 95);

        let results = multi_index.query(&query)?;
        assert_eq!(results, vec![4]); // Matches only id=4

        Ok(())
    }
}
