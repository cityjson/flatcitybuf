use crate::sorted_index::{SearchableIndex, ValueOffset};
use crate::{error, sorted_index, ByteSerializable, ByteSerializableType};
use std::collections::HashMap;
use std::collections::HashSet;
use std::io::{Read, Seek, SeekFrom};

use chrono::{DateTime, Utc};
#[cfg(feature = "http")]
use http_range_client::{AsyncBufferedHttpRangeClient, AsyncHttpRangeClient};

#[cfg(feature = "http")]
use std::ops::Range;

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
    ) -> Result<Vec<ValueOffset>, error::Error> {
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

        let streamable_index = StreamableMultiIndex::from_reader(reader, &filtered_offsets)?;

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
        todo!()
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

/// Type-erased IndexMeta that can work with any ByteSerializable type.
/// This allows us to store different IndexMeta<T> instances in a HashMap.
#[derive(Debug, Clone)]
pub struct TypeErasedIndexMeta {
    /// Number of entries in the index.
    pub entry_count: u64,
    /// Total size of the index in bytes.
    pub size: u64,
    /// Type identifier for the index.
    pub type_id: ByteSerializableType,
}

impl TypeErasedIndexMeta {
    /// Create a new TypeErasedIndexMeta from an IndexMeta<T>.
    pub fn from_generic<T: ByteSerializable + Ord + Send + Sync + 'static>(
        index_meta: &sorted_index::IndexMeta<T>,
        type_id: ByteSerializableType,
    ) -> Self {
        Self {
            entry_count: index_meta.entry_count,
            size: index_meta.size,
            type_id,
        }
    }

    /// Read and deserialize stream query exact results.
    pub fn stream_query_exact<R: Read + Seek>(
        &self,
        reader: &mut R,
        key: &[u8],
    ) -> std::io::Result<Vec<ValueOffset>> {
        // Store current position to restore later
        let start_pos = reader.stream_position()?;

        // Skip the type ID (4 bytes) and entry count (8 bytes)
        reader.seek(SeekFrom::Start(start_pos + 12))?;

        // Binary search through the index
        let mut left = 0;
        let mut right = self.entry_count as i64 - 1;
        let mut result = Vec::new();

        while left <= right {
            let mid = left + (right - left) / 2;

            // Seek to the entry at the mid position
            self.seek_to_entry(reader, mid as u64, start_pos)?;

            // Read key length and key
            let mut key_len_bytes = [0u8; 8];
            reader.read_exact(&mut key_len_bytes)?;
            let key_len = u64::from_le_bytes(key_len_bytes) as usize;

            let mut key_buf = vec![0u8; key_len];
            reader.read_exact(&mut key_buf)?;

            // Compare keys based on the type
            let comparison = self.compare_keys(&key_buf, key);

            match comparison {
                std::cmp::Ordering::Equal => {
                    // Found a match, read offsets
                    let mut offsets_len_bytes = [0u8; 8];
                    reader.read_exact(&mut offsets_len_bytes)?;
                    let offsets_len = u64::from_le_bytes(offsets_len_bytes) as usize;

                    for _ in 0..offsets_len {
                        let mut offset_bytes = [0u8; 8];
                        reader.read_exact(&mut offset_bytes)?;
                        let offset = u64::from_le_bytes(offset_bytes);
                        result.push(offset);
                    }
                    break;
                }
                std::cmp::Ordering::Less => {
                    left = mid + 1;
                }
                std::cmp::Ordering::Greater => {
                    right = mid - 1;
                }
            }
        }

        // Reset position
        reader.seek(SeekFrom::Start(start_pos))?;

        Ok(result)
    }

    /// Read and deserialize stream query range results.
    pub fn stream_query_range<R: Read + Seek>(
        &self,
        reader: &mut R,
        lower: Option<&[u8]>,
        upper: Option<&[u8]>,
    ) -> std::io::Result<Vec<ValueOffset>> {
        // Store current position to restore later
        let start_pos = reader.stream_position()?;

        // Find the starting position based on lower bound
        let start_index = if let Some(lower_bound) = lower {
            self.find_lower_bound(reader, lower_bound, start_pos)?
        } else {
            0
        };

        // Seek to the starting entry
        self.seek_to_entry(reader, start_index, start_pos)?;

        let mut result = Vec::new();

        // Iterate through entries until we hit the upper bound
        for i in start_index..self.entry_count {
            // Read key length
            let mut key_len_bytes = [0u8; 8];
            reader.read_exact(&mut key_len_bytes)?;
            let key_len = u64::from_le_bytes(key_len_bytes) as usize;

            // Read key bytes
            let mut key_buf = vec![0u8; key_len];
            reader.read_exact(&mut key_buf)?;

            // Check upper bound
            if let Some(upper_bound) = upper {
                if self.compare_keys(&key_buf, upper_bound) != std::cmp::Ordering::Less {
                    break;
                }
            }

            // Read offsets
            let mut offsets_len_bytes = [0u8; 8];
            reader.read_exact(&mut offsets_len_bytes)?;
            let offsets_len = u64::from_le_bytes(offsets_len_bytes) as usize;

            for _ in 0..offsets_len {
                let mut offset_bytes = [0u8; 8];
                reader.read_exact(&mut offset_bytes)?;
                let offset = u64::from_le_bytes(offset_bytes);
                result.push(offset);
            }
        }

        // Reset position
        reader.seek(SeekFrom::Start(start_pos))?;

        Ok(result)
    }

    /// Helper method to seek to a specific entry in the index.
    fn seek_to_entry<R: Read + Seek>(
        &self,
        reader: &mut R,
        entry_index: u64,
        start_pos: u64,
    ) -> std::io::Result<()> {
        // Reset to the beginning of the index
        reader.seek(SeekFrom::Start(start_pos))?;

        // Skip the type identifier and entry count
        reader.seek(SeekFrom::Current(12))?;

        // Iterate through entries until we reach the target
        for _ in 0..entry_index {
            // Read key length
            let mut key_len_bytes = [0u8; 8];
            reader.read_exact(&mut key_len_bytes)?;
            let key_len = u64::from_le_bytes(key_len_bytes) as usize;

            // Skip key bytes
            reader.seek(SeekFrom::Current(key_len as i64))?;

            // Read offsets length
            let mut offsets_len_bytes = [0u8; 8];
            reader.read_exact(&mut offsets_len_bytes)?;
            let offsets_len = u64::from_le_bytes(offsets_len_bytes) as usize;

            // Skip offset bytes
            reader.seek(SeekFrom::Current((offsets_len * 8) as i64))?;
        }

        Ok(())
    }

    /// Helper method to find the lower bound index for range queries.
    fn find_lower_bound<R: Read + Seek>(
        &self,
        reader: &mut R,
        lower_bound: &[u8],
        start_pos: u64,
    ) -> std::io::Result<u64> {
        // Binary search to find the lower bound
        let mut left = 0;
        let mut right = self.entry_count as i64 - 1;
        let mut result = 0;

        while left <= right {
            let mid = left + (right - left) / 2;

            // Seek to the mid entry
            self.seek_to_entry(reader, mid as u64, start_pos)?;

            // Read key length
            let mut key_len_bytes = [0u8; 8];
            reader.read_exact(&mut key_len_bytes)?;
            let key_len = u64::from_le_bytes(key_len_bytes) as usize;

            // Read key bytes
            let mut key_buf = vec![0u8; key_len];
            reader.read_exact(&mut key_buf)?;

            // Compare keys
            let ordering = self.compare_keys(&key_buf, lower_bound);

            match ordering {
                std::cmp::Ordering::Equal => {
                    result = mid as u64;
                    break;
                }
                std::cmp::Ordering::Less => {
                    left = mid + 1;
                    result = left as u64;
                }
                std::cmp::Ordering::Greater => {
                    right = mid - 1;
                }
            }
        }

        Ok(result)
    }

    // TODO: Fix me!!!!!!
    /// Helper method to compare keys based on the type identifier.
    fn compare_keys(&self, key_bytes: &[u8], query_key: &[u8]) -> std::cmp::Ordering {
        match self.type_id {
            ByteSerializableType::F32 => {
                // OrderedFloat<f32>
                if key_bytes.len() == 4 && query_key.len() == 4 {
                    let key_val = ordered_float::OrderedFloat(f32::from_le_bytes([
                        key_bytes[0],
                        key_bytes[1],
                        key_bytes[2],
                        key_bytes[3],
                    ]));
                    let query_val = ordered_float::OrderedFloat(f32::from_le_bytes([
                        query_key[0],
                        query_key[1],
                        query_key[2],
                        query_key[3],
                    ]));

                    key_val
                        .partial_cmp(&query_val)
                        .unwrap_or(std::cmp::Ordering::Equal)
                } else {
                    key_bytes.cmp(query_key)
                }
            }
            ByteSerializableType::F64 => {
                // OrderedFloat<f64>
                if key_bytes.len() == 8 && query_key.len() == 8 {
                    let key_val = ordered_float::OrderedFloat(f64::from_le_bytes([
                        key_bytes[0],
                        key_bytes[1],
                        key_bytes[2],
                        key_bytes[3],
                        key_bytes[4],
                        key_bytes[5],
                        key_bytes[6],
                        key_bytes[7],
                    ]));
                    let query_val = ordered_float::OrderedFloat(f64::from_le_bytes([
                        query_key[0],
                        query_key[1],
                        query_key[2],
                        query_key[3],
                        query_key[4],
                        query_key[5],
                        query_key[6],
                        query_key[7],
                    ]));

                    key_val
                        .partial_cmp(&query_val)
                        .unwrap_or(std::cmp::Ordering::Equal)
                } else {
                    key_bytes.cmp(query_key)
                }
            }
            ByteSerializableType::String => {
                // Try to convert to strings for comparison
                match (
                    std::str::from_utf8(key_bytes),
                    std::str::from_utf8(query_key),
                ) {
                    (Ok(key_str), Ok(query_str)) => key_str.cmp(query_str),
                    _ => key_bytes.cmp(query_key),
                }
            }
            ByteSerializableType::DateTime => {
                // DateTime<Utc>
                let key_val = DateTime::<Utc>::from_bytes(key_bytes);
                let query_val = DateTime::<Utc>::from_bytes(query_key);
                key_val.cmp(&query_val)
                // if key_bytes.len() == 8 && query_key.len() == 8 {
                //     let key_val = DateTime::<Utc>::from_bytes(key_bytes);
                //     let query_val = DateTime::<Utc>::from_bytes(query_key);

                //     key_val.cmp(&query_val)
                // } else {
                //     key_bytes.cmp(query_key)
                // }
            }

            // For all other types, we can directly compare the byte slices
            _ => key_bytes.cmp(query_key),
        }
    }
}

