use chrono::{DateTime, Utc};
use ordered_float::OrderedFloat;
use std::collections::HashMap;
use std::io::{Read, Write};

use crate::entry::Entry;
use crate::error::{Error, Result};
use crate::key::{FixedStringKey, Key, KeyType, Max, Min};
use crate::query::types::{Operator, SearchIndex};
use crate::stree::Stree;

use super::types::QueryCondition;
use super::MultiIndex;

/// In-memory index implementation that wraps the Stree structure
// NOTE: This can be type alias for Stree later
#[derive(Debug, Clone)]
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
        let stree = Stree::<K>::build(entries, branching_factor)?;

        Ok(Self { stree })
    }

    pub fn from_buf(mut data: impl Read, num_items: usize, branching_factor: u16) -> Result<Self> {
        let stree = Stree::from_buf(&mut data, num_items, branching_factor)?;

        Ok(Self { stree })
    }

    pub fn num_items(&self) -> usize {
        self.stree.num_leaf_items()
    }

    pub fn branching_factor(&self) -> u16 {
        self.stree.branching_factor()
    }

    pub fn size(&self) -> usize {
        Stree::<K>::tree_size(self.num_items())
    }

    pub fn serialize(&self, out: &mut impl Write) -> Result<usize> {
        self.stree.stream_write(out)
    }

    pub fn payload_size(&self) -> usize {
        self.stree.payload_size()
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
            (None, None) => Err(Error::QueryError(
                "find_range requires at least one bound".to_string(),
            )),
        }
    }
}

/// Trait for different index types we might store
pub trait TypedSearchIndex: Send + Sync {
    /// Execute the query condition
    fn execute_query_condition(&self, condition: &QueryCondition) -> Result<Vec<u64>>;
}

// Macro to implement TypedSearchIndex for each key type following the same pattern
macro_rules! impl_typed_search_index {
    ($key_type:ty, $enum_variant:path) => {
        impl TypedSearchIndex for MemoryIndex<$key_type> {
            fn execute_query_condition(&self, condition: &QueryCondition) -> Result<Vec<u64>> {
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
                    Operator::Eq => self.find_exact(key),
                    Operator::Ne => {
                        let min = <$key_type>::min_value();
                        let max = <$key_type>::max_value();
                        let all_items = self.find_range(Some(min), Some(max))?;
                        let matching_items = self.find_exact(key)?;
                        Ok(all_items
                            .into_iter()
                            .filter(|item| !matching_items.contains(item))
                            .collect())
                    }
                    Operator::Gt => {
                        let mut results = self.find_range(Some(key.clone()), None)?;
                        let exact_matches = self.find_exact(key)?;
                        results.retain(|item| !exact_matches.contains(item));
                        Ok(results)
                    }
                    Operator::Lt => {
                        let mut results = self.find_range(None, Some(key.clone()))?;
                        let exact_matches = self.find_exact(key)?;
                        results.retain(|item| !exact_matches.contains(item));
                        Ok(results)
                    }
                    Operator::Ge => self.find_range(Some(key), None),
                    Operator::Le => self.find_range(None, Some(key)),
                }
            }
        }
    };
}

// Implement TypedSearchIndex for all supported key types
impl_typed_search_index!(i32, KeyType::Int32);
impl_typed_search_index!(i64, KeyType::Int64);
impl_typed_search_index!(u32, KeyType::UInt32);
impl_typed_search_index!(u64, KeyType::UInt64);
impl_typed_search_index!(OrderedFloat<f32>, KeyType::Float32);
impl_typed_search_index!(OrderedFloat<f64>, KeyType::Float64);
impl_typed_search_index!(bool, KeyType::Bool);
impl_typed_search_index!(DateTime<Utc>, KeyType::DateTime);
impl_typed_search_index!(FixedStringKey<20>, KeyType::StringKey20);
impl_typed_search_index!(FixedStringKey<50>, KeyType::StringKey50);
impl_typed_search_index!(FixedStringKey<100>, KeyType::StringKey100);

// pub trait SerdeIndex {
//     fn serialize(&self, out: &mut impl Write) -> Result<usize>;
//     fn deserialize(data: impl Read, num_items: usize, branching_factor: u16) -> Result<Self>
//     where
//         Self: Sized;
// }

// pub trait SerdeIndex: Sized {
//     fn serialize(&self, out: &mut impl Write) -> Result<usize>;
//     fn deserialize(data: impl Read, num_items: usize, branching_factor: u16) -> Result<Self>;
// }

// macro_rules! impl_serde_index {
//     ($index_type:ty) => {
//         impl SerdeIndex for MemoryIndex<$index_type> {
//             fn serialize(&self, out: &mut impl Write) -> Result<usize> {
//                 self.serialize(out)
//             }
//             fn deserialize(data: impl Read, num_items: usize, branching_factor: u16) -> Result<Self> {
//                 MemoryIndex::<$index_type>::from_buf(data, num_items, branching_factor)
//             }
//         }
//     };
// }

