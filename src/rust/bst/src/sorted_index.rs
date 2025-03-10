use std::io::{Read, Seek, SeekFrom, Write};

use ordered_float::OrderedFloat;

use crate::{
    byte_serializable::ByteSerializable,
    error, ByteSerializableType,
};

/// The offset type used to point to actual record data.
pub type ValueOffset = u64;

/// A key–offset pair. The key must be orderable and serializable.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct KeyValue<T: Ord + ByteSerializable + Send + Sync + 'static> {
    pub key: T,
    pub offsets: Vec<ValueOffset>,
}

/// A buffered index implemented as an in-memory array of key–offset pairs.
///
/// This index is fully loaded into memory for fast access, making it suitable
/// for smaller datasets or when memory usage is not a concern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferedIndex<T: Ord + ByteSerializable + Send + Sync + 'static> {
    pub entries: Vec<KeyValue<T>>,
}

impl<T: Ord + ByteSerializable + Send + Sync + 'static> Default for BufferedIndex<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Ord + ByteSerializable + Send + Sync + 'static> BufferedIndex<T> {
    /// Create an empty index.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Build the index from unsorted data.
    pub fn build_index(&mut self, mut data: Vec<KeyValue<T>>) {
        data.sort_by(|a, b| a.key.cmp(&b.key));
        self.entries = data;
    }
}

/// A trait defining byte-based search operations on an index.
///
/// This trait is object-safe and works with serialized byte representations
/// of keys, making it suitable for use with trait objects and dynamic dispatch.
pub trait SearchableIndex: Send + Sync {
    /// Return offsets for an exact key match given a serialized key.
    fn query_exact_bytes(&self, key: &[u8]) -> Vec<ValueOffset>;

    /// Return offsets for keys in the half-open interval [lower, upper) given serialized keys.
    /// (A `None` for either bound means unbounded.)
    fn query_range_bytes(&self, lower: Option<&[u8]>, upper: Option<&[u8]>) -> Vec<ValueOffset>;
}

/// A trait defining type-specific search operations on an index.
///
/// This trait provides strongly-typed search methods that work with the actual
/// key type rather than byte representations.
pub trait TypedSearchableIndex<T: Ord + ByteSerializable> {
    /// Return offsets for an exact key match.
    fn query_exact(&self, key: &T) -> Option<&[ValueOffset]>;

    /// Return offsets for keys in the half-open interval [lower, upper).
    /// (A `None` for either bound means unbounded.)
    fn query_range(&self, lower: Option<&T>, upper: Option<&T>) -> Vec<&[ValueOffset]>;

    /// Return offsets for which the key satisfies the given predicate.
    fn query_filter<F>(&self, predicate: F) -> Vec<&[ValueOffset]>
    where
        F: Fn(&T) -> bool;
}

impl<T: Ord + ByteSerializable + 'static + Send + Sync> TypedSearchableIndex<T>
    for BufferedIndex<T>
{
    fn query_exact(&self, key: &T) -> Option<&[ValueOffset]> {
        self.entries
            .binary_search_by_key(&key, |kv| &kv.key)
            .ok()
            .map(|i| self.entries[i].offsets.as_slice())
    }

    fn query_range(&self, lower: Option<&T>, upper: Option<&T>) -> Vec<&[ValueOffset]> {
        let mut results = Vec::new();
        let start_index = if let Some(lower_bound) = lower {
            match self
                .entries
                .binary_search_by_key(&lower_bound, |kv| &kv.key)
            {
                Ok(index) => index,
                Err(index) => index,
            }
        } else {
            0
        };

        for kv in self.entries.iter().skip(start_index) {
            if let Some(upper_bound) = upper {
                if &kv.key >= upper_bound {
                    break;
                }
            }
            results.push(kv.offsets.as_slice());
        }
        results
    }

    fn query_filter<F>(&self, predicate: F) -> Vec<&[ValueOffset]>
    where
        F: Fn(&T) -> bool,
    {
        self.entries
            .iter()
            .filter(|kv| predicate(&kv.key))
            .map(|kv| kv.offsets.as_slice())
            .collect()
    }
}

