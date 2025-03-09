use crate::sorted_index::{BufferedIndex, SearchableIndex, StreamableIndex, ValueOffset};
use crate::IndexMeta;
use std::collections::HashMap;
use std::collections::HashSet;
use std::io::{Read, Seek};
use std::ops::Range;

#[cfg(feature = "http")]
use http_range_client::{AsyncBufferedHttpRangeClient, AsyncHttpRangeClient};

/// Comparison operators for queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
}

/// A condition in a query, consisting of a field name, an operator, and a key value.
///
/// The key value is stored as a byte vector, obtained via ByteSerializable::to_bytes.
#[derive(Debug, Clone)]
pub struct QueryCondition {
    /// The field identifier (e.g., "id", "name", etc.)
    pub field: String,
    /// The comparison operator.
    pub operator: Operator,
    /// The key value as a byte vector (obtained via ByteSerializable::to_bytes).
    pub key: Vec<u8>,
}

/// A query consisting of one or more conditions.
#[derive(Debug, Clone)]
pub struct Query {
    pub conditions: Vec<QueryCondition>,
}

/// A multi-index that maps field names to their corresponding indices.
pub struct MultiIndex {
    /// A mapping from field names to their corresponding index.
    pub indices: HashMap<String, Box<dyn SearchableIndex>>,
}

impl Default for MultiIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl MultiIndex {
    /// Create a new, empty multi-index.
    pub fn new() -> Self {
        Self {
            indices: HashMap::new(),
        }
    }

    /// Add an index for a field.
    pub fn add_index(&mut self, field_name: String, index: Box<dyn SearchableIndex>) {
        self.indices.insert(field_name, index);
    }

    /// Execute a query against the multi-index.
    ///
    /// Returns a vector of offsets for records that match all conditions in the query.
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

    /// Performs a streaming query on the multi-index without loading the entire index into memory.
    /// This is useful for large indices where loading the entire index would be inefficient.
    ///
    /// # Arguments
    ///
    /// * `reader` - A reader positioned at the start of the index data
    /// * `query` - The query to execute
    /// * `index_offsets` - A map of field names to their byte offsets in the file
    ///
    /// # Returns
    ///
    /// A vector of value offsets that match the query
    pub fn stream_query<R: Read + Seek>(
        &self,
        reader: &mut R,
        query: &Query,
        index_offsets: &HashMap<String, u64>,
    ) -> std::io::Result<Vec<ValueOffset>> {
        // If there are no conditions, return an empty result.
        if query.conditions.is_empty() {
            return Ok(Vec::new());
        }

        let field_names: Vec<String> = query.conditions.iter().map(|c| c.field.clone()).collect();

        // Only load the indices needed for this query
        let filtered_offsets: HashMap<String, u64> = index_offsets
            .iter()
            .filter(|(k, _)| field_names.contains(k))
            .map(|(k, v)| (k.clone(), *v))
            .collect();

        let streamable_index =
            StreamableMultiIndex::from_reader(reader, &field_names, &filtered_offsets)?;

        // Execute the query using the streamable index
        streamable_index.stream_query(reader, query)
    }

    #[cfg(feature = "http")]
    /// Performs a streaming query on the multi-index over HTTP without loading the entire index into memory.
    /// This is useful for large indices where loading the entire index would be inefficient.
    ///
    /// # Arguments
    ///
    /// * `client` - An HTTP client for making range requests
    /// * `query` - The query to execute
    /// * `index_offsets` - A map of field names to their byte offsets in the file
    /// * `feature_begin` - The byte offset where the feature data begins
    ///
    /// # Returns
    ///
    /// A vector of HTTP search result items that match the query
    pub async fn http_stream_query<T: AsyncHttpRangeClient>(
        &self,
        client: &mut AsyncBufferedHttpRangeClient<T>,
        query: &Query,
        index_offsets: &HashMap<String, usize>,
        feature_begin: usize,
    ) -> std::io::Result<Vec<HttpSearchResultItem>> {
        // If there are no conditions, return an empty result.
        if query.conditions.is_empty() {
            return Ok(Vec::new());
        }

        // Create a StreamableMultiIndex from the HTTP client
        let field_names: Vec<String> = query.conditions.iter().map(|c| c.field.clone()).collect();

        // Only load the indices needed for this query
        let filtered_offsets: HashMap<String, usize> = index_offsets
            .iter()
            .filter(|(k, _)| field_names.contains(k))
            .map(|(k, v)| (k.clone(), *v))
            .collect();

        let streamable_index =
            StreamableMultiIndex::from_http(client, &field_names, &filtered_offsets).await?;

        // Get the minimum index offset to use as the base
        let min_offset = index_offsets.values().min().copied().unwrap_or(0);

        // Execute the query using the streamable index
        streamable_index
            .http_stream_query(client, query, min_offset, feature_begin)
            .await
    }
}

