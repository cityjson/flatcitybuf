use chrono::{DateTime, Utc};
use ordered_float::OrderedFloat;

use crate::error::Result;
use crate::key::{FixedStringKey, Key};

/// Comparison operators for queries
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    /// Equal
    Eq,
    /// Not equal
    Ne,
    /// Greater than
    Gt,
    /// Less than
    Lt,
    /// Greater than or equal
    Ge,
    /// Less than or equal
    Le,
}

/// Enum to hold different key types supported by the system
#[derive(Debug, Clone)]
pub enum KeyType {
    /// Fixed-size string keys (with different sizes as type parameters)
    StringKey20(FixedStringKey<20>),
    StringKey50(FixedStringKey<50>),
    StringKey100(FixedStringKey<100>),
    /// Integer keys
    Int32(i32),
    Int64(i64),
    UInt32(u32),
    UInt64(u64),
    /// Floating point keys (wrapped in OrderedFloat for total ordering)
    Float32(OrderedFloat<f32>),
    Float64(OrderedFloat<f64>),
    /// Boolean keys
    Bool(bool),
    /// DateTime keys
    DateTime(DateTime<Utc>),
}

/// A query condition with an enum key type
#[derive(Debug, Clone)]
pub struct TypedQueryCondition {
    pub field: String,
    pub operator: Operator,
    pub key: KeyType,
}

/// A complete query with multiple conditions
#[derive(Debug, Clone)]
pub struct Query {
    /// List of conditions combined with AND logic
    pub conditions: Vec<TypedQueryCondition>,
}

impl Query {
    /// Create a new empty query
    pub fn new() -> Self {
        Self {
            conditions: Vec::new(),
        }
    }

    /// Add a condition to the query
    pub fn add_condition(&mut self, field: String, operator: Operator, key: KeyType) {
        self.conditions.push(TypedQueryCondition {
            field,
            operator,
            key,
        });
    }

    /// Create a query with a single condition
    pub fn with_condition(field: String, operator: Operator, key: KeyType) -> Self {
        let mut query = Self::new();
        query.add_condition(field, operator, key);
        query
    }
}

impl Default for Query {
    fn default() -> Self {
        Self::new()
    }
}

/// Core trait for index searching capabilities
pub trait SearchIndex<K: Key> {
    /// Find exact matches for a key
    fn find_exact(&self, key: K) -> Result<Vec<u64>>;

    /// Find matches within a range (inclusive start, inclusive end)
    fn find_range(&self, start: Option<K>, end: Option<K>) -> Result<Vec<u64>>;
}

/// Trait for multi-index query capabilities
pub trait MultiIndex {
    /// Execute a query and return matching offsets
    fn query(&self, query: &[TypedQueryCondition]) -> Result<Vec<u64>>;
}
