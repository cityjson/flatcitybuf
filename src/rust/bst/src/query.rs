use crate::sorted_index::{AnyIndex, SortedIndexMeta, StreamableIndex, ValueOffset};
use crate::Error;
use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek};

#[cfg(feature = "http")]
use http_range_client::{AsyncBufferedHttpRangeClient, AsyncHttpRangeClient};

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

/// Create a StreamableMultiIndex from a regular MultiIndex
pub fn create_streamable_index(m_indices: &MultiIndex) -> StreamableMultiIndex {
    // For now, we don't have a way to convert from AnyIndex to SortedIndexMeta
    // This would need to be implemented in a real-world scenario

    StreamableMultiIndex::new()
}

/// Convert a regular Query to use with StreamableMultiIndex
#[cfg(feature = "http")]
pub async fn stream_query_with_streamable(
    s_indices: &StreamableMultiIndex,
    query: Query,
    client: &mut AsyncBufferedHttpRangeClient<impl AsyncHttpRangeClient>,
    index_offset: usize,
    feature_begin: usize,
) -> Result<Vec<HttpSearchResultItem>, Error> {
    s_indices
        .http_stream_query(client, &query, index_offset, feature_begin)
        .await
        .map_err(|e| {
            Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            ))
        })
}

/// Legacy stream_query function that uses the in-memory MultiIndex
#[cfg(feature = "http")]
pub async fn stream_query(
    m_indices: &MultiIndex,
    query: Query,
    feature_begin: usize,
) -> Result<Vec<HttpSearchResultItem>, Error> {
    // This is the legacy implementation that uses the in-memory MultiIndex
    // It's kept for backward compatibility

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

/// Stream query using the StreamableMultiIndex for better performance with HTTP range requests.
/// This is the recommended approach for new code.
#[cfg(feature = "http")]
pub async fn stream_query_streamable<T: AsyncHttpRangeClient>(
    s_indices: &StreamableMultiIndex,
    query: Query,
    client: &mut AsyncBufferedHttpRangeClient<T>,
    index_offset: usize,
    feature_begin: usize,
) -> Result<Vec<HttpSearchResultItem>, Error> {
    s_indices
        .http_stream_query(client, &query, index_offset, feature_begin)
        .await
        .map_err(|e| {
            Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            ))
        })
}

/// A streamable version of MultiIndex that doesn't load the entire index into memory.
#[derive(Default)]
pub struct StreamableMultiIndex {
    /// A mapping from field names to their corresponding index metadata.
    pub indices: HashMap<String, SortedIndexMeta>,
}

impl StreamableMultiIndex {
    /// Create a new empty StreamableMultiIndex.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a streamable index for a field.
    pub fn add_index(&mut self, field_name: String, index: SortedIndexMeta) {
        self.indices.insert(field_name, index);
    }

    /// Create a StreamableMultiIndex from a file reader.
    pub fn from_reader<R: Read + Seek>(
        reader: &mut R,
        field_names: &[String],
        index_offsets: &HashMap<String, u64>,
    ) -> std::io::Result<Self> {
        let mut indices = HashMap::new();

        for field_name in field_names {
            if let Some(&offset) = index_offsets.get(field_name) {
                // Seek to the index position
                reader.seek(std::io::SeekFrom::Start(offset))?;

                // Read the index metadata
                let meta = SortedIndexMeta::from_reader(reader)?;

                // Add the index to the map
                indices.insert(field_name.clone(), meta);
            }
        }

        Ok(Self { indices })
    }

    #[cfg(feature = "http")]
    /// Create a StreamableMultiIndex from HTTP range requests.
    pub async fn from_http<T: AsyncHttpRangeClient>(
        client: &mut AsyncBufferedHttpRangeClient<T>,
        field_names: &[String],
        index_offsets: &HashMap<String, usize>,
    ) -> std::io::Result<Self> {
        use std::io::{Error, ErrorKind};

        let mut indices = HashMap::new();

        for field_name in field_names {
            if let Some(&offset) = index_offsets.get(field_name) {
                // Read the type identifier (4 bytes)
                let type_id_range = offset..(offset + 4);
                let type_id_bytes = client
                    .min_req_size(0)
                    .get_range(type_id_range.start, type_id_range.len())
                    .await
                    .map_err(|e| Error::new(ErrorKind::Other, e.to_string()))?;

                let type_id = u32::from_le_bytes(type_id_bytes.as_ref().try_into().unwrap());

                // Read the entry count (8 bytes)
                let entry_count_range = (offset + 4)..(offset + 12);
                let entry_count_bytes = client
                    .min_req_size(0)
                    .get_range(entry_count_range.start, entry_count_range.len())
                    .await
                    .map_err(|e| Error::new(ErrorKind::Other, e.to_string()))?;

                let entry_count =
                    u64::from_le_bytes(entry_count_bytes.as_ref().try_into().unwrap());

                // Calculate the size of the index
                // This is a simplification - in a real implementation, we would need to
                // read through the index to determine its exact size
                let size = 12; // type_id (4 bytes) + entry_count (8 bytes)

                // Create the index metadata
                let meta = SortedIndexMeta {
                    entry_count,
                    size,
                    type_id,
                };

                // Add the index to the map
                indices.insert(field_name.clone(), meta);
            }
        }

        Ok(Self { indices })
    }

