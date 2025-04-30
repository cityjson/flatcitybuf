use crate::sorted_index::ValueOffset;
use crate::{error, ByteSerializable, ByteSerializableType};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::collections::HashSet;
use std::io::{Read, Seek, SeekFrom};

use super::{Operator, Query};

/// A trait for type-safe streaming access to an index.
pub trait TypedStreamableIndex<T: Ord + ByteSerializable + Send + Sync + 'static>:
    Send + Sync
{
    /// Returns the size of the index in bytes.
    fn index_size(&self) -> u64;

    /// Returns the offsets for an exact match given a key.
    /// The reader should be positioned at the start of the index data.
    fn stream_query_exact<R: Read + Seek>(
        &self,
        reader: &mut R,
        key: &T,
    ) -> std::io::Result<Vec<ValueOffset>>;

    /// Returns the offsets for a range query given optional lower and upper keys.
    /// The reader should be positioned at the start of the index data.
    fn stream_query_range<R: Read + Seek>(
        &self,
        reader: &mut R,
        lower: Option<&T>,
        upper: Option<&T>,
    ) -> std::io::Result<Vec<ValueOffset>>;
}

/// Metadata for a serialized BufferedIndex, used for streaming access.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexMeta<T: Ord + ByteSerializable + Send + Sync + 'static> {
    /// Number of entries in the index.
    pub entry_count: u64,
    /// Total size of the index in bytes.
    pub size: u64,
    /// Phantom data to represent the type parameter.
    pub _phantom: std::marker::PhantomData<T>,
}

