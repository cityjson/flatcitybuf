use std::collections::HashMap;
use std::fmt::Debug;
use std::io::{Read, Seek, SeekFrom};
use std::marker::PhantomData;
use std::ops::Range;

use chrono::{DateTime, Utc};
use ordered_float::OrderedFloat;

use crate::static_btree::error::{Error, Result};
use crate::static_btree::key::{FixedStringKey, Key, KeyType, Max, Min};
use crate::static_btree::query::types::{Operator, QueryCondition};
use crate::static_btree::stree::Stree;

/// Stream-based index for file access
#[derive(Debug, Clone)]
pub struct StreamIndex<K: Key> {
    /// Number of items in the index
    num_items: usize,
    /// Branching factor of the tree
    branching_factor: u16,
    /// Offset of the index in the file
    index_offset: u64,
    /// Size of the index
    length: u64,
    /// Phantom marker for the key type
    _marker: PhantomData<K>,
}

impl<K: Key> StreamIndex<K> {
    /// Create a new stream index with metadata
    pub fn new(num_items: usize, branching_factor: u16, index_offset: u64, length: u64) -> Self {
        Self {
            num_items,
            branching_factor,
            index_offset,
            length,
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

    /// Get the length of the index
    pub fn length(&self) -> u64 {
        self.length
    }

    /// Find exact matches using a reader
    pub fn find_exact_with_reader<R: Read + Seek + ?Sized>(
        &self,
        reader: &mut R,
        key: K,
    ) -> Result<Vec<u64>> {
        let results = Stree::stream_find_exact(reader, self.num_items, self.branching_factor, key)?;

        Ok(results.into_iter().map(|item| item.offset as u64).collect())
    }

    /// Find range matches using a reader
    pub fn find_range_with_reader<R: Read + Seek + ?Sized>(
        &self,
        reader: &mut R,
        start: Option<K>,
        end: Option<K>,
    ) -> Result<Vec<u64>> {
        // print current cursor position
        let start_position = reader.stream_position()?;
        let results = match (start, end) {
            (Some(start_key), Some(end_key)) => {
                let results = Stree::stream_find_range(
                    reader,
                    self.num_items,
                    self.branching_factor,
                    start_key,
                    end_key,
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
                )?;
                Ok(results.into_iter().map(|item| item.offset as u64).collect())
            }
            (None, None) => Err(Error::QueryError(
                "find_range requires at least one bound".to_string(),
            )),
        };

        reader.seek(SeekFrom::Start(start_position))?;
        results
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
        condition: &QueryCondition,
    ) -> Result<Vec<u64>>;
}

// Macro to implement TypedStreamSearchIndex for each supported key type
macro_rules! impl_typed_stream_search_index {
    ($key_type:ty, $enum_variant:path) => {
        impl TypedStreamSearchIndex for StreamIndex<$key_type> {
            fn execute_query_condition(
                &self,
                reader: &mut dyn ReadSeek,
                condition: &QueryCondition,
            ) -> Result<Vec<u64>> {
                let start_position = reader.stream_position()?;
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
                let items = match condition.operator {
                    Operator::Eq => self.find_exact_with_reader(reader, key)?,
                    Operator::Ne => {
                        let all_items = self.find_range_with_reader(
                            reader,
                            Some(<$key_type>::min_value()),
                            Some(<$key_type>::max_value()),
                        )?;
                        let matching_items = self.find_exact_with_reader(reader, key.clone())?;
                        all_items
                            .into_iter()
                            .filter(|item| !matching_items.contains(item))
                            .collect()
                    }
                    Operator::Gt => {
                        let mut results =
                            self.find_range_with_reader(reader, Some(key.clone()), None)?;
                        let exact_matches = self.find_exact_with_reader(reader, key.clone())?;
                        results.retain(|item| !exact_matches.contains(item));
                        results
                    }
                    Operator::Lt => {
                        let mut results =
                            self.find_range_with_reader(reader, None, Some(key.clone()))?;
                        let exact_matches = self.find_exact_with_reader(reader, key.clone())?;
                        results.retain(|item| !exact_matches.contains(item));
                        results
                    }
                    Operator::Ge => self.find_range_with_reader(reader, Some(key), None)?,
                    Operator::Le => self.find_range_with_reader(reader, None, Some(key))?,
                };
                reader.seek(SeekFrom::Start(start_position))?;
                Ok(items)
            }
        }
    };
}

// Implement TypedStreamSearchIndex for all supported key types
impl_typed_stream_search_index!(i8, KeyType::Int8);
impl_typed_stream_search_index!(u8, KeyType::UInt8);
impl_typed_stream_search_index!(i16, KeyType::Int16);
impl_typed_stream_search_index!(u16, KeyType::UInt16);
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
    index_offsets: HashMap<String, Range<usize>>,
}

impl StreamMultiIndex {
    /// Create a new empty multi-index
    pub fn new() -> Self {
        Self {
            indices: HashMap::new(),
            index_offsets: HashMap::new(),
        }
    }

