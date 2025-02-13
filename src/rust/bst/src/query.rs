use anyhow::Result;
use std::collections::{HashMap, HashSet};

use crate::sorted_index::{AnyIndex, ValueOffset};

#[cfg(feature = "http")]
use packed_rtree::http::{HttpRange, HttpSearchResultItem};

/// Operators for comparisons in queries.
#[derive(Debug, Clone, Copy)]
pub enum Operator {
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
}

/// A query condition now refers to a field by name and carries the serialized key.
#[derive(Debug, Clone)]
pub struct QueryCondition {
    /// The field identifier (e.g., "id", "name", etc.)
    pub field: String,
    /// The comparison operator.
    pub operator: Operator,
    /// The key value as a byte vector (obtained via ByteSerializable::to_bytes).
    pub key: Vec<u8>,
}

/// A query is a set of conditions (implicitly AND‑combined).
#[derive(Debug, Clone)]
pub struct Query {
    pub conditions: Vec<QueryCondition>,
}

pub struct MultiIndex {
    /// A mapping from field names to their corresponding index.
    pub indices: HashMap<String, Box<dyn AnyIndex>>,
}

impl Default for MultiIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl MultiIndex {
    /// Create an empty MultiIndex.
    pub fn new() -> Self {
        MultiIndex {
            indices: HashMap::new(),
        }
    }

    /// Register an index under the given field name.
    pub fn add_index(&mut self, field_name: String, index: Box<dyn AnyIndex>) {
        self.indices.insert(field_name, index);
    }

    /// Execute a query over the registered indices.
    /// For each condition, candidate offsets are retrieved from the corresponding index.
    /// The final result is the intersection of candidates from all conditions.
    pub fn query(&self, query: Query) -> Vec<ValueOffset> {
        let mut candidate_sets: Vec<HashSet<ValueOffset>> = Vec::new();

        for condition in query.conditions {
            if let Some(index) = self.indices.get(&condition.field) {
                let offsets: Vec<ValueOffset> = match condition.operator {
                    Operator::Eq => {
                        // Exactly equal.
                        index.query_exact_bytes(&condition.key)
                    }
                    Operator::Gt => {
                        // Keys strictly greater than the boundary:
                        // Use query_range_bytes(Some(key), None) and remove those equal to key.
                        let offsets = index.query_range_bytes(Some(&condition.key), None);
                        let eq = index.query_exact_bytes(&condition.key);
                        offsets.into_iter().filter(|o| !eq.contains(o)).collect()
                    }
                    Operator::Ge => {
                        // Keys greater than or equal.
                        index.query_range_bytes(Some(&condition.key), None)
                    }
                    Operator::Lt => {
                        // Keys strictly less than the boundary.
                        index.query_range_bytes(None, Some(&condition.key))
                    }
                    Operator::Le => {
                        // Keys less than or equal to the boundary:
                        // Union the keys that are strictly less and those equal to the boundary.
                        let mut offsets = index.query_range_bytes(None, Some(&condition.key));
                        let eq = index.query_exact_bytes(&condition.key);
                        offsets.extend(eq);
                        // Remove duplicates by collecting into a set.
                        let set: HashSet<ValueOffset> = offsets.into_iter().collect();
                        set.into_iter().collect()
                    }
                    Operator::Ne => {
                        // All offsets minus those equal to the boundary.
                        let all: HashSet<ValueOffset> =
                            index.query_range_bytes(None, None).into_iter().collect();
                        let eq: HashSet<ValueOffset> = index
                            .query_exact_bytes(&condition.key)
                            .into_iter()
                            .collect();
                        all.difference(&eq).cloned().collect::<Vec<_>>()
                    }
                };
                candidate_sets.push(offsets.into_iter().collect());
            }
        }

        if candidate_sets.is_empty() {
            return vec![];
        }

        // Intersect candidate sets.
        let mut intersection: HashSet<ValueOffset> = candidate_sets.first().unwrap().clone();
        for set in candidate_sets.iter().skip(1) {
            intersection = intersection.intersection(set).cloned().collect();
        }

        let mut result: Vec<ValueOffset> = intersection.into_iter().collect();
        result.sort();
        result
    }
}

// TODO: improve this method to process on stream. Also, do something to avoid fetching many discrete ranges.
#[cfg(feature = "http")]
pub async fn stream_query(
    m_indices: &MultiIndex,
    query: Query,
    feature_begin: usize,
) -> Result<Vec<HttpSearchResultItem>> {
    // Compute candidate offset set for each query condition.

    let mut candidate_sets: Vec<HashSet<ValueOffset>> = Vec::new();
    for condition in query.conditions.iter() {
        if let Some(idx) = m_indices.indices.get(&condition.field) {
            let offsets: Vec<ValueOffset> = match condition.operator {
                Operator::Eq => idx.query_exact_bytes(&condition.key),
                Operator::Gt => {
                    let offsets = idx.query_range_bytes(Some(&condition.key), None);
                    let eq = idx.query_exact_bytes(&condition.key);
                    offsets.into_iter().filter(|o| !eq.contains(o)).collect()
                }
                Operator::Ge => idx.query_range_bytes(Some(&condition.key), None),
                Operator::Lt => idx.query_range_bytes(None, Some(&condition.key)),
                Operator::Le => {
                    let mut offsets = idx.query_range_bytes(None, Some(&condition.key));
                    let eq = idx.query_exact_bytes(&condition.key);
                    offsets.extend(eq);
                    // Remove duplicates.
                    offsets
                        .into_iter()
                        .collect::<HashSet<_>>()
                        .into_iter()
                        .collect()
                }
                Operator::Ne => {
                    let all: HashSet<ValueOffset> =
                        idx.query_range_bytes(None, None).into_iter().collect();
                    let eq: HashSet<ValueOffset> =
                        idx.query_exact_bytes(&condition.key).into_iter().collect();
                    all.difference(&eq).cloned().collect::<Vec<_>>()
                }
            };
            candidate_sets.push(offsets.into_iter().collect());
        }
    }

    if candidate_sets.is_empty() {
        return Ok(vec![]);
    }

    // Intersect candidate sets to get matching offsets.
    let mut intersection: HashSet<ValueOffset> = candidate_sets.first().unwrap().clone();
    for set in candidate_sets.iter().skip(1) {
        intersection = intersection.intersection(set).cloned().collect();
    }
    let mut offsets: Vec<ValueOffset> = intersection.into_iter().collect();
    offsets.sort(); // ascending order

    let http_ranges: Vec<HttpSearchResultItem> = offsets
        .into_iter()
        .map(|offset| HttpSearchResultItem {
            range: HttpRange::RangeFrom(offset as usize + feature_begin..),
        })
        .collect();

    Ok(http_ranges)
}