impl<T: Ord + ByteSerializable + Send + Sync + 'static + std::fmt::Debug> IndexMeta<T> {
    /// Creates a new IndexMeta.
    pub fn new(entry_count: u64, size: u64) -> Self {
        Self {
            entry_count,
            size,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Read metadata and construct an IndexMeta from a reader.
    pub fn from_reader<R: Read + Seek>(reader: &mut R, size: u64) -> Result<Self, error::Error> {
        let start_pos = reader.stream_position()?;

        // Read the type identifier.
        let mut type_id_bytes = [0u8; 4];
        reader.read_exact(&mut type_id_bytes)?;

        // Read the number of entries.
        let mut entry_count_bytes = [0u8; 8];
        reader.read_exact(&mut entry_count_bytes)?;
        let entry_count = u64::from_le_bytes(entry_count_bytes);

        // Seek back to the start position.
        reader.seek(SeekFrom::Start(start_pos))?;

        Ok(Self::new(entry_count, size))
    }

    /// Seek to a specific entry in the index.
    pub fn seek_to_entry<R: Read + Seek>(
        &self,
        reader: &mut R,
        entry_index: u64,
        start_pos: u64,
    ) -> std::io::Result<()> {
        if entry_index >= self.entry_count {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "entry index {} out of bounds (max: {})",
                    entry_index,
                    self.entry_count - 1
                ),
            ));
        }

        // Skip the type id (4 bytes) and entry count (8 bytes).
        let pos = start_pos + 12;

        reader.seek(SeekFrom::Start(pos))?;

        // Skip entries until we reach the desired one.
        for _ in 0..entry_index {
            // Read the key length.
            let mut key_len_bytes = [0u8; 8];
            reader.read_exact(&mut key_len_bytes)?;
            let key_len = u64::from_le_bytes(key_len_bytes);

            // Skip the key.
            reader.seek(SeekFrom::Current(key_len as i64))?;

            // Read the offsets count.
            let mut offsets_count_bytes = [0u8; 8];
            reader.read_exact(&mut offsets_count_bytes)?;
            let offsets_count = u64::from_le_bytes(offsets_count_bytes);

            // Skip the offsets.
            reader.seek(SeekFrom::Current((offsets_count * 8) as i64))?;
        }

        Ok(())
    }

    /// Find the lower bound for a key using binary search.
    pub fn find_lower_bound<R: Read + Seek>(
        &self,
        reader: &mut R,
        key: &T,
        start_pos: u64,
    ) -> std::io::Result<u64> {
        if self.entry_count == 0 {
            return Ok(0);
        }

        let mut left = 0;
        let mut right = self.entry_count - 1;

        while left <= right {
            let mid = left + (right - left) / 2;
            self.seek_to_entry(reader, mid, start_pos)?;

            // Read the key length.
            let mut key_len_bytes = [0u8; 8];
            reader.read_exact(&mut key_len_bytes)?;
            let key_len = u64::from_le_bytes(key_len_bytes);

            // Read the key.
            let mut key_bytes = vec![0u8; key_len as usize];
            reader.read_exact(&mut key_bytes)?;

            // Deserialize the key and compare.
            let entry_key = T::from_bytes(&key_bytes);
            let ordering = entry_key.cmp(key);

            match ordering {
                std::cmp::Ordering::Equal => return Ok(mid),
                std::cmp::Ordering::Less => left = mid + 1,
                std::cmp::Ordering::Greater => {
                    if mid == 0 {
                        break;
                    }
                    right = mid - 1;
                }
            }
        }

        Ok(left)
    }

    /// Find the upper bound for a key using binary search.
    pub fn find_upper_bound<R: Read + Seek>(
        &self,
        reader: &mut R,
        key: &T,
        start_pos: u64,
    ) -> std::io::Result<u64> {
        if self.entry_count == 0 {
            return Ok(0);
        }

        let mut left = 0;
        let mut right = self.entry_count - 1;

        while left <= right {
            let mid = left + (right - left) / 2;
            self.seek_to_entry(reader, mid, start_pos)?;

            // Read the key length.
            let mut key_len_bytes = [0u8; 8];
            reader.read_exact(&mut key_len_bytes)?;
            let key_len = u64::from_le_bytes(key_len_bytes);

            // Read the key.
            let mut key_bytes = vec![0u8; key_len as usize];
            reader.read_exact(&mut key_bytes)?;

            // Deserialize the key and compare.
            let entry_key = T::from_bytes(&key_bytes);
            let ordering = entry_key.cmp(key);

            match ordering {
                std::cmp::Ordering::Equal | std::cmp::Ordering::Less => left = mid + 1,
                std::cmp::Ordering::Greater => {
                    if mid == 0 {
                        break;
                    }
                    right = mid - 1;
                }
            }
        }

        Ok(left)
    }

    /// Read the offsets for a specific entry.
    pub fn read_offsets<R: Read + Seek>(
        &self,
        reader: &mut R,
        entry_index: u64,
        start_pos: u64,
    ) -> std::io::Result<Vec<ValueOffset>> {
        self.seek_to_entry(reader, entry_index, start_pos)?;

        // Read the key length.
        let mut key_len_bytes = [0u8; 8];
        reader.read_exact(&mut key_len_bytes)?;
        let key_len = u64::from_le_bytes(key_len_bytes);

        // Skip the key.
        reader.seek(SeekFrom::Current(key_len as i64))?;

        // Read the offsets count.
        let mut offsets_count_bytes = [0u8; 8];
        reader.read_exact(&mut offsets_count_bytes)?;
        let offsets_count = u64::from_le_bytes(offsets_count_bytes);

        // Read the offsets.
        let mut offsets = Vec::with_capacity(offsets_count as usize);
        for _ in 0..offsets_count {
            let mut offset_bytes = [0u8; 8];
            reader.read_exact(&mut offset_bytes)?;
            offsets.push(u64::from_le_bytes(offset_bytes));
        }

        Ok(offsets)
    }
}