#[cfg(feature = "http")]
#[derive(Debug, Clone)]
pub enum HttpRange {
    Range(Range<usize>),
    RangeFrom(std::ops::RangeFrom<usize>),
}

#[cfg(feature = "http")]
impl HttpRange {
    pub fn start(&self) -> usize {
        match self {
            HttpRange::Range(range) => range.start,
            HttpRange::RangeFrom(range) => range.start,
        }
    }

    pub fn end(&self) -> Option<usize> {
        match self {
            HttpRange::Range(range) => Some(range.end),
            HttpRange::RangeFrom(_) => None,
        }
    }
}

#[cfg(feature = "http")]
#[derive(Debug, Clone)]
pub struct HttpSearchResultItem {
    /// Byte range in the feature data section
    pub range: HttpRange,
}

/// A multi-index that can be streamed from a reader.
pub struct StreamableMultiIndex {
    /// A mapping from field names to their corresponding index metadata.
    pub indices: HashMap<String, IndexMeta>,
}

impl Default for StreamableMultiIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamableMultiIndex {
    /// Create a new, empty streamable multi-index.
    pub fn new() -> Self {
        Self {
            indices: HashMap::new(),
        }
    }

    /// Add an index for a field.
    pub fn add_index(&mut self, field_name: String, index: IndexMeta) {
        self.indices.insert(field_name, index);
    }

    /// Create a streamable multi-index from a reader.
    pub fn from_reader<R: Read + Seek>(
        reader: &mut R,
        field_names: &[String],
        index_offsets: &HashMap<String, u64>,
    ) -> std::io::Result<Self> {
        let mut indices = HashMap::new();

        for field_name in field_names {
            if let Some(&offset) = index_offsets.get(field_name) {
                reader.seek(std::io::SeekFrom::Start(offset))?;
                let meta = IndexMeta::from_reader(reader)?;
                indices.insert(field_name.clone(), meta);
            }
        }

        Ok(Self { indices })
    }

    /// Create a streamable multi-index from an HTTP client.
    #[cfg(feature = "http")]
    pub async fn from_http<T: AsyncHttpRangeClient>(
        client: &mut AsyncBufferedHttpRangeClient<T>,
        field_names: &[String],
        index_offsets: &HashMap<String, usize>,
    ) -> std::io::Result<Self> {
        let mut indices = HashMap::new();

        for field_name in field_names {
            if let Some(&offset) = index_offsets.get(field_name) {
                // Read the index metadata from the HTTP client.
                let meta_size = std::mem::size_of::<u64>() * 2 + std::mem::size_of::<u32>();
                let meta_bytes = client
                    .get_range(offset, meta_size)
                    .await
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

                let entry_count = u64::from_le_bytes([
                    meta_bytes[0],
                    meta_bytes[1],
                    meta_bytes[2],
                    meta_bytes[3],
                    meta_bytes[4],
                    meta_bytes[5],
                    meta_bytes[6],
                    meta_bytes[7],
                ]);
                let size = u64::from_le_bytes([
                    meta_bytes[8],
                    meta_bytes[9],
                    meta_bytes[10],
                    meta_bytes[11],
                    meta_bytes[12],
                    meta_bytes[13],
                    meta_bytes[14],
                    meta_bytes[15],
                ]);

                let meta = IndexMeta {
                    entry_count,
                    size,
                    type_id: 1, // TODO: get the type id from the index
                };
                indices.insert(field_name.clone(), meta);
            }
        }

        Ok(Self { indices })
    }