/// A multi-index that can be streamed from a reader.
#[derive(Default)]
pub struct StreamableMultiIndex {
    /// A mapping from field names to their corresponding index metadata.
    pub indices: HashMap<String, TypeErasedIndexMeta>,
    /// A mapping from field names to their offsets in the file.
    pub index_offsets: HashMap<String, u64>,
}

impl StreamableMultiIndex {
    /// Create a new, empty streamable multi-index.
    pub fn new() -> Self {
        Self {
            indices: HashMap::new(),
            index_offsets: HashMap::new(),
        }
    }

    /// Add an index for a field.
    pub fn add_index(&mut self, field_name: String, index: TypeErasedIndexMeta) {
        self.indices.insert(field_name, index);
    }

    /// Create a streamable multi-index from a reader.
    pub fn from_reader<R: Read + Seek>(
        reader: &mut R,
        index_offsets: &HashMap<String, u64>,
    ) -> Result<Self, error::Error> {
        println!("StreamableMultiIndex::from_reader - Starting");
        let mut multi_index = Self::new();

        // Copy the index offsets
        for (field, offset) in index_offsets {
            multi_index.index_offsets.insert(field.clone(), *offset);
        }

        // Get the type identifier and entry count for each index
        for (field, offset) in index_offsets {
            reader.seek(SeekFrom::Start(*offset))?;

            // Read the type ID
            let mut type_id_bytes = [0u8; 4];
            reader.read_exact(&mut type_id_bytes)?;
            let type_id = ByteSerializableType::from_type_id(u32::from_le_bytes(type_id_bytes))?;

            // Read the entry count
            let mut entry_count_bytes = [0u8; 8];
            reader.read_exact(&mut entry_count_bytes)?;
            let entry_count = u64::from_le_bytes(entry_count_bytes);

            // Get the size of the index by reading through all entries
            let start_pos = *offset;
            reader.seek(SeekFrom::Start(start_pos + 12))?; // Skip type ID and entry count

            let mut curr_pos = start_pos + 12;

            // For each entry, skip over the key and offsets
            for _ in 0..entry_count {
                // Read key length
                let mut key_len_bytes = [0u8; 8];
                reader.read_exact(&mut key_len_bytes)?;
                let key_len = u64::from_le_bytes(key_len_bytes);

                // Skip key bytes
                reader.seek(SeekFrom::Current(key_len as i64))?;

                // Read offsets length
                let mut offsets_len_bytes = [0u8; 8];
                reader.read_exact(&mut offsets_len_bytes)?;
                let offsets_len = u64::from_le_bytes(offsets_len_bytes);

                // Skip offset bytes
                reader.seek(SeekFrom::Current((offsets_len * 8) as i64))?;

                curr_pos = reader.stream_position()?;
            }

            // Calculate the size of the index
            let size = curr_pos - start_pos;

            // Create a type-erased index meta and add it to the multi-index
            let index_meta = TypeErasedIndexMeta {
                entry_count,
                size,
                type_id,
            };

            multi_index.add_index(field.clone(), index_meta);
        }

        Ok(multi_index)
    }