// impl_serde_index!(i32);
// impl_serde_index!(i64);
// impl_serde_index!(u32);
// impl_serde_index!(u64);
// impl_serde_index!(OrderedFloat<f32>);
// impl_serde_index!(OrderedFloat<f64>);
// impl_serde_index!(bool);
// impl_serde_index!(DateTime<Utc>);
// impl_serde_index!(FixedStringKey<20>);
// impl_serde_index!(FixedStringKey<50>);
// impl_serde_index!(FixedStringKey<100>);

/// Container for multiple in-memory indices with different key types
pub struct MemoryMultiIndex {
    /// Map of field names to typed indices
    indices: HashMap<String, Box<dyn TypedSearchIndex>>,
}

impl MemoryMultiIndex {
    /// Create a new empty multi-index
    pub fn new() -> Self {
        Self {
            indices: HashMap::new(),
        }
    }

    /// Generic method to add an index for any supported key type
    pub fn add_index<K: Key + 'static>(&mut self, field: String, index: MemoryIndex<K>)
    where
        MemoryIndex<K>: TypedSearchIndex,
    {
        self.indices.insert(field, Box::new(index));
    }

    pub fn indices(&self) -> &HashMap<String, Box<dyn TypedSearchIndex>> {
        &self.indices
    }

    /// Add a string index with key size 20
    pub fn add_string_index20(&mut self, field: String, index: MemoryIndex<FixedStringKey<20>>) {
        self.indices.insert(field, Box::new(index));
    }

    /// Add a string index with key size 50
    pub fn add_string_index50(&mut self, field: String, index: MemoryIndex<FixedStringKey<50>>) {
        self.indices.insert(field, Box::new(index));
    }

    /// Add a string index with key size 100
    pub fn add_string_index100(&mut self, field: String, index: MemoryIndex<FixedStringKey<100>>) {
        self.indices.insert(field, Box::new(index));
    }

    /// Add an i32 index
    pub fn add_i32_index(&mut self, field: String, index: MemoryIndex<i32>) {
        self.indices.insert(field, Box::new(index));
    }

    /// Add an i64 index
    pub fn add_i64_index(&mut self, field: String, index: MemoryIndex<i64>) {
        self.indices.insert(field, Box::new(index));
    }

    /// Add a u32 index
    pub fn add_u32_index(&mut self, field: String, index: MemoryIndex<u32>) {
        self.indices.insert(field, Box::new(index));
    }

    /// Add a u64 index
    pub fn add_u64_index(&mut self, field: String, index: MemoryIndex<u64>) {
        self.indices.insert(field, Box::new(index));
    }

    /// Add a float32 index
    pub fn add_f32_index(&mut self, field: String, index: MemoryIndex<OrderedFloat<f32>>) {
        self.indices.insert(field, Box::new(index));
    }

    /// Add a float64 index
    pub fn add_f64_index(&mut self, field: String, index: MemoryIndex<OrderedFloat<f64>>) {
        self.indices.insert(field, Box::new(index));
    }

    /// Add a boolean index
    pub fn add_bool_index(&mut self, field: String, index: MemoryIndex<bool>) {
        self.indices.insert(field, Box::new(index));
    }

    /// Add a datetime index
    pub fn add_datetime_index(&mut self, field: String, index: MemoryIndex<DateTime<Utc>>) {
        self.indices.insert(field, Box::new(index));
    }
}

impl MultiIndex for MemoryMultiIndex {
    /// Execute a heterogeneous query with different key types
    fn query(&self, conditions: &[QueryCondition]) -> Result<Vec<u64>> {
        if conditions.is_empty() {
            return Err(Error::QueryError("query cannot be empty".to_string()));
        }

        // Process the first condition to initialize the result set
        let first_condition = &conditions[0];
        let index = self.indices.get(&first_condition.field).ok_or_else(|| {
            Error::QueryError(format!(
                "no index found for field '{}'",
                first_condition.field
            ))
        })?;
        let mut result_set = index.execute_query_condition(first_condition)?;
        if result_set.is_empty() {
            return Ok(vec![]);
        }

        // Process remaining conditions with set intersection
        for condition in &conditions[1..] {
            let index = self.indices.get(&condition.field).ok_or_else(|| {
                Error::QueryError(format!("no index found for field '{}'", condition.field))
            })?;
            let condition_results = index.execute_query_condition(condition)?;

            // Perform intersection (AND logic)
            result_set.retain(|offset| condition_results.contains(offset));

            // If result set is empty, we can short-circuit. For now it's AND logic, so if any condition is empty, the result set is empty
            if result_set.is_empty() {
                return Ok(vec![]);
            }
        }

        Ok(result_set)
    }
}

impl Default for MemoryMultiIndex {
    fn default() -> Self {
        Self::new()
    }
}