    /// Query the index using a file reader.
    pub fn stream_query<R: Read + Seek>(
        &self,
        reader: &mut R,
        query: &Query,
    ) -> std::io::Result<Vec<ValueOffset>> {
        // Compute candidate offset set for each query condition.
        let mut candidate_sets: Vec<HashSet<ValueOffset>> = Vec::new();

        for condition in query.conditions.iter() {
            if let Some(idx) = self.indices.get(&condition.field) {
                let offsets: Vec<ValueOffset> = match condition.operator {
                    Operator::Eq => idx.stream_query_exact(reader, &condition.key)?,
                    Operator::Gt => {
                        let offsets = idx.stream_query_range(reader, Some(&condition.key), None)?;
                        let eq = idx.stream_query_exact(reader, &condition.key)?;
                        offsets.into_iter().filter(|o| !eq.contains(o)).collect()
                    }
                    Operator::Ge => idx.stream_query_range(reader, Some(&condition.key), None)?,
                    Operator::Lt => idx.stream_query_range(reader, None, Some(&condition.key))?,
                    Operator::Le => {
                        let mut offsets =
                            idx.stream_query_range(reader, None, Some(&condition.key))?;
                        let eq = idx.stream_query_exact(reader, &condition.key)?;
                        offsets.extend(eq);
                        // Remove duplicates.
                        offsets
                            .into_iter()
                            .collect::<HashSet<_>>()
                            .into_iter()
                            .collect()
                    }
                    Operator::Ne => {
                        let all: HashSet<ValueOffset> = idx
                            .stream_query_range(reader, None, None)?
                            .into_iter()
                            .collect();
                        let eq: HashSet<ValueOffset> = idx
                            .stream_query_exact(reader, &condition.key)?
                            .into_iter()
                            .collect();
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

        Ok(offsets)
    }

    /// Query the index using HTTP range requests.
    #[cfg(feature = "http")]
    pub async fn http_stream_query<T: AsyncHttpRangeClient>(
        &self,
        client: &mut AsyncBufferedHttpRangeClient<T>,
        query: &Query,
        index_offset: usize,
        feature_begin: usize,
    ) -> std::io::Result<Vec<HttpSearchResultItem>> {
        // Compute candidate offset set for each query condition.
        let mut candidate_sets: Vec<HashSet<ValueOffset>> = Vec::new();

        for condition in query.conditions.iter() {
            if let Some(idx) = self.indices.get(&condition.field) {
                let offsets: Vec<ValueOffset> = match condition.operator {
                    Operator::Eq => {
                        idx.http_stream_query_exact(client, index_offset, &condition.key)
                            .await?
                    }
                    Operator::Gt => {
                        let offsets = idx
                            .http_stream_query_range(
                                client,
                                index_offset,
                                Some(&condition.key),
                                None,
                            )
                            .await?;
                        let eq = idx
                            .http_stream_query_exact(client, index_offset, &condition.key)
                            .await?;
                        offsets.into_iter().filter(|o| !eq.contains(o)).collect()
                    }
                    Operator::Ge => {
                        idx.http_stream_query_range(
                            client,
                            index_offset,
                            Some(&condition.key),
                            None,
                        )
                        .await?
                    }
                    Operator::Lt => {
                        idx.http_stream_query_range(
                            client,
                            index_offset,
                            None,
                            Some(&condition.key),
                        )
                        .await?
                    }
                    Operator::Le => {
                        let mut offsets = idx
                            .http_stream_query_range(
                                client,
                                index_offset,
                                None,
                                Some(&condition.key),
                            )
                            .await?;
                        let eq = idx
                            .http_stream_query_exact(client, index_offset, &condition.key)
                            .await?;
                        offsets.extend(eq);
                        // Remove duplicates.
                        offsets
                            .into_iter()
                            .collect::<HashSet<_>>()
                            .into_iter()
                            .collect()
                    }
                    Operator::Ne => {
                        let all: HashSet<ValueOffset> = idx
                            .http_stream_query_range(client, index_offset, None, None)
                            .await?
                            .into_iter()
                            .collect();
                        let eq: HashSet<ValueOffset> = idx
                            .http_stream_query_exact(client, index_offset, &condition.key)
                            .await?
                            .into_iter()
                            .collect();
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

        // Convert offsets to HTTP ranges
        let http_ranges: Vec<HttpSearchResultItem> = offsets
            .into_iter()
            .map(|offset| HttpSearchResultItem {
                range: HttpRange::RangeFrom(offset as usize + feature_begin..),
            })
            .collect();

        Ok(http_ranges)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::byte_serializable::ByteSerializable;
    use crate::sorted_index::{IndexSerializable, KeyValue, SortedIndex};
    use ordered_float::OrderedFloat;
    use std::io::{Cursor, Seek, SeekFrom};

    #[cfg(feature = "http")]
    use {
        async_trait::async_trait,
        bytes::Bytes,
        http_range_client::{
            AsyncBufferedHttpRangeClient, AsyncHttpRangeClient, Result as HttpResult,
        },
        std::sync::{Arc, Mutex},
        tokio::test as tokio_test,
    };

    // Helper function to create a serialized index buffer
    fn create_serialized_height_index() -> Vec<u8> {
        // Create a height index directly
        let mut height_entries = Vec::new();

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
            height_entries.push(KeyValue {
                key: OrderedFloat(*height),
                offsets: offsets.iter().map(|&i| i as u64).collect(),
            });
        }

        let mut height_index = SortedIndex::new();
        height_index.build_index(height_entries);

        // Serialize the index
        let mut buffer = Vec::new();
        height_index.serialize(&mut buffer).unwrap();
        buffer
    }

    #[test]
    fn test_streamable_multi_index_from_reader() -> std::io::Result<()> {
        // Create a serialized index buffer
        let buffer = create_serialized_height_index();

        // Create a cursor for the buffer
        let mut cursor = Cursor::new(buffer.clone());

        // Create a map of field names to offsets
        let mut index_offsets = HashMap::new();
        index_offsets.insert("height".to_string(), 0);

        // Create a StreamableMultiIndex from the reader
        let streamable_index = StreamableMultiIndex::from_reader(
            &mut cursor,
            &["height".to_string()],
            &index_offsets,
        )?;

        // Verify the index was created correctly
        assert!(streamable_index.indices.contains_key("height"));

        // Test streaming query
        cursor.seek(SeekFrom::Start(0))?;

        // Create a query for height = 30.0
        let test_height = OrderedFloat(30.0f32);
        let height_bytes = test_height.to_bytes();

        let query = Query {
            conditions: vec![QueryCondition {
                field: "height".to_string(),
                operator: Operator::Eq,
                key: height_bytes.clone(),
            }],
        };

        // Execute the query
        let stream_results = streamable_index.stream_query(&mut cursor, &query)?;

        // Verify the actual values
        assert_eq!(
            stream_results,
            vec![6, 7, 8],
            "Expected buildings 6, 7, 8 to have height 30.0"
        );

        Ok(())
    }

    #[cfg(feature = "http")]
    struct MockHttpClient {
        data: Arc<Mutex<Vec<u8>>>,
    }

    #[cfg(feature = "http")]
    #[async_trait]
    impl AsyncHttpRangeClient for MockHttpClient {
        async fn get_range(&self, _url: &str, range: &str) -> HttpResult<Bytes> {
            // Parse the range header
            let range_str = range.strip_prefix("bytes=").unwrap();
            let parts: Vec<&str> = range_str.split('-').collect();
            let start: usize = parts[0].parse().unwrap();
            let end: usize = parts[1].parse().unwrap();

            // Get the data
            let data = self.data.lock().unwrap();
            let slice = data[start..=end].to_vec();

            Ok(Bytes::from(slice))
        }

        async fn head_response_header(
            &self,
            _url: &str,
            _header: &str,
        ) -> HttpResult<Option<String>> {
            Ok(None)
        }
    }

    #[cfg(feature = "http")]
    #[tokio_test]
    async fn test_streamable_multi_index_http_query() -> std::io::Result<()> {
        // Create a serialized index buffer
        let buffer = create_serialized_height_index();

        // Create a mock HTTP client with the serialized data
        let data = Arc::new(Mutex::new(buffer.clone()));
        let mock_client = MockHttpClient { data };
        let mut buffered_client = AsyncBufferedHttpRangeClient::with(mock_client, "test-url");

        // Create a map of field names to offsets
        let mut index_offsets = HashMap::new();
        index_offsets.insert("height".to_string(), 0);

        // Create a StreamableMultiIndex from HTTP
        let streamable_index = StreamableMultiIndex::from_http(
            &mut buffered_client,
            &["height".to_string()],
            &index_offsets,
        )
        .await?;

        // Verify the index was created correctly
        assert!(streamable_index.indices.contains_key("height"));

        // Create a query for height = 30.0
        let test_height = OrderedFloat(30.0f32);
        let height_bytes = test_height.to_bytes();

        let query = Query {
            conditions: vec![QueryCondition {
                field: "height".to_string(),
                operator: Operator::Eq,
                key: height_bytes.clone(),
            }],
        };

        // Execute the HTTP query
        let http_results = streamable_index
            .http_stream_query(
                &mut buffered_client,
                &query,
                0,
                100, // Feature begin offset
            )
            .await?;

        // Extract offsets from HTTP results
        let http_offsets: Vec<ValueOffset> = http_results
            .iter()
            .map(|item| match &item.range {
                HttpRange::Range(range) => (range.start - 100) as u64,
                HttpRange::RangeFrom(range) => (range.start - 100) as u64,
            })
            .collect();

        // Verify the actual values (after adjusting for the feature_begin offset)
        let mut sorted_offsets = http_offsets.clone();
        sorted_offsets.sort();
        assert_eq!(
            sorted_offsets,
            vec![6, 7, 8],
            "Expected buildings 6, 7, 8 to have height 30.0"
        );

        Ok(())
    }
}