    /// Execute a query on the multi-index.
    pub fn stream_query<R: Read + Seek>(
        &self,
        reader: &mut R,
        query: &Query,
    ) -> Result<Vec<ValueOffset>, error::Error> {
        println!("StreamableMultiIndex::stream_query - Starting");

        // Save the current position to restore later
        let start_pos = reader.stream_position()?;

        // Process each condition and collect the results
        let mut all_results: Option<HashSet<ValueOffset>> = None;

        for condition in &query.conditions {
            println!("Processing condition: {:?}", condition);

            // Get the index for this field
            let index_meta = match self.indices.get(&condition.field) {
                Some(index) => index,
                None => {
                    println!("No index found for field: {}", condition.field);
                    continue;
                }
            };

            // Get the offset for this field
            let offset = match self.index_offsets.get(&condition.field) {
                Some(offset) => *offset,
                None => {
                    println!("No offset found for field: {}", condition.field);
                    continue;
                }
            };

            // Seek to the start of the index
            reader.seek(SeekFrom::Start(offset))?;

            // Execute the query based on the operator
            let results = match condition.operator {
                Operator::Eq => index_meta.stream_query_exact(reader, &condition.key)?,
                Operator::Ne => {
                    // For not equal, we need to get all results and filter out the matching ones
                    let matching = index_meta.stream_query_exact(reader, &condition.key)?;
                    let all = index_meta.stream_query_range(reader, None, None)?;
                    all.into_iter().filter(|v| !matching.contains(v)).collect()
                }
                Operator::Gt => {
                    // For greater than, we get the range but exclude exact matches
                    let range_results =
                        index_meta.stream_query_range(reader, Some(&condition.key), None)?;
                    let exact_matches = index_meta.stream_query_exact(reader, &condition.key)?;

                    // Filter out exact matches from range results
                    range_results
                        .into_iter()
                        .filter(|v| !exact_matches.contains(v))
                        .collect()
                }
                Operator::Lt => {
                    index_meta.stream_query_range(reader, None, Some(&condition.key))?
                }
                Operator::Ge => {
                    // For greater than or equal, we include the key
                    index_meta.stream_query_range(reader, Some(&condition.key), None)?
                }
                Operator::Le => {
                    // For less than or equal, we include the key
                    index_meta.stream_query_range(reader, None, Some(&condition.key))?
                }
            };

            println!("Condition results: {} matches", results.len());

            // Intersect with previous results
            match all_results {
                None => {
                    all_results = Some(results.into_iter().collect());
                }
                Some(ref mut existing) => {
                    let new_results: HashSet<ValueOffset> = results.into_iter().collect();
                    *existing = existing.intersection(&new_results).cloned().collect();
                }
            }
        }

        // Restore the original position
        reader.seek(SeekFrom::Start(start_pos))?;

        // Convert the results to a sorted vector
        let mut result_vec = match all_results {
            Some(set) => set.into_iter().collect::<Vec<_>>(),
            None => Vec::new(),
        };

        result_vec.sort();

        println!(
            "StreamableMultiIndex::stream_query - Completed with {} results",
            result_vec.len()
        );

        Ok(result_vec)
    }