    /// Generic method to add an index for any supported key type
    pub fn add_index<K: Key + 'static>(&mut self, field: String, index: StreamIndex<K>)
    where
        StreamIndex<K>: TypedStreamSearchIndex,
    {
        self.indices.insert(field, Box::new(index));
    }

    fn add_index_offset(&mut self, field: String, length: u64) {
        //length of the index about to be added
        // get the last index offset
        let largest_offset = self
            .index_offsets
            .values()
            .map(|v| v.end)
            .max()
            .unwrap_or(0);
        self.index_offsets
            .insert(field, largest_offset..largest_offset + length as usize);
    }

    /// Add a string index with key size 20
    pub fn add_string_index20(
        &mut self,
        field: String,
        index: StreamIndex<FixedStringKey<20>>,
        length: u64,
    ) {
        self.indices.insert(field.clone(), Box::new(index));
        self.add_index_offset(field, length);
    }

    /// Add a string index with key size 50
    pub fn add_string_index50(
        &mut self,
        field: String,
        index: StreamIndex<FixedStringKey<50>>,
        length: u64,
    ) {
        self.indices.insert(field.clone(), Box::new(index));
        self.add_index_offset(field, length);
    }

    /// Add a string index with key size 100
    pub fn add_string_index100(
        &mut self,
        field: String,
        index: StreamIndex<FixedStringKey<100>>,
        length: u64,
    ) {
        self.indices.insert(field.clone(), Box::new(index));
        self.add_index_offset(field, length);
    }

    /// Add an i8 index
    pub fn add_i8_index(&mut self, field: String, index: StreamIndex<i8>, length: u64) {
        self.indices.insert(field.clone(), Box::new(index));
        self.add_index_offset(field, length);
    }

    /// Add a u8 index
    pub fn add_u8_index(&mut self, field: String, index: StreamIndex<u8>, length: u64) {
        self.indices.insert(field.clone(), Box::new(index));
        self.add_index_offset(field, length);
    }

    /// Add an i16 index
    pub fn add_i16_index(&mut self, field: String, index: StreamIndex<i16>, length: u64) {
        self.indices.insert(field.clone(), Box::new(index));
        self.add_index_offset(field, length);
    }

    /// Add a u16 index
    pub fn add_u16_index(&mut self, field: String, index: StreamIndex<u16>, length: u64) {
        self.indices.insert(field.clone(), Box::new(index));
        self.add_index_offset(field, length);
    }

    /// Add an i32 index
    pub fn add_i32_index(&mut self, field: String, index: StreamIndex<i32>, length: u64) {
        self.indices.insert(field.clone(), Box::new(index));
        self.add_index_offset(field, length);
    }

    /// Add an i64 index
    pub fn add_i64_index(&mut self, field: String, index: StreamIndex<i64>, length: u64) {
        self.indices.insert(field.clone(), Box::new(index));
        self.add_index_offset(field, length);
    }

    /// Add a u32 index
    pub fn add_u32_index(&mut self, field: String, index: StreamIndex<u32>, length: u64) {
        self.indices.insert(field.clone(), Box::new(index));
        self.add_index_offset(field, length);
    }

