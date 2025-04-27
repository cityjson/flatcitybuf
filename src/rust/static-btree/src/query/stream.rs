use std::collections::HashMap;
use std::io::{Read, Seek};
use std::marker::PhantomData;

use crate::error::{Error, Result};
use crate::key::Key;
use crate::query::types::{Operator, Query, QueryCondition, SearchIndex};
use crate::stree::{SearchResultItem, Stree};

/// Stream-based index for file access
pub struct StreamIndex<K: Key> {
    /// Number of items in the index
    num_items: usize,
    /// Branching factor of the tree
    branching_factor: u16,
    /// Offset of the index in the file
    index_offset: u64,
    /// Size of the payload section
    payload_size: usize,
    /// Phantom marker for the key type
    _marker: PhantomData<K>,
}

impl<K: Key> StreamIndex<K> {
    /// Create a new stream index with metadata
    pub fn new(
        num_items: usize,
        branching_factor: u16,
        index_offset: u64,
        payload_size: usize,
    ) -> Self {
        Self {
            num_items,
            branching_factor,
            index_offset,
            payload_size,
            _marker: PhantomData,
        }
    }

    /// Get the number of items in the index
    pub fn num_items(&self) -> usize {
        self.num_items
    }

    /// Get the branching factor of the tree
    pub fn branching_factor(&self) -> u16 {
        self.branching_factor
    }

    /// Get the index offset
    pub fn index_offset(&self) -> u64 {
        self.index_offset
    }

    /// Get the payload size
    pub fn payload_size(&self) -> usize {
        self.payload_size
    }

    /// Find exact matches using a reader
    pub fn find_exact_with_reader<R: Read + Seek>(
        &self,
        reader: &mut R,
        key: K,
    ) -> Result<Vec<u64>> {
        let results = Stree::stream_find_exact(
            reader,
            self.num_items,
            self.branching_factor,
            key,
            self.payload_size,
        )?;

        Ok(results.into_iter().map(|item| item.offset as u64).collect())
    }

    /// Find range matches using a reader
    pub fn find_range_with_reader<R: Read + Seek>(
        &self,
        reader: &mut R,
        start: Option<K>,
        end: Option<K>,
    ) -> Result<Vec<u64>> {
        match (start, end) {
            (Some(start_key), Some(end_key)) => {
                let results = Stree::stream_find_range(
                    reader,
                    self.num_items,
                    self.branching_factor,
                    start_key,
                    end_key,
                    self.payload_size,
                )?;
                Ok(results.into_iter().map(|item| item.offset as u64).collect())
            }
            (Some(start_key), None) => {
                // Find all items >= start_key
                let results = Stree::stream_find_range(
                    reader,
                    self.num_items,
                    self.branching_factor,
                    start_key,
                    K::max_value(),
                    self.payload_size,
                )?;
                Ok(results.into_iter().map(|item| item.offset as u64).collect())
            }
            (None, Some(end_key)) => {
                // Find all items <= end_key
                let results = Stree::stream_find_range(
                    reader,
                    self.num_items,
                    self.branching_factor,
                    K::min_value(),
                    end_key,
                    self.payload_size,
                )?;
                Ok(results.into_iter().map(|item| item.offset as u64).collect())
            }
            (None, None) => Err(Error::QueryError(
                "find_range requires at least one bound".to_string(),
            )),
        }
    }
}

/// Container for multiple stream indices
pub struct StreamMultiIndex<K: Key> {
    /// Map of field names to corresponding indices
    indices: HashMap<String, StreamIndex<K>>,
    /// Phantom marker for the key type
    _marker: PhantomData<K>,
}

impl<K: Key> StreamMultiIndex<K> {
    /// Create a new empty multi-index
    pub fn new() -> Self {
        Self {
            indices: HashMap::new(),
            _marker: PhantomData,
        }
    }

    /// Add an index for a specific field
    pub fn add_index(&mut self, field: String, index: StreamIndex<K>) {
        self.indices.insert(field, index);
    }

    /// Get an index by field name
    pub fn get_index(&self, field: &str) -> Option<&StreamIndex<K>> {
        self.indices.get(field)
    }

    /// Execute a query using the provided reader
    pub fn query_with_reader<R: Read + Seek>(
        &self,
        reader: &mut R,
        query: &Query<K>,
    ) -> Result<Vec<u64>> {
        if query.conditions.is_empty() {
            return Err(Error::QueryError("query cannot be empty".to_string()));
        }

        // Process the first condition to initialize the result set
        let first_condition = &query.conditions[0];
        let mut result_set = self.process_condition(reader, first_condition)?;

        // Process remaining conditions with set intersection
        for condition in &query.conditions[1..] {
            let condition_results = self.process_condition(reader, condition)?;

            // Perform intersection (AND logic)
            result_set.retain(|offset| condition_results.contains(offset));

            // If result set is empty, we can short-circuit
            if result_set.is_empty() {
                break;
            }
        }

        Ok(result_set)
    }

