use std::collections::HashMap;
use std::io::{Read, Seek};
use std::marker::PhantomData;

use chrono::{DateTime, Utc};
use ordered_float::OrderedFloat;

use crate::error::{Error, Result};
use crate::key::{FixedStringKey, Key, Max, Min};
use crate::query::memory::{KeyType, TypedQueryCondition};
use crate::query::types::Operator;
use crate::stree::Stree;

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
    pub fn find_exact_with_reader<R: Read + Seek + ?Sized>(
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
    pub fn find_range_with_reader<R: Read + Seek + ?Sized>(
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

/// Trait alias for objects that implement Read and Seek, to allow trait objects
pub trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}

/// Trait for typed stream search index with heterogeneous key types
pub trait TypedStreamSearchIndex: Send + Sync {
    /// Execute the query condition using the provided reader
    fn execute_query_condition(
        &self,
        reader: &mut dyn ReadSeek,
        condition: &TypedQueryCondition,
    ) -> Result<Vec<u64>>;
}

// Macro to implement TypedStreamSearchIndex for each supported key type
macro_rules! impl_typed_stream_search_index {
    ($key_type:ty, $enum_variant:path) => {
        impl TypedStreamSearchIndex for StreamIndex<$key_type> {
            fn execute_query_condition(
                &self,
                reader: &mut dyn ReadSeek,
                condition: &TypedQueryCondition,
            ) -> Result<Vec<u64>> {
                // Extract the key value from the enum variant
                let key = match &condition.key {
                    $enum_variant(val) => val.clone(),
                    _ => {
                        return Err(Error::QueryError(format!(
                            "key type mismatch: expected {}, got {:?}",
                            stringify!($key_type),
                            condition.key
                        )))
                    }
                };
                // Execute query based on operator
                match condition.operator {
                    Operator::Eq => self.find_exact_with_reader(reader, key),
                    Operator::Ne => {
                        let all_items = self.find_range_with_reader(
                            reader,
                            Some(<$key_type>::min_value()),
                            Some(<$key_type>::max_value()),
                        )?;
                        let matching_items = self.find_exact_with_reader(reader, key.clone())?;
                        Ok(all_items
                            .into_iter()
                            .filter(|item| !matching_items.contains(item))
                            .collect())
                    }
                    Operator::Gt => {
                        let mut results =
                            self.find_range_with_reader(reader, Some(key.clone()), None)?;
                        let exact_matches = self.find_exact_with_reader(reader, key.clone())?;
                        results.retain(|item| !exact_matches.contains(item));
                        Ok(results)
                    }
                    Operator::Lt => {
                        let mut results =
                            self.find_range_with_reader(reader, None, Some(key.clone()))?;
                        let exact_matches = self.find_exact_with_reader(reader, key.clone())?;
                        results.retain(|item| !exact_matches.contains(item));
                        Ok(results)
                    }
                    Operator::Ge => self.find_range_with_reader(reader, Some(key), None),
                    Operator::Le => self.find_range_with_reader(reader, None, Some(key)),
                }
            }
        }
    };
}

// Implement TypedStreamSearchIndex for all supported key types
impl_typed_stream_search_index!(i32, KeyType::Int32);
impl_typed_stream_search_index!(i64, KeyType::Int64);
impl_typed_stream_search_index!(u32, KeyType::UInt32);
impl_typed_stream_search_index!(u64, KeyType::UInt64);
impl_typed_stream_search_index!(OrderedFloat<f32>, KeyType::Float32);
impl_typed_stream_search_index!(OrderedFloat<f64>, KeyType::Float64);
impl_typed_stream_search_index!(bool, KeyType::Bool);
impl_typed_stream_search_index!(DateTime<Utc>, KeyType::DateTime);
impl_typed_stream_search_index!(FixedStringKey<20>, KeyType::StringKey20);
impl_typed_stream_search_index!(FixedStringKey<50>, KeyType::StringKey50);
impl_typed_stream_search_index!(FixedStringKey<100>, KeyType::StringKey100);

/// Container for multiple stream indices with different key types
pub struct StreamMultiIndex {
    indices: HashMap<String, Box<dyn TypedStreamSearchIndex>>,
}

impl StreamMultiIndex {
    /// Create a new empty multi-index
    pub fn new() -> Self {
        Self {
            indices: HashMap::new(),
        }
    }

    /// Generic method to add an index for any supported key type
    pub fn add_index<K: Key + 'static>(&mut self, field: String, index: StreamIndex<K>)
    where
        StreamIndex<K>: TypedStreamSearchIndex,
    {
        self.indices.insert(field, Box::new(index));
    }

    /// Add a string index with key size 20
    pub fn add_string_index20(&mut self, field: String, index: StreamIndex<FixedStringKey<20>>) {
        self.indices.insert(field, Box::new(index));
    }

    /// Add a string index with key size 50
    pub fn add_string_index50(&mut self, field: String, index: StreamIndex<FixedStringKey<50>>) {
        self.indices.insert(field, Box::new(index));
    }

    /// Add a string index with key size 100
    pub fn add_string_index100(&mut self, field: String, index: StreamIndex<FixedStringKey<100>>) {
        self.indices.insert(field, Box::new(index));
    }

    /// Add an i32 index
    pub fn add_i32_index(&mut self, field: String, index: StreamIndex<i32>) {
        self.indices.insert(field, Box::new(index));
    }

    /// Add an i64 index
    pub fn add_i64_index(&mut self, field: String, index: StreamIndex<i64>) {
        self.indices.insert(field, Box::new(index));
    }

    /// Add a u32 index
    pub fn add_u32_index(&mut self, field: String, index: StreamIndex<u32>) {
        self.indices.insert(field, Box::new(index));
    }

    /// Add a u64 index
    pub fn add_u64_index(&mut self, field: String, index: StreamIndex<u64>) {
        self.indices.insert(field, Box::new(index));
    }

    /// Add a float32 index
    pub fn add_f32_index(&mut self, field: String, index: StreamIndex<OrderedFloat<f32>>) {
        self.indices.insert(field, Box::new(index));
    }

    /// Add a float64 index
    pub fn add_f64_index(&mut self, field: String, index: StreamIndex<OrderedFloat<f64>>) {
        self.indices.insert(field, Box::new(index));
    }

    /// Add a boolean index
    pub fn add_bool_index(&mut self, field: String, index: StreamIndex<bool>) {
        self.indices.insert(field, Box::new(index));
    }

    /// Add a datetime index
    pub fn add_datetime_index(&mut self, field: String, index: StreamIndex<DateTime<Utc>>) {
        self.indices.insert(field, Box::new(index));
    }

    /// Execute a heterogeneous query with different key types using a reader
    pub fn query_with_reader(
        &self,
        reader: &mut dyn ReadSeek,
        conditions: &[TypedQueryCondition],
    ) -> Result<Vec<u64>> {
        if conditions.is_empty() {
            return Err(Error::QueryError("query cannot be empty".to_string()));
        }
        let first = &conditions[0];
        let indexer = self.indices.get(&first.field).ok_or_else(|| {
            Error::QueryError(format!("no index found for field '{}'", first.field))
        })?;
        let mut result_set = indexer.execute_query_condition(reader, first)?;
        for cond in &conditions[1..] {
            let condition_results = indexer.execute_query_condition(reader, cond)?;
            result_set.retain(|offset| condition_results.contains(offset));
            if result_set.is_empty() {
                break;
            }
        }
        Ok(result_set)
    }
}

impl Default for StreamMultiIndex {
    fn default() -> Self {
        Self::new()
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::entry::Entry;
//     use crate::query::memory::{KeyType, TypedQueryCondition};
//     use crate::stree::Stree;
//     use std::io::{Cursor, Seek, SeekFrom};

//     fn create_test_data<K: Key>(entries: &[Entry<K>], branching_factor: u16) -> Vec<u8> {
//         let index = Stree::build(entries, branching_factor).unwrap();
//         let mut buffer = Vec::new();
//         index.stream_write(&mut buffer).unwrap();
//         buffer
//     }

//     #[test]
//     fn test_stream_index_find_exact() -> Result<()> {
//         // Create test data
//         let entries = vec![
//             Entry::new(10, 100),
//             Entry::new(20, 200),
//             Entry::new(30, 300),
//             Entry::new(40, 400),
//         ];

//         let buffer = create_test_data(&entries, 4);
//         let mut cursor = Cursor::new(buffer);

//         // Create stream index
//         let stream_index = StreamIndex::new(entries.len(), 4, 0, 0);

//         // Test exact match
//         let results = stream_index.find_exact_with_reader(&mut cursor, 20)?;
//         assert_eq!(results, vec![200]);

//         // Reset cursor position
//         cursor.seek(SeekFrom::Start(0))?;

//         // Test non-existent key
//         let results = stream_index.find_exact_with_reader(&mut cursor, 25)?;
//         assert!(results.is_empty());

//         Ok(())
//     }

//     #[test]
//     fn test_stream_index_find_range() -> Result<()> {
//         // Create test data
//         let entries = vec![
//             Entry::new(10, 100),
//             Entry::new(20, 200),
//             Entry::new(30, 300),
//             Entry::new(40, 400),
//         ];

//         let buffer = create_test_data(&entries, 4);
//         let mut cursor = Cursor::new(buffer);

//         // Create stream index
//         let stream_index = StreamIndex::new(entries.len(), 4, 0, 0);

//         // Test inclusive range
//         let results = stream_index.find_range_with_reader(&mut cursor, Some(20), Some(30))?;
//         assert_eq!(results, vec![200, 300]);

//         // Reset cursor position
//         cursor.seek(SeekFrom::Start(0))?;

//         // Test range with only start bound
//         let results = stream_index.find_range_with_reader(&mut cursor, Some(30), None)?;
//         assert_eq!(results, vec![300, 400]);

//         Ok(())
//     }

//     #[test]
//     fn test_stream_multi_index_query() -> Result<()> {
//         // Create entries for "age" field
//         let age_entries = vec![
//             Entry::new(20, 1), // id=1, age=20
//             Entry::new(30, 2), // id=2, age=30
//             Entry::new(25, 3), // id=3, age=25
//             Entry::new(40, 4), // id=4, age=40
//         ];

//         // Create entries for "score" field
//         let score_entries = vec![
//             Entry::new(85, 1), // id=1, score=85
//             Entry::new(90, 2), // id=2, score=90
//             Entry::new(75, 3), // id=3, score=75
//             Entry::new(95, 4), // id=4, score=95
//         ];

//         // Create combined buffer with both indices
//         let age_buffer = create_test_data(&age_entries, 4);
//         let score_buffer = create_test_data(&score_entries, 4);
//         let mut combined_buffer = Vec::new();
//         let age_index_offset = 0;
//         let score_index_offset = age_buffer.len() as u64;
//         combined_buffer.extend_from_slice(&age_buffer);
//         combined_buffer.extend_from_slice(&score_buffer);

//         let mut cursor = Cursor::new(combined_buffer);

//         // Create stream indices
//         let age_index = StreamIndex::new(age_entries.len(), 4, age_index_offset, 0);
//         let score_index = StreamIndex::new(score_entries.len(), 4, score_index_offset, 0);

//         // Create multi-index
//         let mut multi_index = StreamMultiIndex::new();
//         multi_index.add_i32_index("age".to_string(), age_index);
//         multi_index.add_i32_index("score".to_string(), score_index);

//         // Query: age >= 25 AND score >= 85
//         let conditions = vec![
//             TypedQueryCondition {
//                 field: "age".to_string(),
//                 operator: Operator::Ge,
//                 key: KeyType::Int32(25),
//             },
//             TypedQueryCondition {
//                 field: "score".to_string(),
//                 operator: Operator::Ge,
//                 key: KeyType::Int32(85),
//             },
//         ];
//         let results = multi_index.query_with_reader(&mut cursor, &conditions)?;
//         assert_eq!(results, vec![2, 4]);

//         // Reset cursor position
//         cursor.seek(SeekFrom::Start(0))?;

//         // Query: age = 40 AND score = 95
//         let conditions = vec![
//             TypedQueryCondition {
//                 field: "age".to_string(),
//                 operator: Operator::Eq,
//                 key: KeyType::Int32(40),
//             },
//             TypedQueryCondition {
//                 field: "score".to_string(),
//                 operator: Operator::Eq,
//                 key: KeyType::Int32(95),
//             },
//         ];
//         let results = multi_index.query_with_reader(&mut cursor, &conditions)?;
//         assert_eq!(results, vec![4]);

//         Ok(())
//     }
// }