impl<T: Ord + ByteSerializable + 'static + Send + Sync> SearchableIndex for BufferedIndex<T> {
    fn query_exact_bytes(&self, key: &[u8]) -> Vec<ValueOffset> {
        let key_t = T::from_bytes(key);
        self.query_exact(&key_t).unwrap_or(&[]).to_vec()
    }

    fn query_range_bytes(&self, lower: Option<&[u8]>, upper: Option<&[u8]>) -> Vec<ValueOffset> {
        // Convert the optional byte slices into T
        let lower_t = lower.map(|b| T::from_bytes(b));
        let upper_t = upper.map(|b| T::from_bytes(b));
        // We need to pass references.
        let lower_ref = lower_t.as_ref();
        let upper_ref = upper_t.as_ref();
        let results = self.query_range(lower_ref, upper_ref);
        results.into_iter().flatten().cloned().collect()
    }
}

/// A trait for serializing and deserializing an index.
pub trait IndexSerializable {
    /// Write the index to a writer.
    fn serialize<W: Write>(&self, writer: &mut W) -> Result<(), error::Error>;

    /// Read the index from a reader.
    fn deserialize<R: Read>(reader: &mut R) -> Result<Self, error::Error>
    where
        Self: Sized;
}

impl<T: Ord + ByteSerializable + Send + Sync + 'static> IndexSerializable for BufferedIndex<T> {
    fn serialize<W: Write>(&self, writer: &mut W) -> Result<(), error::Error> {
        // Write the type identifier for T
        let value_type = self.entries.first().unwrap().key.value_type();
        writer.write_all(&value_type.to_bytes())?;

        // Write the number of entries
        let len = self.entries.len() as u64;
        writer.write_all(&len.to_le_bytes())?;

        for kv in &self.entries {
            let key_bytes = kv.key.to_bytes();
            let key_len = key_bytes.len() as u64;
            writer.write_all(&key_len.to_le_bytes())?;
            writer.write_all(&key_bytes)?;
            let offsets_len = kv.offsets.len() as u64;
            writer.write_all(&offsets_len.to_le_bytes())?;
            for offset in &kv.offsets {
                writer.write_all(&offset.to_le_bytes())?;
            }
        }
        Ok(())
    }

    fn deserialize<R: Read>(reader: &mut R) -> Result<Self, error::Error> {
        // Read the type identifier
        let mut type_id_bytes = [0u8; 4];
        reader.read_exact(&mut type_id_bytes)?;
        let _ = ByteSerializableType::from_bytes(&type_id_bytes)?;

        // Read the number of entries
        let mut len_bytes = [0u8; 8];
        reader.read_exact(&mut len_bytes)?;
        let num_entries = u64::from_le_bytes(len_bytes);

        let mut entries = Vec::with_capacity(num_entries as usize);
        for _ in 0..num_entries {
            // Read key length.
            let mut key_len_bytes = [0u8; 8];
            reader.read_exact(&mut key_len_bytes)?;
            let key_len = u64::from_le_bytes(key_len_bytes) as usize;
            // Read key bytes.
            let mut key_buf = vec![0u8; key_len];
            reader.read_exact(&mut key_buf)?;
            let key = T::from_bytes(&key_buf);

            // Read the number of offsets.
            let mut offsets_len_bytes = [0u8; 8];
            reader.read_exact(&mut offsets_len_bytes)?;
            let offsets_len = u64::from_le_bytes(offsets_len_bytes) as usize;
            let mut offsets = Vec::with_capacity(offsets_len);
            for _ in 0..offsets_len {
                let mut offset_bytes = [0u8; 8];
                reader.read_exact(&mut offset_bytes)?;
                let offset = u64::from_le_bytes(offset_bytes);
                offsets.push(offset);
            }
            entries.push(KeyValue { key, offsets });
        }
        Ok(BufferedIndex { entries })
    }
}

/// A trait for streaming access to index data without loading the entire index into memory.
pub trait StreamableIndex: Send + Sync {
    /// Returns the size of the index in bytes.
    fn index_size(&self) -> u64;

    /// Returns the offsets for an exact match given a serialized key.
    /// The reader should be positioned at the start of the index data.
    fn stream_query_exact<R: Read + Seek>(
        &self,
        reader: &mut R,
        key: &[u8],
    ) -> std::io::Result<Vec<ValueOffset>>;

    /// Returns the offsets for a range query given optional lower and upper serialized keys.
    /// The reader should be positioned at the start of the index data.
    fn stream_query_range<R: Read + Seek>(
        &self,
        reader: &mut R,
        lower: Option<&[u8]>,
        upper: Option<&[u8]>,
    ) -> std::io::Result<Vec<ValueOffset>>;

