use std::io::{Read, Seek, SeekFrom, Write};

use crate::byte_serializable::ByteSerializable;

/// The offset type used to point to actual record data.
pub type ValueOffset = u64;

/// A key–offset pair. The key must be orderable and serializable.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct KeyValue<T: Ord + ByteSerializable + 'static> {
    pub key: T,
    pub offsets: Vec<ValueOffset>,
}

/// A sorted index implemented as an array of key–offset pairs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SortedIndex<T: Ord + ByteSerializable + 'static> {
    pub entries: Vec<KeyValue<T>>,
}

impl<T: Ord + ByteSerializable + 'static> Default for SortedIndex<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Ord + ByteSerializable + 'static> SortedIndex<T> {
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

    // Helper method to get a type identifier for T
    fn get_type_id() -> u32 {
        // This is a simple way to identify types
        // In a real implementation, you might want a more robust approach
        if std::any::TypeId::of::<T>() == std::any::TypeId::of::<ordered_float::OrderedFloat<f32>>()
        {
            1
        } else if std::any::TypeId::of::<T>()
            == std::any::TypeId::of::<ordered_float::OrderedFloat<f64>>()
        {
            2
        } else if std::any::TypeId::of::<T>() == std::any::TypeId::of::<String>() {
            3
        } else if std::any::TypeId::of::<T>() == std::any::TypeId::of::<i32>() {
            4
        } else if std::any::TypeId::of::<T>() == std::any::TypeId::of::<i64>() {
            5
        } else if std::any::TypeId::of::<T>() == std::any::TypeId::of::<u32>() {
            6
        } else if std::any::TypeId::of::<T>() == std::any::TypeId::of::<u64>() {
            7
        } else if std::any::TypeId::of::<T>() == std::any::TypeId::of::<bool>() {
            8
        } else if std::any::TypeId::of::<T>() == std::any::TypeId::of::<i16>() {
            9
        } else if std::any::TypeId::of::<T>() == std::any::TypeId::of::<i8>() {
            10
        } else if std::any::TypeId::of::<T>() == std::any::TypeId::of::<u16>() {
            11
        } else if std::any::TypeId::of::<T>() == std::any::TypeId::of::<u8>() {
            12
        } else if std::any::TypeId::of::<T>() == std::any::TypeId::of::<chrono::NaiveDateTime>() {
            13
        } else if std::any::TypeId::of::<T>() == std::any::TypeId::of::<chrono::NaiveDate>() {
            14
        } else if std::any::TypeId::of::<T>()
            == std::any::TypeId::of::<chrono::DateTime<chrono::Utc>>()
        {
            15
        } else {
            0 // Unknown type
        }
    }
}

/// A trait defining flexible search operations on an index.
pub trait SearchableIndex<T: Ord + ByteSerializable> {
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

impl<T: Ord + ByteSerializable + 'static> SearchableIndex<T> for SortedIndex<T> {
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

/// A trait for serializing and deserializing an index.
pub trait IndexSerializable {
    /// Write the index to a writer.
    fn serialize<W: Write>(&self, writer: &mut W) -> std::io::Result<()>;

    /// Read the index from a reader.
    fn deserialize<R: Read>(reader: &mut R) -> std::io::Result<Self>
    where
        Self: Sized;
}

impl<T: Ord + ByteSerializable + 'static> IndexSerializable for SortedIndex<T> {
    fn serialize<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        // Write the type identifier for T
        let type_id = Self::get_type_id();
        writer.write_all(&type_id.to_le_bytes())?;

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

    fn deserialize<R: Read>(reader: &mut R) -> std::io::Result<Self> {
        // Read the type identifier
        let mut type_id_bytes = [0u8; 4];
        reader.read_exact(&mut type_id_bytes)?;
        let type_id = u32::from_le_bytes(type_id_bytes);

        // Verify the type matches
        if type_id != Self::get_type_id() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "Type mismatch: expected {}, got {}",
                    Self::get_type_id(),
                    type_id
                ),
            ));
        }

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
        Ok(SortedIndex { entries })
    }
}

pub trait AnyIndex {
    /// Returns the offsets for an exact match given a serialized key.
    fn query_exact_bytes(&self, key: &[u8]) -> Vec<ValueOffset>;
    /// Returns the offsets for a range query given optional lower and upper serialized keys.
    fn query_range_bytes(&self, lower: Option<&[u8]>, upper: Option<&[u8]>) -> Vec<ValueOffset>;
}