    /// Add a u64 index
    pub fn add_u64_index(&mut self, field: String, index: StreamIndex<u64>, length: u64) {
        self.indices.insert(field.clone(), Box::new(index));
        self.add_index_offset(field, length);
    }

    /// Add a float32 index
    pub fn add_f32_index(
        &mut self,
        field: String,
        index: StreamIndex<OrderedFloat<f32>>,
        length: u64,
    ) {
        self.indices.insert(field.clone(), Box::new(index));
        self.add_index_offset(field, length);
    }

    /// Add a float64 index
    pub fn add_f64_index(
        &mut self,
        field: String,
        index: StreamIndex<OrderedFloat<f64>>,
        length: u64,
    ) {
        self.indices.insert(field.clone(), Box::new(index));
        self.add_index_offset(field, length);
    }

    /// Add a boolean index
    pub fn add_bool_index(&mut self, field: String, index: StreamIndex<bool>, length: u64) {
        self.indices.insert(field.clone(), Box::new(index));
        self.add_index_offset(field, length);
    }

    /// Add a datetime index
    pub fn add_datetime_index(
        &mut self,
        field: String,
        index: StreamIndex<DateTime<Utc>>,
        length: u64,
    ) {
        self.indices.insert(field.clone(), Box::new(index));
        self.add_index_offset(field, length);
    }

    /// Execute a heterogeneous query with different key types using a reader
    pub fn query(
        &self,
        reader: &mut dyn ReadSeek,
        conditions: &[QueryCondition],
    ) -> Result<Vec<u64>> {
        if conditions.is_empty() {
            return Err(Error::QueryError("query cannot be empty".to_string()));
        }
        let first = &conditions[0];
        let indexer = self.indices.get(&first.field).ok_or_else(|| {
            Error::QueryError(format!("no index found for field '{}'", first.field))
        })?;
        let index_range = self.index_offsets.get(&first.field).ok_or_else(|| {
            Error::QueryError(format!("no index range found for field '{}'", first.field))
        })?;

        // currently reader is continuous buffer of multiple indices. We need to create different readers for each index. `index_offsets` field of the struct accomodates Range of each indices. e.g. if index_offsets is [(field1, 0..100), (field2, 100..200)], it means that field1 is at offset 0-99 and field2 is at offset 100-199 in the reader. Since `execute_query_condition` is called with a reader, we need to create a new reader for each index.

        let start_position = reader.stream_position()?;
        // set cursor to the start of the index
        reader.seek(SeekFrom::Start(start_position + index_range.start as u64))?;

        let mut result_set = indexer.execute_query_condition(reader, first)?;
        if result_set.is_empty() {
            return Ok(vec![]);
        }
        // set cursor to the start of the index
        reader.seek(SeekFrom::Start(start_position))?;

        for cond in &conditions[1..] {
            let start_position = reader.stream_position()?;
            let indexer = self.indices.get(&cond.field).ok_or_else(|| {
                Error::QueryError(format!("no index found for field '{}'", cond.field))
            })?;
            let index_range = self.index_offsets.get(&cond.field).ok_or_else(|| {
                Error::QueryError(format!("no index range found for field '{}'", cond.field))
            })?;
            let index_start = start_position + index_range.start as u64;
            // set cursor to the start of the index
            reader.seek(SeekFrom::Start(index_start))?;
            println!("index_start: {index_start}");
            println!("start_position: {start_position}");
            println!("query condition: {cond:?}");
            let condition_results = indexer.execute_query_condition(reader, cond)?;
            result_set.retain(|offset| condition_results.contains(offset));
            if result_set.is_empty() {
                return Ok(vec![]); // no results found for this condition, return early so we don't waste time intersecting empty sets
            }
            // set cursor to the start of the index
            reader.seek(SeekFrom::Start(start_position))?;
        }
        // set cursor to the start of the index
        reader.seek(SeekFrom::Start(start_position))?;
        Ok(result_set)
    }
}

impl Default for StreamMultiIndex {
    fn default() -> Self {
        Self::new()
    }
}