    /// Execute a query against the streamable multi-index.
    ///
    /// Returns a vector of offsets for records that match all conditions in the query.
    pub fn stream_query<R: Read + Seek>(
        &self,
        reader: &mut R,
        query: &Query,
    ) -> std::io::Result<Vec<ValueOffset>> {
        // If there are no conditions, return an empty result.
        if query.conditions.is_empty() {
            return Ok(Vec::new());
        }

        let mut candidate_sets: Vec<HashSet<ValueOffset>> = Vec::new();

        // Process all conditions and collect candidate sets
        for condition in &query.conditions {
            if let Some(index_meta) = self.indices.get(&condition.field) {
                let offsets: Vec<ValueOffset> = match condition.operator {
                    Operator::Eq => {
                        // Exactly equal
                        index_meta.stream_query_exact(reader, &condition.key)?
                    }
                    Operator::Ne => {
                        // All offsets minus those equal to the key
                        let all_offsets = index_meta.stream_query_range(reader, None, None)?;
                        let eq_offsets = index_meta.stream_query_exact(reader, &condition.key)?;
                        let eq_set: HashSet<ValueOffset> = eq_offsets.into_iter().collect();
                        all_offsets
                            .into_iter()
                            .filter(|o| !eq_set.contains(o))
                            .collect()
                    }
                    Operator::Gt => {
                        // Keys strictly greater than the boundary (exclude equality)
                        let range_offsets =
                            index_meta.stream_query_range(reader, Some(&condition.key), None)?;
                        let eq_offsets = index_meta.stream_query_exact(reader, &condition.key)?;
                        let eq_set: HashSet<ValueOffset> = eq_offsets.into_iter().collect();
                        range_offsets
                            .into_iter()
                            .filter(|o| !eq_set.contains(o))
                            .collect()
                    }
                    Operator::Ge => {
                        // Keys greater than or equal to the boundary
                        index_meta.stream_query_range(reader, Some(&condition.key), None)?
                    }
                    Operator::Lt => {
                        // Keys strictly less than the boundary
                        index_meta.stream_query_range(reader, None, Some(&condition.key))?
                    }
                    Operator::Le => {
                        // Keys less than or equal to the boundary
                        // We need to include both the range and exact matches
                        let mut range_offsets =
                            index_meta.stream_query_range(reader, None, Some(&condition.key))?;
                        let eq_offsets = index_meta.stream_query_exact(reader, &condition.key)?;

                        // Combine both sets (may contain duplicates)
                        range_offsets.extend(eq_offsets);

                        // Remove duplicates by collecting into a set and back to a vector
                        let set: HashSet<ValueOffset> = range_offsets.into_iter().collect();
                        set.into_iter().collect()
                    }
                };

                candidate_sets.push(offsets.into_iter().collect());
            }
        }

        if candidate_sets.is_empty() {
            return Ok(Vec::new());
        }

        // Intersect all candidate sets to get the final result
        let mut intersection: HashSet<ValueOffset> = candidate_sets.remove(0);
        for set in candidate_sets {
            intersection = intersection.intersection(&set).cloned().collect();
        }

        // Sort the results for consistent output
        let mut result: Vec<ValueOffset> = intersection.into_iter().collect();
        result.sort();

        Ok(result)
    }

    /// Execute a query against the streamable multi-index using HTTP range requests.
    ///
    /// Returns a vector of HttpSearchResultItem for records that match all conditions in the query.
    #[cfg(feature = "http")]
    pub async fn http_stream_query<T: AsyncHttpRangeClient>(
        &self,
        client: &mut AsyncBufferedHttpRangeClient<T>,
        query: &Query,
        index_offset: usize,
        feature_begin: usize,
    ) -> std::io::Result<Vec<HttpSearchResultItem>> {
        // If there are no conditions, return an empty result.
        if query.conditions.is_empty() {
            return Ok(Vec::new());
        }

        // TODO: implement this
        let matching_items: Vec<HttpSearchResultItem> = Vec::new();

        Ok(matching_items)
    }

