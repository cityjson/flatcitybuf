use std::io::{Read, Seek, SeekFrom, Write};

use crate::{byte_serializable::ByteSerializable, error, ByteSerializableType};

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

    /// Returns the offsets for an exact match given a key.
    /// For use with HTTP range requests.
    #[cfg(feature = "http")]
    async fn http_stream_query_exact<C: http_range_client::AsyncHttpRangeClient>(
        &self,
        client: &mut http_range_client::AsyncBufferedHttpRangeClient<C>,
        index_offset: usize,
        key: &T,
    ) -> std::io::Result<Vec<ValueOffset>>;

    /// Returns the offsets for a range query given optional lower and upper keys.
    /// For use with HTTP range requests.
    #[cfg(feature = "http")]
    async fn http_stream_query_range<C: http_range_client::AsyncHttpRangeClient>(
        &self,
        client: &mut http_range_client::AsyncBufferedHttpRangeClient<C>,
        index_offset: usize,
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

    #[cfg(feature = "http")]
    async fn http_stream_query_exact<C: http_range_client::AsyncHttpRangeClient>(
        &self,
        client: &mut http_range_client::AsyncBufferedHttpRangeClient<C>,
        index_offset: usize,
        key: &T,
    ) -> std::io::Result<Vec<ValueOffset>> {
        // HTTP implementation would go here, similar to the existing one but type-aware
        unimplemented!("Type-aware HTTP streaming not yet implemented")
    }

    #[cfg(feature = "http")]
    async fn http_stream_query_range<C: http_range_client::AsyncHttpRangeClient>(
        &self,
        client: &mut http_range_client::AsyncBufferedHttpRangeClient<C>,
        index_offset: usize,
        lower: Option<&T>,
        upper: Option<&T>,
    ) -> std::io::Result<Vec<ValueOffset>> {
        // HTTP implementation would go here, similar to the existing one but type-aware
        unimplemented!("Type-aware HTTP streaming not yet implemented")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::byte_serializable::Float;
    use chrono::{NaiveDate, NaiveDateTime};
    use ordered_float::OrderedFloat;
    use std::io::Cursor;
    use std::io::{Seek, SeekFrom};

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
}