impl<T: Ord + ByteSerializable + Send + Sync + 'static + std::fmt::Debug> TypedStreamableIndex<T>
    for IndexMeta<T>
{
    fn index_size(&self) -> u64 {
        self.size
    }

    fn stream_query_exact<R: Read + Seek>(
        &self,
        reader: &mut R,
        key: &T,
    ) -> std::io::Result<Vec<ValueOffset>> {
        let start_pos = reader.stream_position()?;
        let index = self.find_lower_bound(reader, key, start_pos)?;

        if index >= self.entry_count {
            return Ok(Vec::new());
        }

        // Seek to the found entry.
        self.seek_to_entry(reader, index, start_pos)?;

        // Read the key length.
        let mut key_len_bytes = [0u8; 8];
        reader.read_exact(&mut key_len_bytes)?;
        let key_len = u64::from_le_bytes(key_len_bytes);

        // Read the key.
        let mut key_bytes = vec![0u8; key_len as usize];
        reader.read_exact(&mut key_bytes)?;

        // Deserialize the key and check for exact match.
        let entry_key = T::from_bytes(&key_bytes);

        if &entry_key == key {
            // Read the offsets count.
            let mut offsets_count_bytes = [0u8; 8];
            reader.read_exact(&mut offsets_count_bytes)?;
            let offsets_count = u64::from_le_bytes(offsets_count_bytes);

            // Read the offsets.
            let mut offsets = Vec::with_capacity(offsets_count as usize);
            for _ in 0..offsets_count {
                let mut offset_bytes = [0u8; 8];
                reader.read_exact(&mut offset_bytes)?;
                offsets.push(u64::from_le_bytes(offset_bytes));
            }

            return Ok(offsets);
        }

        Ok(Vec::new())
    }

    fn stream_query_range<R: Read + Seek>(
        &self,
        reader: &mut R,
        lower: Option<&T>,
        upper: Option<&T>,
    ) -> std::io::Result<Vec<ValueOffset>> {
        let start_pos = reader.stream_position()?;
        // Find lower bound.
        let start_index = if let Some(lower_key) = lower {
            self.find_lower_bound(reader, lower_key, start_pos)?
        } else {
            0
        };

        // Find upper bound.
        let end_index = if let Some(upper_key) = upper {
            self.find_upper_bound(reader, upper_key, start_pos)?
        } else {
            self.entry_count
        };

        if start_index >= end_index || start_index >= self.entry_count {
            return Ok(Vec::new());
        }

        let mut all_offsets = Vec::new();

        // Collect all offsets within the range.
        for entry_index in start_index..end_index.min(self.entry_count) {
            let offsets = self.read_offsets(reader, entry_index, start_pos)?;
            all_offsets.extend(offsets);
        }

        Ok(all_offsets)
    }
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
        index_meta: &IndexMeta<T>,
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
        // Save the current position to restore later
        let start_pos = reader.stream_position()?;

        // Process each condition and collect the results
        let mut all_results: Option<HashSet<ValueOffset>> = None;

        for condition in &query.conditions {
            // Get the index for this field
            let index_meta = match self.indices.get(&condition.field) {
                Some(index) => index,
                None => {
                    continue;
                }
            };

            // Get the offset for this field
            let offset = match self.index_offsets.get(&condition.field) {
                Some(offset) => *offset,
                None => {
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

        Ok(result_vec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sorted_index::BufferedIndex;
    use crate::Float;
    use crate::IndexSerializable;
    use crate::KeyValue;
    use crate::QueryCondition;
    use crate::TypedSearchableIndex;

    use chrono::NaiveDate;
    use chrono::NaiveDateTime;
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

    fn create_sample_date_index() -> BufferedIndex<NaiveDateTime> {
        let mut entries = Vec::new();
        let dates = [
            (NaiveDate::from_ymd(2020, 1, 1).and_hms(0, 0, 0), vec![0]),
            (NaiveDate::from_ymd(2020, 1, 2).and_hms(0, 0, 0), vec![1]),
            (NaiveDate::from_ymd(2020, 1, 3).and_hms(0, 0, 0), vec![2]),
            (NaiveDate::from_ymd(2020, 1, 4).and_hms(0, 0, 0), vec![3]),
            (NaiveDate::from_ymd(2020, 1, 5).and_hms(0, 0, 0), vec![4, 5]),
            (NaiveDate::from_ymd(2020, 1, 7).and_hms(0, 0, 0), vec![6]),
            (NaiveDate::from_ymd(2020, 1, 8).and_hms(0, 0, 0), vec![7]),
            (NaiveDate::from_ymd(2020, 1, 9).and_hms(0, 0, 0), vec![8]),
            (NaiveDate::from_ymd(2020, 1, 10).and_hms(0, 0, 0), vec![9]),
            (
                NaiveDate::from_ymd(2020, 1, 11).and_hms(0, 0, 0),
                vec![10, 11, 12],
            ),
            (NaiveDate::from_ymd(2020, 1, 14).and_hms(0, 0, 0), vec![13]),
            (NaiveDate::from_ymd(2020, 1, 15).and_hms(0, 0, 0), vec![14]),
            (NaiveDate::from_ymd(2020, 1, 16).and_hms(0, 0, 0), vec![15]),
            (NaiveDate::from_ymd(2020, 1, 17).and_hms(0, 0, 0), vec![16]),
            (NaiveDate::from_ymd(2020, 1, 18).and_hms(0, 0, 0), vec![17]),
            (NaiveDate::from_ymd(2020, 1, 19).and_hms(0, 0, 0), vec![18]),
            (NaiveDate::from_ymd(2020, 1, 20).and_hms(0, 0, 0), vec![19]),
        ];
        for (date, offsets) in dates.iter() {
            entries.push(KeyValue {
                key: *date,
                offsets: offsets.iter().map(|&i| i as u64).collect(),
            });
        }
        let mut index = BufferedIndex::new();
        index.build_index(entries);
        index
    }

    #[test]
    fn test_stream_query_exact_height() -> Result<(), error::Error> {
        // Create the index
        let index = create_sample_height_index();

        // Serialize to a temporary file
        let mut tmp_file = tempfile::NamedTempFile::new()?;
        index.serialize(&mut tmp_file)?;

        // Get the size of the serialized index
        let size = tmp_file.as_file().metadata()?.len();

        // Prepare for reading
        let mut file = tmp_file.reopen()?;
        file.seek(SeekFrom::Start(0))?;

        // Read the metadata
        let index_meta = IndexMeta::<Float<f32>>::from_reader(&mut file, size)?;

        // Reset position
        file.seek(SeekFrom::Start(0))?;

        // Perform streaming query
        let test_height = OrderedFloat(74.5);
        let stream_results = index_meta.stream_query_exact(&mut file, &test_height)?;

        // Also test with in-memory cursor
        let mut serialized = Vec::new();
        {
            let mut cursor = Cursor::new(&mut serialized);
            index.serialize(&mut cursor)?;
        }

        let mut cursor = Cursor::new(&serialized);
        let index_meta =
            IndexMeta::<Float<f32>>::from_reader(&mut cursor, serialized.len() as u64)?;

        cursor.set_position(0);
        let stream_results = index_meta.stream_query_exact(&mut cursor, &test_height)?;

        // Verify results
        let typed_results = index.query_exact(&test_height);
        assert_eq!(
            stream_results,
            typed_results.map(|v| v.to_vec()).unwrap_or_default()
        );

        Ok(())
    }

    #[test]
    fn test_stream_query_range_height() -> Result<(), error::Error> {
        // Create the index
        let index = create_sample_height_index();

        // Serialize to a temporary file
        let mut tmp_file = tempfile::NamedTempFile::new()?;
        index.serialize(&mut tmp_file)?;

        // Get the size of the serialized index
        let size = tmp_file.as_file().metadata()?.len();

        // Prepare for reading
        let mut file = tmp_file.reopen()?;
        file.seek(SeekFrom::Start(0))?;

        // Read the metadata
        let index_meta = IndexMeta::<Float<f32>>::from_reader(&mut file, size)?;

        // Reset position
        file.seek(SeekFrom::Start(0))?;

        // Define range query
        let lower = OrderedFloat(70.0);
        let upper = OrderedFloat(75.0);

        // Perform streaming query
        let stream_results =
            index_meta.stream_query_range(&mut file, Some(&lower), Some(&upper))?;

        // Also test with in-memory cursor
        let mut serialized = Vec::new();
        {
            let mut cursor = Cursor::new(&mut serialized);
            index.serialize(&mut cursor)?;
        }

        let mut cursor = Cursor::new(&serialized);
        let index_meta =
            IndexMeta::<Float<f32>>::from_reader(&mut cursor, serialized.len() as u64)?;

        cursor.set_position(0);
        let stream_results =
            index_meta.stream_query_range(&mut cursor, Some(&lower), Some(&upper))?;

        // Verify results match the typed query
        let typed_results = index.query_range(Some(&lower), Some(&upper));
        let typed_flat: Vec<ValueOffset> = typed_results.into_iter().flatten().cloned().collect();
        assert_eq!(stream_results, typed_flat);

        Ok(())
    }

    #[test]
    fn test_stream_query_exact_id() -> Result<(), error::Error> {
        // Create the index
        let index = create_sample_id_index();

        // Serialize to a temporary file
        let mut tmp_file = tempfile::NamedTempFile::new()?;
        index.serialize(&mut tmp_file)?;

        // Get the size of the serialized index
        let size = tmp_file.as_file().metadata()?.len();

        // Prepare for reading
        let mut file = tmp_file.reopen()?;
        file.seek(SeekFrom::Start(0))?;

        // Read the metadata
        let index_meta = IndexMeta::<String>::from_reader(&mut file, size)?;

        // Reset position
        file.seek(SeekFrom::Start(0))?;

        // Perform streaming query
        let test_id = "c3".to_string();
        let stream_results = index_meta.stream_query_exact(&mut file, &test_id)?;

        let typed_results = index.query_exact(&test_id);
        assert_eq!(
            stream_results,
            typed_results.map(|v| v.to_vec()).unwrap_or_default()
        );

        // Also test with in-memory cursor
        let mut serialized = Vec::new();
        {
            let mut cursor = Cursor::new(&mut serialized);
            index.serialize(&mut cursor)?;
        }

        let mut cursor = Cursor::new(&serialized);
        let index_meta = IndexMeta::<String>::from_reader(&mut cursor, serialized.len() as u64)?;

        cursor.set_position(0);
        let stream_results = index_meta.stream_query_exact(&mut cursor, &test_id)?;

        // Verify results
        assert_eq!(
            stream_results,
            typed_results.map(|v| v.to_vec()).unwrap_or_default()
        );

        Ok(())
    }

    #[test]
    fn test_stream_query_range_id() -> Result<(), error::Error> {
        // Create the index
        let index = create_sample_id_index();

        // Serialize to a temporary file
        let mut tmp_file = tempfile::NamedTempFile::new()?;
        index.serialize(&mut tmp_file)?;

        // Get the size of the serialized index
        let size = tmp_file.as_file().metadata()?.len();

        // Prepare for reading
        let mut file = tmp_file.reopen()?;
        file.seek(SeekFrom::Start(0))?;

        // Read the metadata
        let index_meta = IndexMeta::<String>::from_reader(&mut file, size)?;

        // Reset position
        file.seek(SeekFrom::Start(0))?;

        // Define range query
        let lower = "c1".to_string();
        let upper = "c4".to_string();

        // Perform streaming query
        let stream_results =
            index_meta.stream_query_range(&mut file, Some(&lower), Some(&upper))?;
        let typed_results = index.query_range(Some(&lower), Some(&upper));
        let typed_flat: Vec<ValueOffset> = typed_results.into_iter().flatten().cloned().collect();

        assert_eq!(stream_results, typed_flat);

        // Also test with in-memory cursor
        let mut serialized = Vec::new();
        {
            let mut cursor = Cursor::new(&mut serialized);
            index.serialize(&mut cursor)?;
        }

        let mut cursor = Cursor::new(&serialized);
        let index_meta = IndexMeta::<String>::from_reader(&mut cursor, serialized.len() as u64)?;

        cursor.set_position(0);
        let stream_results =
            index_meta.stream_query_range(&mut cursor, Some(&lower), Some(&upper))?;

        // Verify results match the typed query
        let typed_results = index.query_range(Some(&lower), Some(&upper));
        let typed_flat: Vec<ValueOffset> = typed_results.into_iter().flatten().cloned().collect();
        assert_eq!(stream_results, typed_flat);

        Ok(())
    }

    #[test]
    fn test_stream_query_range_date() -> Result<(), error::Error> {
        // Create the index
        let index = create_sample_date_index();

        // Serialize to a temporary file
        let mut tmp_file = tempfile::NamedTempFile::new()?;
        index.serialize(&mut tmp_file)?;

        // Get the size of the serialized index
        let size = tmp_file.as_file().metadata()?.len();

        // Prepare for reading
        let mut file = tmp_file.reopen()?;
        file.seek(SeekFrom::Start(0))?;

        // Read the metadata
        let index_meta = IndexMeta::<NaiveDateTime>::from_reader(&mut file, size)?;

        // Reset position
        file.seek(SeekFrom::Start(0))?;

        // Define range query
        let lower = NaiveDateTime::new(
            NaiveDate::from_ymd_opt(2022, 1, 1).unwrap(),
            chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
        );
        let upper = NaiveDateTime::new(
            NaiveDate::from_ymd_opt(2022, 2, 1).unwrap(),
            chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
        );

        // Perform streaming query
        let stream_results =
            index_meta.stream_query_range(&mut file, Some(&lower), Some(&upper))?;
        let typed_results = index.query_range(Some(&lower), Some(&upper));
        let typed_flat: Vec<ValueOffset> = typed_results.into_iter().flatten().cloned().collect();

        assert_eq!(stream_results, typed_flat);

        // Also test with in-memory cursor
        let mut serialized = Vec::new();
        {
            let mut cursor = Cursor::new(&mut serialized);
            index.serialize(&mut cursor)?;
        }

        let mut cursor = Cursor::new(&serialized);
        let index_meta =
            IndexMeta::<NaiveDateTime>::from_reader(&mut cursor, serialized.len() as u64)?;

        cursor.set_position(0);
        let stream_results =
            index_meta.stream_query_range(&mut cursor, Some(&lower), Some(&upper))?;

        // Verify results match the typed query
        let typed_results = index.query_range(Some(&lower), Some(&upper));
        let typed_flat: Vec<ValueOffset> = typed_results.into_iter().flatten().cloned().collect();
        assert_eq!(stream_results, typed_flat);

        Ok(())
    }

    #[test]
    fn test_performance_comparison() -> Result<(), error::Error> {
        // Create a sample height index
        let index = create_sample_height_index();

        // Serialize to buffer
        let mut buffer = Vec::new();
        index.serialize(&mut buffer)?;

        // Generate some test values
        let test_values = vec![30.0f32, 74.5, 100.0, 150.0, 200.0];

        // Measure direct query performance
        let direct_start = std::time::Instant::now();
        for &value in &test_values {
            let _results = index.query_exact(&OrderedFloat(value));
        }
        let direct_duration = direct_start.elapsed();

        // Measure streaming query performance
        let mut cursor = Cursor::new(buffer.clone());
        let index_meta = IndexMeta::<Float<f32>>::from_reader(&mut cursor, buffer.len() as u64)?;

        let stream_start = std::time::Instant::now();
        for &value in &test_values {
            let test_height = OrderedFloat(value);
            cursor.seek(SeekFrom::Start(0))?;
            let _results = index_meta.stream_query_exact(&mut cursor, &test_height)?;
        }
        let stream_duration = stream_start.elapsed();

        println!(
            "Performance comparison:\n\
           Direct query: {:?}\n\
           Stream query: {:?}\n\
           Ratio: {:.2}x",
            direct_duration,
            stream_duration,
            stream_duration.as_secs_f64() / direct_duration.as_secs_f64()
        );

        Ok(())
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
