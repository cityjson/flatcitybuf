use crate::error::Result;
use crate::key::Key;

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

/// A single query condition
#[derive(Debug, Clone)]
pub struct QueryCondition<K: Key> {
    /// Field name
    pub field: String,
    /// Comparison operator
    pub operator: Operator,
    /// Key value
    pub key: K,
}

/// A complete query with multiple conditions
#[derive(Debug, Clone)]
pub struct Query<K: Key> {
    /// List of conditions combined with AND logic
    pub conditions: Vec<QueryCondition<K>>,
}

impl<K: Key> Query<K> {
    /// Create a new empty query
    pub fn new() -> Self {
        Self {
            conditions: Vec::new(),
        }
    }

    /// Add a condition to the query
    pub fn add_condition(&mut self, field: String, operator: Operator, key: K) {
        self.conditions.push(QueryCondition {
            field,
            operator,
            key,
        });
    }

    /// Create a query with a single condition
    pub fn with_condition(field: String, operator: Operator, key: K) -> Self {
        let mut query = Self::new();
        query.add_condition(field, operator, key);
        query
    }
}

impl<K: Key> Default for Query<K> {
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
pub trait MultiIndex<K: Key> {
    /// Execute a query and return matching offsets
    fn query(&self, query: &Query<K>) -> Result<Vec<u64>>;
}