    /// Returns the offsets for an exact match given a serialized key.
    /// For use with HTTP range requests.
    #[cfg(feature = "http")]
    async fn http_stream_query_exact<C: http_range_client::AsyncHttpRangeClient>(
        &self,
        client: &mut http_range_client::AsyncBufferedHttpRangeClient<C>,
        index_offset: usize,
        key: &[u8],
    ) -> std::io::Result<Vec<ValueOffset>>;

    /// Returns the offsets for a range query given optional lower and upper serialized keys.
    /// For use with HTTP range requests.
    #[cfg(feature = "http")]
    async fn http_stream_query_range<C: http_range_client::AsyncHttpRangeClient>(
        &self,
        client: &mut http_range_client::AsyncBufferedHttpRangeClient<C>,
        index_offset: usize,
        lower: Option<&[u8]>,
        upper: Option<&[u8]>,
    ) -> std::io::Result<Vec<ValueOffset>>;
}

/// Metadata for a serialized BufferedIndex, used for streaming access.
pub struct IndexMeta {
    /// Number of entries in the index.
    pub entry_count: u64,
    /// Total size of the index in bytes.
    pub size: u64,
    /// Type identifier for the index.
    pub type_id: ByteSerializableType,
}

impl IndexMeta {
    /// Read metadata from a reader positioned at the start of a serialized BufferedIndex.
    ///
    /// The type parameter T is used to verify that the serialized type matches the expected type.
    pub fn from_reader<R: Read + Seek>(reader: &mut R, size: u64) -> Result<Self, error::Error> {
        let start_pos = reader.stream_position()?;

        // Read type identifier
        let mut type_id_bytes = [0u8; 4];
        reader.read_exact(&mut type_id_bytes)?;
        let type_id = ByteSerializableType::from_bytes(&type_id_bytes)?;

        // Read entry count
        let mut len_bytes = [0u8; 8];
        reader.read_exact(&mut len_bytes)?;
        let entry_count = u64::from_le_bytes(len_bytes);

        // Reset position
        reader.seek(SeekFrom::Start(start_pos))?;

        Ok(IndexMeta {
            entry_count,
            size,
            type_id,
        })
    }

