use crate::sorted_index::{
    AnyIndex, SortedIndexMeta, StreamableIndex, ValueOffset,
};
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