impl<T: Ord + ByteSerializable + 'static> AnyIndex for SortedIndex<T>
where
    T: 'static,
{
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

/// A trait for streaming access to index data without loading the entire index into memory.
pub trait StreamableIndex {
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
    async fn http_stream_query_exact<T: http_range_client::AsyncHttpRangeClient>(
        &self,
        client: &mut http_range_client::AsyncBufferedHttpRangeClient<T>,
        index_offset: usize,
        key: &[u8],
    ) -> std::io::Result<Vec<ValueOffset>>;

    /// Returns the offsets for a range query given optional lower and upper serialized keys.
    /// For use with HTTP range requests.
    #[cfg(feature = "http")]
    async fn http_stream_query_range<T: http_range_client::AsyncHttpRangeClient>(
        &self,
        client: &mut http_range_client::AsyncBufferedHttpRangeClient<T>,
        index_offset: usize,
        lower: Option<&[u8]>,
        upper: Option<&[u8]>,
    ) -> std::io::Result<Vec<ValueOffset>>;
}

/// Metadata for a serialized SortedIndex, used for streaming access.
pub struct SortedIndexMeta {
    /// Number of entries in the index.
    pub entry_count: u64,
    /// Total size of the index in bytes.
    pub size: u64,
    /// Type identifier for the index.
    pub type_id: u32,
}

impl SortedIndexMeta {
    /// Read metadata from a reader positioned at the start of a serialized SortedIndex.
    pub fn from_reader<R: Read + Seek>(reader: &mut R) -> std::io::Result<Self> {
        let start_pos = reader.stream_position()?;

        // Read type identifier
        let mut type_id_bytes = [0u8; 4];
        reader.read_exact(&mut type_id_bytes)?;
        let type_id = u32::from_le_bytes(type_id_bytes);

        // Read entry count
        let mut len_bytes = [0u8; 8];
        reader.read_exact(&mut len_bytes)?;
        let entry_count = u64::from_le_bytes(len_bytes);

        // Calculate total size by seeking to the end of the index
        let mut total_size = 4 + 8; // Size of type_id + entry_count

        for _ in 0..entry_count {
            // Read key length
            let mut key_len_bytes = [0u8; 8];
            reader.read_exact(&mut key_len_bytes)?;
            let key_len = u64::from_le_bytes(key_len_bytes) as usize;
            total_size += 8; // Size of key_len

            // Skip key bytes
            reader.seek(SeekFrom::Current(key_len as i64))?;
            total_size += key_len as u64;

            // Read offsets length
            let mut offsets_len_bytes = [0u8; 8];
            reader.read_exact(&mut offsets_len_bytes)?;
            let offsets_len = u64::from_le_bytes(offsets_len_bytes) as usize;
            total_size += 8; // Size of offsets_len

            // Skip offset bytes
            reader.seek(SeekFrom::Current((offsets_len * 8) as i64))?;
            total_size += (offsets_len * 8) as u64;
        }

        // Reset position
        reader.seek(SeekFrom::Start(start_pos))?;

        Ok(SortedIndexMeta {
            entry_count,
            size: total_size,
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

            // Compare keys using type-specific comparison
            match self.compare_keys(&key_buf, lower_bound) {
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
        match self.type_id {
            1 => {
                // OrderedFloat<f32>
                if key_bytes.len() == 4 && query_key.len() == 4 {
                    let key_val = f32::from_le_bytes([
                        key_bytes[0],
                        key_bytes[1],
                        key_bytes[2],
                        key_bytes[3],
                    ]);
                    let query_val = f32::from_le_bytes([
                        query_key[0],
                        query_key[1],
                        query_key[2],
                        query_key[3],
                    ]);

                    // Use epsilon-based comparison for floating-point equality
                    const EPSILON: f32 = 1e-6;
                    let diff = (key_val - query_val).abs();

                    if diff < EPSILON {
                        std::cmp::Ordering::Equal
                    } else {
                        key_val
                            .partial_cmp(&query_val)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    }
                } else {
                    key_bytes.cmp(query_key)
                }
            }
            2 => {
                // OrderedFloat<f64>
                if key_bytes.len() == 8 && query_key.len() == 8 {
                    let key_val = f64::from_le_bytes([
                        key_bytes[0],
                        key_bytes[1],
                        key_bytes[2],
                        key_bytes[3],
                        key_bytes[4],
                        key_bytes[5],
                        key_bytes[6],
                        key_bytes[7],
                    ]);
                    let query_val = f64::from_le_bytes([
                        query_key[0],
                        query_key[1],
                        query_key[2],
                        query_key[3],
                        query_key[4],
                        query_key[5],
                        query_key[6],
                        query_key[7],
                    ]);

                    // Use epsilon-based comparison for floating-point equality
                    const EPSILON: f64 = 1e-12;
                    let diff = (key_val - query_val).abs();

                    if diff < EPSILON {
                        std::cmp::Ordering::Equal
                    } else {
                        key_val
                            .partial_cmp(&query_val)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    }
                } else {
                    key_bytes.cmp(query_key)
                }
            }
            3 => {
                // String
                // For strings, we can directly compare the byte slices
                key_bytes.cmp(query_key)
            }
            4 => {
                // i32
                if key_bytes.len() == 4 && query_key.len() == 4 {
                    let key_val = i32::from_le_bytes([
                        key_bytes[0],
                        key_bytes[1],
                        key_bytes[2],
                        key_bytes[3],
                    ]);
                    let query_val = i32::from_le_bytes([
                        query_key[0],
                        query_key[1],
                        query_key[2],
                        query_key[3],
                    ]);
                    key_val.cmp(&query_val)
                } else {
                    key_bytes.cmp(query_key)
                }
            }
            5 => {
                // i64
                if key_bytes.len() == 8 && query_key.len() == 8 {
                    let key_val = i64::from_le_bytes([
                        key_bytes[0],
                        key_bytes[1],
                        key_bytes[2],
                        key_bytes[3],
                        key_bytes[4],
                        key_bytes[5],
                        key_bytes[6],
                        key_bytes[7],
                    ]);
                    let query_val = i64::from_le_bytes([
                        query_key[0],
                        query_key[1],
                        query_key[2],
                        query_key[3],
                        query_key[4],
                        query_key[5],
                        query_key[6],
                        query_key[7],
                    ]);
                    key_val.cmp(&query_val)
                } else {
                    key_bytes.cmp(query_key)
                }
            }
            6 => {
                // u32
                if key_bytes.len() == 4 && query_key.len() == 4 {
                    let key_val = u32::from_le_bytes([
                        key_bytes[0],
                        key_bytes[1],
                        key_bytes[2],
                        key_bytes[3],
                    ]);
                    let query_val = u32::from_le_bytes([
                        query_key[0],
                        query_key[1],
                        query_key[2],
                        query_key[3],
                    ]);
                    key_val.cmp(&query_val)
                } else {
                    key_bytes.cmp(query_key)
                }
            }
            7 => {
                // u64
                if key_bytes.len() == 8 && query_key.len() == 8 {
                    let key_val = u64::from_le_bytes([
                        key_bytes[0],
                        key_bytes[1],
                        key_bytes[2],
                        key_bytes[3],
                        key_bytes[4],
                        key_bytes[5],
                        key_bytes[6],
                        key_bytes[7],
                    ]);
                    let query_val = u64::from_le_bytes([
                        query_key[0],
                        query_key[1],
                        query_key[2],
                        query_key[3],
                        query_key[4],
                        query_key[5],
                        query_key[6],
                        query_key[7],
                    ]);
                    key_val.cmp(&query_val)
                } else {
                    key_bytes.cmp(query_key)
                }
            }
            8 => {
                // bool
                if key_bytes.len() == 1 && query_key.len() == 1 {
                    let key_val = key_bytes[0] != 0;
                    let query_val = query_key[0] != 0;
                    key_val.cmp(&query_val)
                } else {
                    key_bytes.cmp(query_key)
                }
            }
            9 => {
                // i16
                if key_bytes.len() == 2 && query_key.len() == 2 {
                    let key_val = i16::from_le_bytes([key_bytes[0], key_bytes[1]]);
                    let query_val = i16::from_le_bytes([query_key[0], query_key[1]]);
                    key_val.cmp(&query_val)
                } else {
                    key_bytes.cmp(query_key)
                }
            }
            10 => {
                // i8
                if key_bytes.len() == 1 && query_key.len() == 1 {
                    let key_val = key_bytes[0] as i8;
                    let query_val = query_key[0] as i8;
                    key_val.cmp(&query_val)
                } else {
                    key_bytes.cmp(query_key)
                }
            }
            11 => {
                // u16
                if key_bytes.len() == 2 && query_key.len() == 2 {
                    let key_val = u16::from_le_bytes([key_bytes[0], key_bytes[1]]);
                    let query_val = u16::from_le_bytes([query_key[0], query_key[1]]);
                    key_val.cmp(&query_val)
                } else {
                    key_bytes.cmp(query_key)
                }
            }
            12 => {
                // u8
                if key_bytes.len() == 1 && query_key.len() == 1 {
                    let key_val = key_bytes[0];
                    let query_val = query_key[0];
                    key_val.cmp(&query_val)
                } else {
                    key_bytes.cmp(query_key)
                }
            }
            13 => {
                // NaiveDateTime
                if key_bytes.len() >= 12 && query_key.len() >= 12 {
                    // Extract timestamp
                    let mut ts_key_bytes = [0u8; 8];
                    let mut ts_query_bytes = [0u8; 8];
                    ts_key_bytes.copy_from_slice(&key_bytes[0..8]);
                    ts_query_bytes.copy_from_slice(&query_key[0..8]);
                    let key_ts = i64::from_le_bytes(ts_key_bytes);
                    let query_ts = i64::from_le_bytes(ts_query_bytes);

                    // Extract nanoseconds
                    let mut ns_key_bytes = [0u8; 4];
                    let mut ns_query_bytes = [0u8; 4];
                    ns_key_bytes.copy_from_slice(&key_bytes[8..12]);
                    ns_query_bytes.copy_from_slice(&query_key[8..12]);
                    let key_ns = u32::from_le_bytes(ns_key_bytes);
                    let query_ns = u32::from_le_bytes(ns_query_bytes);

                    // Compare timestamps first, then nanoseconds
                    match key_ts.cmp(&query_ts) {
                        std::cmp::Ordering::Equal => key_ns.cmp(&query_ns),
                        other => other,
                    }
                } else {
                    key_bytes.cmp(query_key)
                }
            }
            14 => {
                // NaiveDate
                if key_bytes.len() >= 12 && query_key.len() >= 12 {
                    // Extract year
                    let mut year_key_bytes = [0u8; 4];
                    let mut year_query_bytes = [0u8; 4];
                    year_key_bytes.copy_from_slice(&key_bytes[0..4]);
                    year_query_bytes.copy_from_slice(&query_key[0..4]);
                    let key_year = i32::from_le_bytes(year_key_bytes);
                    let query_year = i32::from_le_bytes(year_query_bytes);

                    // Extract month
                    let mut month_key_bytes = [0u8; 4];
                    let mut month_query_bytes = [0u8; 4];
                    month_key_bytes.copy_from_slice(&key_bytes[4..8]);
                    month_query_bytes.copy_from_slice(&query_key[4..8]);
                    let key_month = u32::from_le_bytes(month_key_bytes);
                    let query_month = u32::from_le_bytes(month_query_bytes);

                    // Extract day
                    let mut day_key_bytes = [0u8; 4];
                    let mut day_query_bytes = [0u8; 4];
                    day_key_bytes.copy_from_slice(&key_bytes[8..12]);
                    day_query_bytes.copy_from_slice(&query_key[8..12]);
                    let key_day = u32::from_le_bytes(day_key_bytes);
                    let query_day = u32::from_le_bytes(day_query_bytes);

                    // Compare year, month, day in order
                    match key_year.cmp(&query_year) {
                        std::cmp::Ordering::Equal => match key_month.cmp(&query_month) {
                            std::cmp::Ordering::Equal => key_day.cmp(&query_day),
                            other => other,
                        },
                        other => other,
                    }
                } else {
                    key_bytes.cmp(query_key)
                }
            }
            15 => {
                // DateTime<Utc>
                // DateTime<Utc> is serialized the same as NaiveDateTime
                if key_bytes.len() >= 12 && query_key.len() >= 12 {
                    // Extract timestamp
                    let mut ts_key_bytes = [0u8; 8];
                    let mut ts_query_bytes = [0u8; 8];
                    ts_key_bytes.copy_from_slice(&key_bytes[0..8]);
                    ts_query_bytes.copy_from_slice(&query_key[0..8]);
                    let key_ts = i64::from_le_bytes(ts_key_bytes);
                    let query_ts = i64::from_le_bytes(ts_query_bytes);

                    // Extract nanoseconds
                    let mut ns_key_bytes = [0u8; 4];
                    let mut ns_query_bytes = [0u8; 4];
                    ns_key_bytes.copy_from_slice(&key_bytes[8..12]);
                    ns_query_bytes.copy_from_slice(&query_key[8..12]);
                    let key_ns = u32::from_le_bytes(ns_key_bytes);
                    let query_ns = u32::from_le_bytes(ns_query_bytes);

                    // Compare timestamps first, then nanoseconds
                    match key_ts.cmp(&query_ts) {
                        std::cmp::Ordering::Equal => key_ns.cmp(&query_ns),
                        other => other,
                    }
                } else {
                    key_bytes.cmp(query_key)
                }
            }
            _ => key_bytes.cmp(query_key), // Default to byte comparison for other types
        }
    }

    /// Helper method to find the lower bound index for range queries over HTTP.
    #[cfg(feature = "http")]
    pub async fn http_find_lower_bound<T: http_range_client::AsyncHttpRangeClient>(
        &self,
        client: &mut http_range_client::AsyncBufferedHttpRangeClient<T>,
        index_offset: usize,
        lower_bound: &[u8],
    ) -> Result<u64, Box<dyn std::error::Error>> {
        // Binary search to find the lower bound
        let mut left = 0;
        let mut right = self.entry_count as i64 - 1;
        let mut result = 0;

        while left <= right {
            let mid = left + (right - left) / 2;

            // Calculate the position of the mid entry
            let mut pos = index_offset + 12; // Skip type_id (4 bytes) and entry_count (8 bytes)

            // Find the position of the mid entry
            for i in 0..mid {
                // For each entry, we need to fetch its key length
                let key_len_range = pos..(pos + 8);
                let key_len_bytes = client
                    .min_req_size(0)
                    .get_range(key_len_range.start, key_len_range.len())
                    .await?;

                let key_len =
                    u64::from_le_bytes(key_len_bytes.as_ref().try_into().unwrap()) as usize;

                // Skip key bytes
                pos += 8 + key_len;

                // Get offsets length
                let offsets_len_range = pos..(pos + 8);
                let offsets_len_bytes = client
                    .min_req_size(0)
                    .get_range(offsets_len_range.start, offsets_len_range.len())
                    .await?;

                let offsets_len =
                    u64::from_le_bytes(offsets_len_bytes.as_ref().try_into().unwrap()) as usize;

                // Skip offset bytes
                pos += 8 + (offsets_len * 8);
            }

            // Now pos is at the mid entry
            // Read key length
            let key_len_range = pos..(pos + 8);
            let key_len_bytes = client
                .min_req_size(0)
                .get_range(key_len_range.start, key_len_range.len())
                .await?;

            let key_len = u64::from_le_bytes(key_len_bytes.as_ref().try_into().unwrap()) as usize;

            // Read key bytes
            let key_bytes_range = (pos + 8)..(pos + 8 + key_len);
            let key_buf = client
                .min_req_size(0)
                .get_range(key_bytes_range.start, key_bytes_range.len())
                .await?;

            // Compare keys using type-specific comparison
            match self.compare_keys(key_buf, lower_bound) {
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
}

impl StreamableIndex for SortedIndexMeta {
    fn index_size(&self) -> u64 {
        self.size
    }

    fn stream_query_exact<R: Read + Seek>(
        &self,
        reader: &mut R,
        key: &[u8],
    ) -> std::io::Result<Vec<ValueOffset>> {
        // Save the current position
        let start_pos = reader.stream_position()?;

        // Skip the type identifier and entry count
        reader.seek(SeekFrom::Current(12))?;

        // Binary search through the index
        let mut left = 0;
        let mut right = self.entry_count as i64 - 1;
        let mut result = Vec::new();

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

            // Compare keys using type-specific comparison
            match self.compare_keys(&key_buf, key) {
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

    #[cfg(feature = "http")]
    async fn http_stream_query_exact<T: http_range_client::AsyncHttpRangeClient>(
        &self,
        client: &mut http_range_client::AsyncBufferedHttpRangeClient<T>,
        index_offset: usize,
        key: &[u8],
    ) -> std::io::Result<Vec<ValueOffset>> {
        use std::io::{Error, ErrorKind};

        // Binary search through the index
        let mut left = 0;
        let mut right = self.entry_count as i64 - 1;
        let mut result = Vec::new();

        while left <= right {
            let mid = left + (right - left) / 2;

            // Calculate the position of the mid entry
            let mut pos = index_offset + 12; // Skip type_id (4 bytes) and entry_count (8 bytes)

            // We need to find the position of the mid entry by calculating offsets
            // This is more complex with HTTP since we can't just seek
            // First, get the size of each entry up to mid
            for i in 0..mid {
                // For each entry, we need to fetch its key length
                let key_len_range = pos..(pos + 8);
                let key_len_bytes = client
                    .min_req_size(0)
                    .get_range(key_len_range.start, key_len_range.len())
                    .await
                    .map_err(|e| Error::new(ErrorKind::Other, e.to_string()))?;

                let key_len =
                    u64::from_le_bytes(key_len_bytes.as_ref().try_into().unwrap()) as usize;

                // Skip key bytes
                pos += 8 + key_len;

                // Get offsets length
                let offsets_len_range = pos..(pos + 8);
                let offsets_len_bytes = client
                    .min_req_size(0)
                    .get_range(offsets_len_range.start, offsets_len_range.len())
                    .await
                    .map_err(|e| Error::new(ErrorKind::Other, e.to_string()))?;

                let offsets_len =
                    u64::from_le_bytes(offsets_len_bytes.as_ref().try_into().unwrap()) as usize;

                // Skip offset bytes
                pos += 8 + (offsets_len * 8);
            }

            // Now pos is at the mid entry
            // Read key length
            let key_len_range = pos..(pos + 8);
            let key_len_bytes = client
                .min_req_size(0)
                .get_range(key_len_range.start, key_len_range.len())
                .await
                .map_err(|e| Error::new(ErrorKind::Other, e.to_string()))?;

            let key_len = u64::from_le_bytes(key_len_bytes.as_ref().try_into().unwrap()) as usize;

            // Read key bytes
            let key_bytes_range = (pos + 8)..(pos + 8 + key_len);
            let key_buf = client
                .min_req_size(0)
                .get_range(key_bytes_range.start, key_bytes_range.len())
                .await
                .map_err(|e| Error::new(ErrorKind::Other, e.to_string()))?;

            // Compare keys using type-specific comparison
            match self.compare_keys(key_buf, key) {
                std::cmp::Ordering::Equal => {
                    // Found a match, read offsets
                    let offsets_len_range = (pos + 8 + key_len)..(pos + 8 + key_len + 8);
                    let offsets_len_bytes = client
                        .min_req_size(0)
                        .get_range(offsets_len_range.start, offsets_len_range.len())
                        .await
                        .map_err(|e| Error::new(ErrorKind::Other, e.to_string()))?;

                    let offsets_len =
                        u64::from_le_bytes(offsets_len_bytes.as_ref().try_into().unwrap()) as usize;

                    // Read all offsets in one request
                    let offsets_range =
                        (pos + 8 + key_len + 8)..(pos + 8 + key_len + 8 + (offsets_len * 8));
                    let offsets_bytes = client
                        .min_req_size(0)
                        .get_range(offsets_range.start, offsets_range.len())
                        .await
                        .map_err(|e| Error::new(ErrorKind::Other, e.to_string()))?;

                    // Process all offsets
                    for i in 0..offsets_len {
                        let offset_start = i * 8;
                        let offset_end = offset_start + 8;
                        let offset_bytes = &offsets_bytes[offset_start..offset_end];
                        let offset = u64::from_le_bytes(offset_bytes.try_into().unwrap());
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

        Ok(result)
    }

    #[cfg(feature = "http")]
    async fn http_stream_query_range<T: http_range_client::AsyncHttpRangeClient>(
        &self,
        client: &mut http_range_client::AsyncBufferedHttpRangeClient<T>,
        index_offset: usize,
        lower: Option<&[u8]>,
        upper: Option<&[u8]>,
    ) -> std::io::Result<Vec<ValueOffset>> {
        use std::io::{Error, ErrorKind};

        let mut result = Vec::new();

        // Find the starting position based on lower bound
        let start_index = if let Some(lower_bound) = lower {
            self.http_find_lower_bound(client, index_offset, lower_bound)
                .await
                .map_err(|e| Error::new(ErrorKind::Other, e.to_string()))?
        } else {
            0
        };

        // Calculate the position of the start_index entry
        let mut pos = index_offset + 12; // Skip type_id (4 bytes) and entry_count (8 bytes)

        // Find the position of the start_index entry
        for i in 0..start_index {
            // For each entry, we need to fetch its key length
            let key_len_range = pos..(pos + 8);
            let key_len_bytes = client
                .min_req_size(0)
                .get_range(key_len_range.start, key_len_range.len())
                .await
                .map_err(|e| Error::new(ErrorKind::Other, e.to_string()))?;

            let key_len = u64::from_le_bytes(key_len_bytes.as_ref().try_into().unwrap()) as usize;

            // Skip key bytes
            pos += 8 + key_len;

            // Get offsets length
            let offsets_len_range = pos..(pos + 8);
            let offsets_len_bytes = client
                .min_req_size(0)
                .get_range(offsets_len_range.start, offsets_len_range.len())
                .await
                .map_err(|e| Error::new(ErrorKind::Other, e.to_string()))?;

            let offsets_len =
                u64::from_le_bytes(offsets_len_bytes.as_ref().try_into().unwrap()) as usize;

            // Skip offset bytes
            pos += 8 + (offsets_len * 8);
        }

        // Iterate through entries until we hit the upper bound
        for i in start_index..self.entry_count {
            // Read key length
            let key_len_range = pos..(pos + 8);
            let key_len_bytes = client
                .min_req_size(0)
                .get_range(key_len_range.start, key_len_range.len())
                .await
                .map_err(|e| Error::new(ErrorKind::Other, e.to_string()))?;

            let key_len = u64::from_le_bytes(key_len_bytes.as_ref().try_into().unwrap()) as usize;

            // Read key bytes
            let key_bytes_range = (pos + 8)..(pos + 8 + key_len);
            let key_buf = client
                .min_req_size(0)
                .get_range(key_bytes_range.start, key_bytes_range.len())
                .await
                .map_err(|e| Error::new(ErrorKind::Other, e.to_string()))?;

            // Check upper bound
            if let Some(upper_bound) = upper {
                if self.compare_keys(key_buf, upper_bound) != std::cmp::Ordering::Less {
                    break;
                }
            }

            // Read offsets
            let offsets_len_range = (pos + 8 + key_len)..(pos + 8 + key_len + 8);
            let offsets_len_bytes = client
                .min_req_size(0)
                .get_range(offsets_len_range.start, offsets_len_range.len())
                .await
                .map_err(|e| Error::new(ErrorKind::Other, e.to_string()))?;

            let offsets_len =
                u64::from_le_bytes(offsets_len_bytes.as_ref().try_into().unwrap()) as usize;

            // Read all offsets in one request
            let offsets_range =
                (pos + 8 + key_len + 8)..(pos + 8 + key_len + 8 + (offsets_len * 8));
            let offsets_bytes = client
                .min_req_size(0)
                .get_range(offsets_range.start, offsets_range.len())
                .await
                .map_err(|e| Error::new(ErrorKind::Other, e.to_string()))?;

            // Process all offsets
            for j in 0..offsets_len {
                let offset_start = j * 8;
                let offset_end = offset_start + 8;
                let offset_bytes = &offsets_bytes[offset_start..offset_end];
                let offset = u64::from_le_bytes(offset_bytes.try_into().unwrap());
                result.push(offset);
            }

            // Move to the next entry
            pos += 8 + key_len + 8 + (offsets_len * 8);
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    // Helper function to create a sample height index
    fn create_sample_height_index() -> SortedIndex<OrderedFloat<f32>> {
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

        let mut index = SortedIndex::new();
        index.build_index(entries);
        index
    }

    // Helper function to create a sample building ID index
    fn create_sample_id_index() -> SortedIndex<String> {
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

        let mut index = SortedIndex::new();
        index.build_index(entries);
        index
    }

    #[test]
    fn test_stream_query_exact_height() -> std::io::Result<()> {
        // Create a sample height index
        let height_index = create_sample_height_index();

        // Serialize the index to a buffer
        let mut buffer = Vec::new();
        height_index.serialize(&mut buffer)?;

        // Create a cursor for the buffer
        let mut cursor = Cursor::new(buffer);

        // Read the index metadata
        let index_meta = SortedIndexMeta::from_reader(&mut cursor)?;
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
    fn test_stream_query_range_height() -> std::io::Result<()> {
        // Create a sample height index
        let height_index = create_sample_height_index();

        // Serialize the index to a buffer
        let mut buffer = Vec::new();
        height_index.serialize(&mut buffer)?;

        // Create a cursor for the buffer
        let mut cursor = Cursor::new(buffer);

        // Read the index metadata
        let index_meta = SortedIndexMeta::from_reader(&mut cursor)?;

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
    fn test_stream_query_exact_id() -> std::io::Result<()> {
        // Create a sample ID index
        let id_index = create_sample_id_index();

        // Serialize the index to a buffer
        let mut buffer = Vec::new();
        id_index.serialize(&mut buffer)?;

        // Create a cursor for the buffer
        let mut cursor = Cursor::new(buffer);

        // Read the index metadata
        let index_meta = SortedIndexMeta::from_reader(&mut cursor)?;
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
    fn test_stream_query_range_id() -> std::io::Result<()> {
        // Create a sample ID index
        let id_index = create_sample_id_index();

        // Serialize the index to a buffer
        let mut buffer = Vec::new();
        id_index.serialize(&mut buffer)?;

        // Create a cursor for the buffer
        let mut cursor = Cursor::new(buffer);

        // Read the index metadata
        let index_meta = SortedIndexMeta::from_reader(&mut cursor)?;

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
    fn test_performance_comparison() -> std::io::Result<()> {
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

        let mut large_index = SortedIndex::new();
        large_index.build_index(entries);

        // Serialize the index to a buffer
        let mut buffer = Vec::new();
        large_index.serialize(&mut buffer)?;
        println!("Serialized index size: {} bytes", buffer.len());

        // Test values to query
        let test_values = [10.5f32, 25.75f32, 50.25f32, 75.8f32, 99.99f32];

        // Measure streaming query performance
        let mut cursor = Cursor::new(buffer.clone());
        let index_meta = SortedIndexMeta::from_reader(&mut cursor)?;

        let stream_start = std::time::Instant::now();
        for &value in &test_values {
            let key_bytes = value.to_bytes();
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
        let in_memory_index = SortedIndex::<OrderedFloat<f32>>::deserialize(&mut cursor)?;
        let deserialize_duration = in_memory_start.elapsed();
        println!("Deserializing index took {:?}", deserialize_duration);

        let query_start = std::time::Instant::now();
        for &value in &test_values {
            let key_bytes = value.to_bytes();
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
    async fn test_http_stream_query_exact_height() -> std::io::Result<()> {
        // Create a sample height index
        let height_index = create_sample_height_index();

        // Serialize the index to a buffer
        let mut buffer = Vec::new();
        height_index.serialize(&mut buffer)?;

        // Create a mock HTTP client with the serialized data
        let data = Arc::new(Mutex::new(buffer.clone()));
        let mock_client = MockHttpClient { data };
        let mut buffered_client = AsyncBufferedHttpRangeClient::with(mock_client, "test-url");

        // Read the index metadata
        let index_meta = SortedIndexMeta::from_reader(&mut Cursor::new(buffer.clone()))?;

        // Test exact query for height 30.0
        let test_height = OrderedFloat(30.0f32);
        let height_bytes = test_height.to_bytes();

        // Perform HTTP streaming query
        let http_results = index_meta
            .http_stream_query_exact(&mut buffered_client, 0, &height_bytes)
            .await?;

        // Get expected results from the original index
        let expected_results = height_index.query_exact_bytes(&height_bytes);

        // Compare results
        assert_eq!(
            http_results.len(),
            expected_results.len(),
            "Result count mismatch"
        );
        assert_eq!(http_results, expected_results, "Results don't match");

        // Verify the actual values
        assert_eq!(
            http_results,
            vec![6, 7, 8],
            "Expected buildings 6, 7, 8 to have height 30.0"
        );

        Ok(())
    }

    #[cfg(feature = "http")]
    #[tokio_test]
    async fn test_http_stream_query_range_height() -> std::io::Result<()> {
        // Create a sample height index
        let height_index = create_sample_height_index();

        // Serialize the index to a buffer
        let mut buffer = Vec::new();
        height_index.serialize(&mut buffer)?;

        // Create a mock HTTP client with the serialized data
        let data = Arc::new(Mutex::new(buffer.clone()));
        let mock_client = MockHttpClient { data };
        let mut buffered_client = AsyncBufferedHttpRangeClient::with(mock_client, "test-url");

        // Read the index metadata
        let index_meta = SortedIndexMeta::from_reader(&mut Cursor::new(buffer.clone()))?;

        // Test range query for heights between 25.0 and 40.0
        let lower_bound = 25.0f32;
        let upper_bound = 40.0f32;
        let lower_bytes = lower_bound.to_bytes();
        let upper_bytes = upper_bound.to_bytes();

        // Perform HTTP streaming range query
        let http_results = index_meta
            .http_stream_query_range(
                &mut buffered_client,
                0,
                Some(&lower_bytes),
                Some(&upper_bytes),
            )
            .await?;

        // Get expected results from the original index
        let expected_results =
            height_index.query_range_bytes(Some(&lower_bytes), Some(&upper_bytes));

        // Sort both result sets for comparison
        let mut http_sorted = http_results.clone();
        http_sorted.sort();
        let mut expected_sorted = expected_results.clone();
        expected_sorted.sort();

        // Compare results
        assert_eq!(
            http_sorted.len(),
            expected_sorted.len(),
            "Result count mismatch"
        );
        assert_eq!(http_sorted, expected_sorted, "Results don't match");

        // Verify the actual values
        let expected_buildings = vec![5, 6, 7, 8, 9, 10];
        assert_eq!(
            http_sorted, expected_buildings,
            "Expected buildings with heights between 25.0 and 40.0"
        );

        Ok(())
    }
}