    /// Helper method to seek to a specific entry in the index.
    pub fn seek_to_entry<R: Read + Seek>(
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
    pub fn find_lower_bound<R: Read + Seek>(
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

    /// Helper method to compare keys based on the type identifier.
    pub fn compare_keys(&self, key_bytes: &[u8], query_key: &[u8]) -> std::cmp::Ordering {
        println!("compare_keys: type_id={:?}", self.type_id);
        println!("key_bytes: {:?}", key_bytes);
        println!("query_key: {:?}", query_key);

        match self.type_id {
            ByteSerializableType::F32 => {
                // OrderedFloat<f32>
                println!("Comparing as F32");
                if key_bytes.len() == 4 && query_key.len() == 4 {
                    let key_val = OrderedFloat(f32::from_le_bytes([
                        key_bytes[0],
                        key_bytes[1],
                        key_bytes[2],
                        key_bytes[3],
                    ]));
                    let query_val = OrderedFloat(f32::from_le_bytes([
                        query_key[0],
                        query_key[1],
                        query_key[2],
                        query_key[3],
                    ]));

                    println!("key_val: {}", key_val);
                    println!("query_val: {}", query_val);
                    let result = key_val
                        .partial_cmp(&query_val)
                        .unwrap_or(std::cmp::Ordering::Equal);
                    println!("F32 comparison result: {:?}", result);
                    result
                } else {
                    println!("F32 byte length mismatch, falling back to byte comparison");
                    let result = key_bytes.cmp(query_key);
                    println!("Byte comparison result: {:?}", result);
                    result
                }
            }
            ByteSerializableType::F64 => {
                // OrderedFloat<f64>
                println!("Comparing as F64");
                if key_bytes.len() == 8 && query_key.len() == 8 {
                    let key_val = OrderedFloat(f64::from_le_bytes([
                        key_bytes[0],
                        key_bytes[1],
                        key_bytes[2],
                        key_bytes[3],
                        key_bytes[4],
                        key_bytes[5],
                        key_bytes[6],
                        key_bytes[7],
                    ]));
                    let query_val = OrderedFloat(f64::from_le_bytes([
                        query_key[0],
                        query_key[1],
                        query_key[2],
                        query_key[3],
                        query_key[4],
                        query_key[5],
                        query_key[6],
                        query_key[7],
                    ]));

                    println!("key_val: {}", key_val);
                    println!("query_val: {}", query_val);
                    let result = key_val
                        .partial_cmp(&query_val)
                        .unwrap_or(std::cmp::Ordering::Equal);
                    println!("F64 comparison result: {:?}", result);
                    result
                } else {
                    println!("F64 byte length mismatch, falling back to byte comparison");
                    let result = key_bytes.cmp(query_key);
                    println!("Byte comparison result: {:?}", result);
                    result
                }
            }
            ByteSerializableType::String => {
                // Try to convert to strings for comparison
                println!("Comparing as String");
                match (
                    std::str::from_utf8(key_bytes),
                    std::str::from_utf8(query_key),
                ) {
                    (Ok(key_str), Ok(query_str)) => {
                        println!("key_str: {}", key_str);
                        println!("query_str: {}", query_str);
                        let result = key_str.cmp(query_str);
                        println!("String comparison result: {:?}", result);
                        result
                    }
                    _ => {
                        println!("String conversion failed, falling back to byte comparison");
                        let result = key_bytes.cmp(query_key);
                        println!("Byte comparison result: {:?}", result);
                        result
                    }
                }
            }
            // For all other types, we can directly compare the byte slices
            _ => {
                println!("Comparing as raw bytes");
                let result = key_bytes.cmp(query_key);
                println!("Byte comparison result: {:?}", result);
                result
            }
        }
    }
}

impl StreamableIndex for IndexMeta {
    fn index_size(&self) -> u64 {
        self.size
    }

    fn stream_query_exact<R: Read + Seek>(
        &self,
        reader: &mut R,
        key: &[u8],
    ) -> std::io::Result<Vec<ValueOffset>> {
        println!(
            "Cursor position in stream_query_exact: {:?}",
            reader.stream_position()?
        );

        // Save the current position
        let start_pos = reader.stream_position()?;
        println!(
            "stream_query_exact: type_id={:?}, start_pos={}",
            self.type_id, start_pos
        );

        // Skip the type identifier and entry count
        reader.seek(SeekFrom::Current(12))?;

        // Binary search through the index
        let mut left = 0;
        let mut right = self.entry_count as i64 - 1;
        let mut result = Vec::new();

        println!("Binary search range: left={}, right={}", left, right);
        println!("Looking for key bytes: {:?}", key);

        while left <= right {
            let mid = left + (right - left) / 2;
            println!("Checking mid={}", mid);

            // Seek to the mid entry
            self.seek_to_entry(reader, mid as u64, start_pos)?;

            // Read key length
            let mut key_len_bytes = [0u8; 8];
            reader.read_exact(&mut key_len_bytes)?;
            let key_len = u64::from_le_bytes(key_len_bytes) as usize;
            println!("Key length: {}", key_len);

            // Read key bytes
            let mut key_buf = vec![0u8; key_len];
            reader.read_exact(&mut key_buf)?;
            println!("Read key bytes: {:?}", key_buf);

            // For debugging, try to convert keys to strings if possible
            let key_str = match std::str::from_utf8(&key_buf) {
                Ok(s) => s.to_string(),
                Err(_) => format!("{:?}", key_buf),
            };

            let query_str = match std::str::from_utf8(key) {
                Ok(s) => s.to_string(),
                Err(_) => format!("{:?}", key),
            };

            println!("Key as string: {}", key_str);
            println!("Query as string: {}", query_str);

            // Compare keys
            let comparison = self.compare_keys(&key_buf, key);
            println!("Comparison result: {:?}", comparison);

            match comparison {
                std::cmp::Ordering::Equal => {
                    // Found a match, read offsets
                    println!("Found exact match!");
                    let mut offsets_len_bytes = [0u8; 8];
                    reader.read_exact(&mut offsets_len_bytes)?;
                    let offsets_len = u64::from_le_bytes(offsets_len_bytes) as usize;
                    println!("Number of offsets: {}", offsets_len);

                    for i in 0..offsets_len {
                        let mut offset_bytes = [0u8; 8];
                        reader.read_exact(&mut offset_bytes)?;
                        let offset = u64::from_le_bytes(offset_bytes);
                        println!("Offset {}: {}", i, offset);
                        result.push(offset);
                    }
                    break;
                }
                std::cmp::Ordering::Less => {
                    println!("Key is less than query, moving left bound to {}", mid + 1);
                    left = mid + 1;
                }
                std::cmp::Ordering::Greater => {
                    println!(
                        "Key is greater than query, moving right bound to {}",
                        mid - 1
                    );
                    right = mid - 1;
                }
            }
        }

        // Reset position
        reader.seek(SeekFrom::Start(start_pos))?;
        println!("Final result: {:?}", result);

        Ok(result)
    }

    fn stream_query_range<R: Read + Seek>(
        &self,
        reader: &mut R,
        lower: Option<&[u8]>,
        upper: Option<&[u8]>,
    ) -> std::io::Result<Vec<ValueOffset>> {
        // Save the current position
        let start_pos = reader.stream_position()?;

        // Skip the type identifier and entry count
        reader.seek(SeekFrom::Current(12))?;

        let mut result = Vec::new();

        // Find the starting position based on lower bound
        let start_index = if let Some(lower_bound) = lower {
            self.find_lower_bound(reader, lower_bound, start_pos)?
        } else {
            0
        };

        // Seek to the starting entry
        self.seek_to_entry(reader, start_index, start_pos)?;

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

    async fn http_stream_query_exact<T: http_range_client::AsyncHttpRangeClient>(
        &self,
        client: &mut http_range_client::AsyncBufferedHttpRangeClient<T>,
        index_offset: usize,
        key: &[u8],
    ) -> std::io::Result<Vec<ValueOffset>> {
        todo!()
    }

    async fn http_stream_query_range<T: http_range_client::AsyncHttpRangeClient>(
        &self,
        client: &mut http_range_client::AsyncBufferedHttpRangeClient<T>,
        index_offset: usize,
        lower: Option<&[u8]>,
        upper: Option<&[u8]>,
    ) -> std::io::Result<Vec<ValueOffset>> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ordered_float::OrderedFloat;
    use std::io::{Cursor, Seek, SeekFrom};

    

    // Helper function to create a sample height index
    fn create_sample_height_index() -> BufferedIndex<OrderedFloat<f32>> {
        let mut entries = Vec::new();

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

        let mut index = BufferedIndex::new();
        index.build_index(entries);
        index
    }

    // Helper function to create a sample building ID index
    fn create_sample_id_index() -> BufferedIndex<String> {
        let mut entries = Vec::new();

        // Create sample data with building IDs
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

        let mut index = BufferedIndex::new();
        index.build_index(entries);
        index
    }

    #[test]
    fn test_stream_query_exact_height() -> Result<(), error::Error> {
        // Create a sample height index
        let height_index = create_sample_height_index();

        // Serialize the index to a buffer
        let mut buffer = Vec::new();
        height_index.serialize(&mut buffer)?;
        let buffer_size = buffer.len() as u64;

        // Create a cursor for the buffer
        let mut cursor = Cursor::new(buffer);

        // Read the index metadata
        let index_meta = IndexMeta::from_reader(&mut cursor, buffer_size)?;
        println!(
            "Height index metadata: {} entries, {} bytes",
            index_meta.entry_count, index_meta.size
        );

        // Test exact query for height 30.0
        let test_height = OrderedFloat(30.0f32);
        let height_bytes = test_height.to_bytes();

        // Reset cursor position
        cursor.seek(SeekFrom::Start(0))?;

        // Perform streaming query
        let stream_results = index_meta.stream_query_exact(&mut cursor, &height_bytes)?;
        println!(
            "Stream query found {} results for height {}",
            stream_results.len(),
            test_height
        );

        // Get expected results from the original index
        let expected_results = height_index.query_exact_bytes(&height_bytes);
        println!(
            "Expected {} results for height {}",
            expected_results.len(),
            test_height
        );

        // Compare results
        assert_eq!(
            stream_results.len(),
            expected_results.len(),
            "Result count mismatch"
        );
        assert_eq!(stream_results, expected_results, "Results don't match");

        // Verify the actual values
        assert_eq!(
            stream_results,
            vec![6, 7, 8],
            "Expected buildings 6, 7, 8 to have height 30.0"
        );

        Ok(())
    }

    #[test]
    fn test_stream_query_range_height() -> Result<(), error::Error> {
        // Create a sample height index
        let height_index = create_sample_height_index();

        // Serialize the index to a buffer
        let mut buffer = Vec::new();
        height_index.serialize(&mut buffer)?;
        let buffer_size = buffer.len() as u64;

        // Create a cursor for the buffer
        let mut cursor = Cursor::new(buffer);

        // Read the index metadata
        let index_meta = IndexMeta::from_reader(&mut cursor, buffer_size)?;

        // Test range query for heights between 25.0 and 40.0
        let lower_bound = OrderedFloat(25.0f32);
        let upper_bound = OrderedFloat(40.0f32);
        let lower_bytes = lower_bound.to_bytes();
        let upper_bytes = upper_bound.to_bytes();

        // Reset cursor position
        cursor.seek(SeekFrom::Start(0))?;

        // Perform streaming range query
        let stream_results =
            index_meta.stream_query_range(&mut cursor, Some(&lower_bytes), Some(&upper_bytes))?;
        println!(
            "Stream range query found {} results for heights between {} and {}",
            stream_results.len(),
            lower_bound,
            upper_bound
        );

        // Get expected results from the original index
        let expected_results =
            height_index.query_range_bytes(Some(&lower_bytes), Some(&upper_bytes));
        println!(
            "Expected {} results for heights between {} and {}",
            expected_results.len(),
            lower_bound,
            upper_bound
        );

        // Sort both result sets for comparison
        let mut stream_sorted = stream_results.clone();
        stream_sorted.sort();
        let mut expected_sorted = expected_results.clone();
        expected_sorted.sort();

        // Compare results
        assert_eq!(
            stream_sorted.len(),
            expected_sorted.len(),
            "Result count mismatch"
        );
        assert_eq!(stream_sorted, expected_sorted, "Results don't match");

        let expected_buildings = vec![5, 6, 7, 8, 9, 10];
        assert_eq!(
            stream_sorted, expected_buildings,
            "Expected buildings with heights between 25.0 and 40.0"
        );

        Ok(())
    }

    #[test]
    fn test_stream_query_exact_id() -> Result<(), error::Error> {
        // Create a sample ID index
        let id_index = create_sample_id_index();

        // Serialize the index to a buffer
        let mut buffer = Vec::new();
        id_index.serialize(&mut buffer)?;
        let buffer_size = buffer.len() as u64;

        // Create a cursor for the buffer
        let mut cursor = Cursor::new(buffer);

        // Read the index metadata
        let index_meta = IndexMeta::from_reader(&mut cursor, buffer_size)?;
        println!(
            "ID index metadata: {} entries, {} bytes",
            index_meta.entry_count, index_meta.size
        );

        // Test exact query for ID "BLDG0020"
        let test_id = "BLDG0020";
        let id_bytes = test_id.as_bytes();

        // Reset cursor position
        cursor.seek(SeekFrom::Start(0))?;

        // Perform streaming query
        let stream_results = index_meta.stream_query_exact(&mut cursor, id_bytes)?;
        println!(
            "Stream query found {} results for ID {}",
            stream_results.len(),
            test_id
        );

        // Get expected results from the original index
        let expected_results = id_index.query_exact_bytes(id_bytes);
        println!(
            "Expected {} results for ID {}",
            expected_results.len(),
            test_id
        );

        // Compare results
        assert_eq!(
            stream_results.len(),
            expected_results.len(),
            "Result count mismatch"
        );
        assert_eq!(stream_results, expected_results, "Results don't match");

        // Verify the actual values
        assert_eq!(
            stream_results,
            vec![8, 9, 10],
            "Expected buildings 8, 9, 10 to have ID BLDG0020"
        );

        Ok(())
    }

    #[test]
    fn test_stream_query_range_id() -> Result<(), error::Error> {
        // Create a sample ID index
        let id_index = create_sample_id_index();

        // Serialize the index to a buffer
        let mut buffer = Vec::new();
        id_index.serialize(&mut buffer)?;
        let buffer_size = buffer.len() as u64;

        // Create a cursor for the buffer
        let mut cursor = Cursor::new(buffer);

        // Read the index metadata
        let index_meta = IndexMeta::from_reader(&mut cursor, buffer_size)?;

        // Test range query for IDs between "BLDG0020" and "BLDG0050"
        let lower_bound = "BLDG0020";
        let upper_bound = "BLDG0050";
        let lower_bytes = lower_bound.as_bytes();
        let upper_bytes = upper_bound.as_bytes();

        // Reset cursor position
        cursor.seek(SeekFrom::Start(0))?;

        // Perform streaming range query
        let stream_results =
            index_meta.stream_query_range(&mut cursor, Some(lower_bytes), Some(upper_bytes))?;
        println!(
            "Stream range query found {} results for IDs between {} and {}",
            stream_results.len(),
            lower_bound,
            upper_bound
        );

        // Get expected results from the original index
        let expected_results = id_index.query_range_bytes(Some(lower_bytes), Some(upper_bytes));
        println!(
            "Expected {} results for IDs between {} and {}",
            expected_results.len(),
            lower_bound,
            upper_bound
        );

        // Sort both result sets for comparison
        let mut stream_sorted = stream_results.clone();
        stream_sorted.sort();
        let mut expected_sorted = expected_results.clone();
        expected_sorted.sort();

        // Compare results
        assert_eq!(
            stream_sorted.len(),
            expected_sorted.len(),
            "Result count mismatch"
        );
        assert_eq!(stream_sorted, expected_sorted, "Results don't match");

        // Verify we got the expected buildings (8-15)
        let expected_buildings = vec![8, 9, 10, 11, 12, 13, 14, 15];
        assert_eq!(
            stream_sorted, expected_buildings,
            "Expected buildings with IDs between BLDG0020 and BLDG0050"
        );

        Ok(())
    }

    #[test]
    fn test_performance_comparison() -> Result<(), error::Error> {
        // Create a larger sample index for performance testing
        let mut entries = Vec::new();

        // Create 10,000 entries with random heights
        for i in 0..10000 {
            let height = (i as f32) / 100.0; // Heights from 0.0 to 99.99
            entries.push(KeyValue {
                key: OrderedFloat(height),
                offsets: vec![i as u64],
            });
        }

        let mut large_index = BufferedIndex::new();
        large_index.build_index(entries);

        // Serialize the index to a buffer
        let mut buffer = Vec::new();
        large_index.serialize(&mut buffer)?;
        println!("Serialized index size: {} bytes", buffer.len());

        // Test values to query
        let test_values = [10.5f32, 25.75f32, 50.25f32, 75.8f32, 99.99f32];

        // Measure streaming query performance
        let mut cursor = Cursor::new(buffer.clone());
        let index_meta = IndexMeta::from_reader(&mut cursor, buffer.len() as u64)?;

        let stream_start = std::time::Instant::now();
        for &value in &test_values {
            let key_bytes = OrderedFloat(value).to_bytes();
            cursor.seek(SeekFrom::Start(0))?;
            let _results = index_meta.stream_query_exact(&mut cursor, &key_bytes)?;
        }
        let stream_duration = stream_start.elapsed();
        println!(
            "Stream query took {:?} for {} queries",
            stream_duration,
            test_values.len()
        );

        // Measure in-memory query performance (including deserialization)
        let in_memory_start = std::time::Instant::now();
        let mut cursor = Cursor::new(buffer);
        let in_memory_index = BufferedIndex::<OrderedFloat<f32>>::deserialize(&mut cursor)?;
        let deserialize_duration = in_memory_start.elapsed();
        println!("Deserializing index took {:?}", deserialize_duration);

        let query_start = std::time::Instant::now();
        for &value in &test_values {
            let key_bytes = OrderedFloat(value).to_bytes();
            let _results = in_memory_index.query_exact_bytes(&key_bytes);
        }
        let query_duration = query_start.elapsed();
        println!(
            "In-memory query took {:?} for {} queries",
            query_duration,
            test_values.len()
        );

        // Total time for in-memory approach includes deserialization + query
        println!(
            "Total in-memory time: {:?}",
            deserialize_duration + query_duration
        );

        // Only compare if stream_duration is not zero to avoid division by zero
        if !stream_duration.is_zero() {
            println!(
                "Streaming is {}x faster for {} queries",
                (deserialize_duration + query_duration).as_secs_f64()
                    / stream_duration.as_secs_f64(),
                test_values.len()
            );
        }

        Ok(())
    }
}