    /// Performs a streaming query on the multi-index over HTTP with optimized batching.
    /// This groups nearby feature offsets to reduce the number of HTTP requests.
    ///
    /// # Arguments
    ///
    /// * `client` - An HTTP client for making range requests
    /// * `query` - The query to execute
    /// * `index_offset` - The byte offset where the index data begins
    /// * `feature_begin` - The byte offset where the feature data begins
    /// * `batch_threshold` - The maximum distance between offsets to combine into a single request
    ///
    /// # Returns
    ///
    /// A vector of HTTP search result items that match the query
    pub async fn http_stream_query_batched<T: AsyncHttpRangeClient>(
        &self,
        client: &mut AsyncBufferedHttpRangeClient<T>,
        query: &Query,
        index_offset: usize,
        feature_begin: usize,
        batch_threshold: usize,
    ) -> std::io::Result<Vec<HttpSearchResultItem>> {
        // Get the raw results
        let mut results = self
            .http_stream_query(client, query, index_offset, feature_begin)
            .await?;

        // If there are no results or only one result, return as is
        if results.len() <= 1 {
            return Ok(results);
        }

        // Sort results by start offset to optimize batching
        results.sort_by_key(|item| item.range.start());

        // TODO: implement this. Make batches of results that are close to each other.
        let mut batched_results = Vec::new();

        // Todo: Add the final batch

        Ok(batched_results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::byte_serializable::ByteSerializable;
    use crate::sorted_index::{BufferedIndex, IndexSerializable, KeyValue};
    use async_trait::async_trait;
    use ordered_float::OrderedFloat;
    use std::io::Cursor;
    use std::vec;

    #[cfg(feature = "http")]
    use bytes::Bytes;
    #[cfg(feature = "http")]
    use http_range_client::{AsyncHttpRangeClient, HttpError};
    #[cfg(feature = "http")]
    use std::sync::{Arc, Mutex};

    // Helper function to create a sample index for testing.
    fn create_sample_height_index() -> BufferedIndex<OrderedFloat<f32>> {
        let mut entries = Vec::new();
        let mut index = BufferedIndex::new();

        // Create sample data with heights
        let heights = [
            (10.5f32, vec![0]),    // Building 0 has height 10.5
            (15.2f32, vec![1]),    // Building 1 has height 15.2
            (20.0f32, vec![2, 3]), // Buildings 2 and 3 have height 20.0
            (22.7f32, vec![4]),
            (25.3f32, vec![5]),
            (30.0f32, vec![6, 7, 8]), // Buildings 6, 7, 8 have height 30.0
            (32.1f32, vec![9]),
            (35.5f32, vec![10]),
            (40.0f32, vec![11, 12]),
            (45.2f32, vec![13]),
            (50.0f32, vec![14, 15, 16]), // Buildings 14, 15, 16 have height 50.0
            (55.7f32, vec![17]),
            (60.3f32, vec![18]),
            (65.0f32, vec![19]),
        ];

        for (height, offsets) in heights.iter() {
            entries.push(KeyValue {
                key: OrderedFloat(*height),
                offsets: offsets.iter().map(|&i| i as u64).collect(),
            });
        }
        index.build_index(entries);
        index
    }

    fn create_sample_id_index() -> BufferedIndex<String> {
        let mut index = BufferedIndex::new();
        let mut entries = Vec::new();
        let ids = [
            ("BLDG0001", vec![0]),
            ("BLDG0002", vec![1]),
            ("BLDG0003", vec![2]),
            ("BLDG0004", vec![3]),
            ("BLDG0005", vec![4]),
            ("BLDG0010", vec![5, 6]), // Two buildings share the same ID
            ("BLDG0015", vec![7]),
            ("BLDG0020", vec![8, 9, 10]), // Three buildings share the same ID
            ("BLDG0025", vec![11]),
            ("BLDG0030", vec![12]),
            ("BLDG0035", vec![13]),
            ("BLDG0040", vec![14]),
            ("BLDG0045", vec![15]),
            ("BLDG0050", vec![16, 17]), // Two buildings share the same ID
            ("BLDG0055", vec![18]),
            ("BLDG0060", vec![19]),
        ];

        for (id, offsets) in ids.iter() {
            entries.push(KeyValue {
                key: id.to_string(),
                offsets: offsets.iter().map(|&i| i as u64).collect(),
            });
        }
        index.build_index(entries);
        index
    }

    // Helper function to create a serialized height index for testing.
    fn create_serialized_height_index() -> Vec<u8> {
        let index = create_sample_height_index();
        let mut buffer = Vec::new();
        index.serialize(&mut buffer).unwrap();
        buffer
    }

    // Helper function to create a serialized type index for testing
    fn create_serialized_id_index() -> Vec<u8> {
        let index = create_sample_id_index();

        // Serialize the index
        let mut buffer = Vec::new();
        index.serialize(&mut buffer).unwrap();
        buffer
    }

    #[test]
    fn test_streamable_multi_index_from_reader() -> std::io::Result<()> {
        // Create serialized indices.
        let height_index = create_serialized_height_index();
        let id_index = create_serialized_id_index();

        // Create a buffer with all indices.
        let mut buffer = Vec::new();
        buffer.extend_from_slice(&height_index);
        buffer.extend_from_slice(&id_index);

        // Create a reader from the buffer.
        let mut reader = Cursor::new(buffer);

        // Create a mapping from field names to index offsets.
        let mut index_offsets = HashMap::new();
        index_offsets.insert("height".to_string(), 0);
        index_offsets.insert("id".to_string(), height_index.len() as u64);

        // Create a streamable multi-index from the reader.
        let field_names = vec!["height".to_string(), "id".to_string()];
        let multi_index =
            StreamableMultiIndex::from_reader(&mut reader, &field_names, &index_offsets)?;

        // Check that the indices were loaded correctly.
        assert_eq!(multi_index.indices.len(), 2);
        assert!(multi_index.indices.contains_key("height"));
        assert!(multi_index.indices.contains_key("id"));
        assert!(multi_index.indices.get("height").unwrap().entry_count > 0);
        assert!(multi_index.indices.get("id").unwrap().entry_count > 0);

        Ok(())
    }

    #[test]
    fn test_streamable_multi_index_queries() -> std::io::Result<()> {
        // Create serialized indices.
        let height_index = create_serialized_height_index();
        let id_index = create_serialized_id_index();

        // Create a buffer with all indices.
        let mut buffer = Vec::new();
        buffer.extend_from_slice(&height_index);
        buffer.extend_from_slice(&id_index);

        // Create a mapping from field names to index offsets.
        let mut index_offsets = HashMap::new();
        index_offsets.insert("height".to_string(), 0);
        index_offsets.insert("id".to_string(), height_index.len() as u64);

        // Define test cases with queries and expected results
        struct TestCase {
            name: &'static str,
            query: Query,
            expected: Vec<u64>,
        }

        let test_cases = vec![
            // Test case 1: Single condition - height > 30.0 (Gt)
            TestCase {
                name: "single_gt_height",
                query: Query {
                    conditions: vec![QueryCondition {
                        field: "height".to_string(),
                        operator: Operator::Gt,
                        key: OrderedFloat(30.0f32).to_bytes(),
                    }],
                },
                // Buildings 9-19 have heights > 30.0 (excluding 6, 7, 8 which have height exactly 30.0)
                expected: (9..=19).collect(),
            },
            // Test case 2: Single condition - height >= 30.0 (Ge)
            TestCase {
                name: "single_ge_height",
                query: Query {
                    conditions: vec![QueryCondition {
                        field: "height".to_string(),
                        operator: Operator::Ge,
                        key: OrderedFloat(30.0f32).to_bytes(),
                    }],
                },
                // Buildings 6-19 have heights >= 30.0
                expected: (6..=19).collect(),
            },
            // Test case 3: Single condition - height < 15.0 (Lt)
            TestCase {
                name: "single_lt_height",
                query: Query {
                    conditions: vec![QueryCondition {
                        field: "height".to_string(),
                        operator: Operator::Lt,
                        key: OrderedFloat(15.0f32).to_bytes(),
                    }],
                },
                // Only building 0 has height < 15.0
                expected: vec![0],
            },
            // Test case 4: Single condition - height <= 15.2 (Le)
            TestCase {
                name: "single_le_height",
                query: Query {
                    conditions: vec![QueryCondition {
                        field: "height".to_string(),
                        operator: Operator::Le,
                        key: OrderedFloat(15.2f32).to_bytes(),
                    }],
                },
                // Buildings 0 and 1 have heights <= 15.2
                expected: vec![0, 1],
            },
            // Test case 5: Single condition - id = "BLDG0020" (Eq)
            TestCase {
                name: "single_eq_id",
                query: Query {
                    conditions: vec![QueryCondition {
                        field: "id".to_string(),
                        operator: Operator::Eq,
                        key: "BLDG0020".to_string().to_bytes(),
                    }],
                },
                // Buildings 8, 9, 10 have id "BLDG0020"
                expected: vec![8, 9, 10],
            },
            // Test case 6: Single condition - id != "BLDG0020" (Ne)
            TestCase {
                name: "single_ne_id",
                query: Query {
                    conditions: vec![QueryCondition {
                        field: "id".to_string(),
                        operator: Operator::Ne,
                        key: "BLDG0020".to_string().to_bytes(),
                    }],
                },
                // All buildings except 8, 9, 10 have id != "BLDG0020"
                // Based on the sample data, we should have buildings 0-7 and 11-19
                expected: {
                    let mut result = Vec::new();
                    result.extend(0..8);
                    result.extend(11..20);
                    result
                },
            },
            // Test case 7: Multiple conditions - height > 20.0 AND id = "BLDG0001" (Gt & Eq)
            TestCase {
                name: "multiple_gt_height_and_eq_id",
                query: Query {
                    conditions: vec![
                        QueryCondition {
                            field: "height".to_string(),
                            operator: Operator::Gt,
                            key: OrderedFloat(20.0f32).to_bytes(),
                        },
                        QueryCondition {
                            field: "id".to_string(),
                            operator: Operator::Eq,
                            key: "BLDG0001".to_string().to_bytes(),
                        },
                    ],
                },
                // Only building 0 matches both conditions
                expected: vec![],
            },
            // Test case 8: Multiple conditions - height <= 30.0 AND id != "BLDG0001" (Le & Ne)
            TestCase {
                name: "multiple_le_height_and_ne_id",
                query: Query {
                    conditions: vec![
                        QueryCondition {
                            field: "height".to_string(),
                            operator: Operator::Le,
                            key: OrderedFloat(30.0f32).to_bytes(),
                        },
                        QueryCondition {
                            field: "id".to_string(),
                            operator: Operator::Ne,
                            key: "BLDG0001".to_string().to_bytes(),
                        },
                    ],
                },
                // Buildings with height <= 30.0 (0-8) except building 0 (which has id "BLDG0001")
                expected: vec![1],
            },
        ];

        // Run all test cases
        for test_case in test_cases {
            println!("Running test case: {}", test_case.name);

            // Create a reader from the buffer.
            let mut reader = Cursor::new(buffer.clone());

            // Create a streamable multi-index from the reader.
            let field_names: Vec<String> = test_case
                .query
                .conditions
                .iter()
                .map(|c| c.field.clone())
                .collect();

            let multi_index =
                StreamableMultiIndex::from_reader(&mut reader, &field_names, &index_offsets)?;

            // Execute the query.
            let mut reader = Cursor::new(buffer.clone());
            let result = multi_index.stream_query(&mut reader, &test_case.query)?;

            // Verify the results.
            assert_eq!(
                result, test_case.expected,
                "Test case '{}' failed: expected {:?}, got {:?}",
                test_case.name, test_case.expected, result
            );
        }

        Ok(())
    }
}