    // Helper method to process a single condition with a reader
    fn process_condition<R: Read + Seek>(
        &self,
        reader: &mut R,
        condition: &QueryCondition<K>,
    ) -> Result<Vec<u64>> {
        let index = self.indices.get(&condition.field).ok_or_else(|| {
            Error::QueryError(format!("no index found for field '{}'", condition.field))
        })?;

        match condition.operator {
            Operator::Eq => index.find_exact_with_reader(reader, condition.key.clone()),
            Operator::Ne => {
                // Return all items except those matching the key
                let all_items = index.find_range_with_reader(
                    reader,
                    Some(K::min_value()),
                    Some(K::max_value()),
                )?;
                let matching_items = index.find_exact_with_reader(reader, condition.key.clone())?;

                Ok(all_items
                    .into_iter()
                    .filter(|item| !matching_items.contains(item))
                    .collect())
            }
            Operator::Gt => {
                let mut results =
                    index.find_range_with_reader(reader, Some(condition.key.clone()), None)?;
                // Remove exact matches
                let exact_matches = index.find_exact_with_reader(reader, condition.key.clone())?;
                results.retain(|item| !exact_matches.contains(item));
                Ok(results)
            }
            Operator::Lt => {
                let mut results =
                    index.find_range_with_reader(reader, None, Some(condition.key.clone()))?;
                // Remove exact matches
                let exact_matches = index.find_exact_with_reader(reader, condition.key.clone())?;
                results.retain(|item| !exact_matches.contains(item));
                Ok(results)
            }
            Operator::Ge => index.find_range_with_reader(reader, Some(condition.key.clone()), None),
            Operator::Le => index.find_range_with_reader(reader, None, Some(condition.key.clone())),
        }
    }
}

impl<K: Key> Default for StreamMultiIndex<K> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::Entry;
    use crate::stree::Stree;
    // use super::memory::MemoryIndex;
    use std::io::{Cursor, Seek, SeekFrom};

    fn create_test_data<K: Key>(entries: &[Entry<K>], branching_factor: u16) -> Vec<u8> {
        let index = Stree::build(entries, branching_factor).unwrap();
        let mut buffer = Vec::new();
        index.stream_write(&mut buffer).unwrap();
        buffer
    }

    #[test]
    fn test_stream_index_find_exact() -> Result<()> {
        // Create test data
        let entries = vec![
            Entry::new(10, 100),
            Entry::new(20, 200),
            Entry::new(30, 300),
            Entry::new(40, 400),
        ];

        let buffer = create_test_data(&entries, 4);
        let mut cursor = Cursor::new(buffer);

        // Create stream index
        let stream_index = StreamIndex::new(entries.len(), 4, 0, 0);

        // Test exact match
        let results = stream_index.find_exact_with_reader(&mut cursor, 20)?;
        assert_eq!(results, vec![200]);

        // Reset cursor position
        cursor.seek(SeekFrom::Start(0))?;

        // Test non-existent key
        let results = stream_index.find_exact_with_reader(&mut cursor, 25)?;
        assert!(results.is_empty());

        Ok(())
    }

    #[test]
    fn test_stream_index_find_range() -> Result<()> {
        // Create test data
        let entries = vec![
            Entry::new(10, 100),
            Entry::new(20, 200),
            Entry::new(30, 300),
            Entry::new(40, 400),
        ];

        let buffer = create_test_data(&entries, 4);
        let mut cursor = Cursor::new(buffer);

        // Create stream index
        let stream_index = StreamIndex::new(entries.len(), 4, 0, 0);

        // Test inclusive range
        let results = stream_index.find_range_with_reader(&mut cursor, Some(20), Some(30))?;
        assert_eq!(results, vec![200, 300]);

        // Reset cursor position
        cursor.seek(SeekFrom::Start(0))?;

        // Test range with only start bound
        let results = stream_index.find_range_with_reader(&mut cursor, Some(30), None)?;
        assert_eq!(results, vec![300, 400]);

        Ok(())
    }

    #[test]
    fn test_stream_multi_index_query() -> Result<()> {
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

        // Create data buffers
        let age_buffer = create_test_data(&age_entries, 4);
        let score_buffer = create_test_data(&score_entries, 4);

        // Create combined buffer with both indices
        let mut combined_buffer = Vec::new();
        let age_index_offset = 0;
        let score_index_offset = age_buffer.len() as u64;

        combined_buffer.extend_from_slice(&age_buffer);
        combined_buffer.extend_from_slice(&score_buffer);

        let mut cursor = Cursor::new(combined_buffer);

        // Create stream indices
        let age_index = StreamIndex::new(age_entries.len(), 4, age_index_offset, 0);
        let score_index = StreamIndex::new(score_entries.len(), 4, score_index_offset, 0);

        // Create multi-index
        let mut multi_index = StreamMultiIndex::new();
        multi_index.add_index("age".to_string(), age_index);
        multi_index.add_index("score".to_string(), score_index);

        // Query: age >= 25 AND score >= 85
        let mut query = Query::new();
        query.add_condition("age".to_string(), Operator::Ge, 25);
        query.add_condition("score".to_string(), Operator::Ge, 85);

        let results = multi_index.query_with_reader(&mut cursor, &query)?;
        assert_eq!(results, vec![2, 4]); // Matches id=2 and id=4

        // Reset cursor
        cursor.seek(SeekFrom::Start(0))?;

        // Query: age = 40 AND score = 95
        let mut query = Query::new();
        query.add_condition("age".to_string(), Operator::Eq, 40);
        query.add_condition("score".to_string(), Operator::Eq, 95);

        let results = multi_index.query_with_reader(&mut cursor, &query)?;
        assert_eq!(results, vec![4]); // Matches only id=4

        Ok(())
    }
}