    #[cfg(feature = "http")]
    pub async fn http_stream_query<T: AsyncHttpRangeClient>(
        &self,
        client: &mut AsyncBufferedHttpRangeClient<T>,
        query: &Query,
        index_offset: usize,
        feature_begin: usize,
    ) -> std::io::Result<Vec<HttpSearchResultItem>> {
        // TODO: Implement HTTP streaming query
        unimplemented!("HTTP streaming query not yet implemented for TypeErasedIndexMeta");
    }

    #[cfg(feature = "http")]
    pub async fn http_stream_query_batched<T: AsyncHttpRangeClient>(
        &self,
        client: &mut AsyncBufferedHttpRangeClient<T>,
        query: &Query,
        index_offset: usize,
        feature_begin: usize,
        batch_threshold: usize,
    ) -> std::io::Result<Vec<HttpSearchResultItem>> {
        // TODO: Implement batched HTTP streaming query
        unimplemented!("Batched HTTP streaming query not yet implemented for TypeErasedIndexMeta");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sorted_index::BufferedIndex;
    use crate::IndexSerializable;
    use crate::KeyValue;

    use ordered_float::OrderedFloat;
    use std::io::Cursor;

    fn create_sample_height_index() -> BufferedIndex<OrderedFloat<f32>> {
        let mut index = BufferedIndex::new();
        let entries = vec![
            KeyValue {
                key: OrderedFloat(10.5),
                offsets: vec![1, 2, 3],
            },
            KeyValue {
                key: OrderedFloat(20.0),
                offsets: vec![4, 5],
            },
            KeyValue {
                key: OrderedFloat(30.0),
                offsets: vec![6, 7, 8],
            },
            KeyValue {
                key: OrderedFloat(74.5),
                offsets: vec![9, 10],
            },
        ];
        index.build_index(entries);
        index
    }

    fn create_sample_id_index() -> BufferedIndex<String> {
        let mut index = BufferedIndex::new();
        let entries = vec![
            KeyValue {
                key: "a1".to_string(),
                offsets: vec![1, 2],
            },
            KeyValue {
                key: "b2".to_string(),
                offsets: vec![3, 4, 5],
            },
            KeyValue {
                key: "c3".to_string(),
                offsets: vec![6, 7],
            },
            KeyValue {
                key: "d4".to_string(),
                offsets: vec![8, 9, 10],
            },
        ];
        index.build_index(entries);
        index
    }

    fn create_serialized_height_index() -> Vec<u8> {
        let index = create_sample_height_index();
        let mut buffer = Vec::new();
        index.serialize(&mut buffer).unwrap();
        buffer
    }

    fn create_serialized_id_index() -> Vec<u8> {
        let index = create_sample_id_index();
        let mut buffer = Vec::new();
        index.serialize(&mut buffer).unwrap();
        buffer
    }

    #[test]
    fn test_streamable_multi_index_from_reader() -> Result<(), error::Error> {
        // Create serialized indices
        let height_buffer = create_serialized_height_index();
        let id_buffer = create_serialized_id_index();

        // Create a combined buffer with both indices
        let mut combined_buffer = Vec::new();
        combined_buffer.extend_from_slice(&height_buffer);
        combined_buffer.extend_from_slice(&id_buffer);

        // Create a cursor for the combined buffer
        let mut cursor = Cursor::new(&combined_buffer);

        // Create index offsets
        let mut index_offsets = HashMap::new();
        index_offsets.insert("height".to_string(), 0);
        index_offsets.insert("id".to_string(), height_buffer.len() as u64);

        // Create a streamable multi-index
        let multi_index = StreamableMultiIndex::from_reader(&mut cursor, &index_offsets)?;

        // Verify the indices were loaded correctly
        assert_eq!(multi_index.indices.len(), 2);
        assert!(multi_index.indices.contains_key("height"));
        assert!(multi_index.indices.contains_key("id"));

        // Verify the offsets were stored correctly
        assert_eq!(multi_index.index_offsets.len(), 2);
        assert_eq!(multi_index.index_offsets.get("height"), Some(&0));
        assert_eq!(
            multi_index.index_offsets.get("id"),
            Some(&(height_buffer.len() as u64))
        );

        // Verify the type IDs are correct
        assert_eq!(
            multi_index.indices.get("height").unwrap().type_id,
            ByteSerializableType::F32
        );
        assert_eq!(
            multi_index.indices.get("id").unwrap().type_id,
            ByteSerializableType::String
        );

        Ok(())
    }

    #[test]
    fn test_streamable_multi_index_queries() -> Result<(), error::Error> {
        // Create serialized indices
        let height_buffer = create_serialized_height_index();
        let id_buffer = create_serialized_id_index();

        // Create a combined buffer with both indices
        let mut combined_buffer = Vec::new();
        combined_buffer.extend_from_slice(&height_buffer);
        combined_buffer.extend_from_slice(&id_buffer);

        // Create a cursor for the combined buffer
        let mut cursor = Cursor::new(&combined_buffer);

        // Create index offsets
        let mut index_offsets = HashMap::new();
        index_offsets.insert("height".to_string(), 0);
        index_offsets.insert("id".to_string(), height_buffer.len() as u64);

        // Create a streamable multi-index
        let multi_index = StreamableMultiIndex::from_reader(&mut cursor, &index_offsets)?;

        // Define test cases
        struct TestCase {
            name: &'static str,
            query: Query,
            expected: Vec<u64>,
        }

        let test_cases = vec![
            TestCase {
                name: "Exact height match",
                query: Query {
                    conditions: vec![QueryCondition {
                        field: "height".to_string(),
                        operator: Operator::Eq,
                        key: OrderedFloat(30.0f32).to_bytes(),
                    }],
                },
                expected: vec![6, 7, 8],
            },
            TestCase {
                name: "Height range query",
                query: Query {
                    conditions: vec![QueryCondition {
                        field: "height".to_string(),
                        operator: Operator::Gt,
                        key: OrderedFloat(20.0f32).to_bytes(),
                    }],
                },
                expected: vec![6, 7, 8, 9, 10],
            },
            TestCase {
                name: "Exact ID match",
                query: Query {
                    conditions: vec![QueryCondition {
                        field: "id".to_string(),
                        operator: Operator::Eq,
                        key: "c3".to_string().to_bytes(),
                    }],
                },
                expected: vec![6, 7],
            },
            TestCase {
                name: "Combined query (height and ID)",
                query: Query {
                    conditions: vec![
                        QueryCondition {
                            field: "height".to_string(),
                            operator: Operator::Ge,
                            key: OrderedFloat(30.0f32).to_bytes(),
                        },
                        QueryCondition {
                            field: "id".to_string(),
                            operator: Operator::Eq,
                            key: "c3".to_string().to_bytes(),
                        },
                    ],
                },
                expected: vec![6, 7],
            },
        ];

        // Run the test cases
        for test_case in test_cases {
            println!("Running test case: {}", test_case.name);

            // Reset cursor position
            cursor.set_position(0);

            // Execute the query
            let results = multi_index.stream_query(&mut cursor, &test_case.query)?;

            // Verify the results
            assert_eq!(
                results, test_case.expected,
                "Test case '{}' failed: expected {:?}, got {:?}",
                test_case.name, test_case.expected, results
            );
        }

        Ok(())
    }
}
