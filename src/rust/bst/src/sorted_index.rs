use std::io::{Read, Seek, SeekFrom, Write};

use crate::byte_serializable::ByteSerializable;

/// The offset type used to point to actual record data.
pub type ValueOffset = u64;

/// A key–offset pair. The key must be orderable and serializable.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct KeyValue<T: Ord + ByteSerializable> {
    pub key: T,
    pub offsets: Vec<ValueOffset>,
}

/// A sorted index implemented as an array of key–offset pairs.
#[derive(Debug)]
pub struct SortedIndex<T: Ord + ByteSerializable> {
    pub entries: Vec<KeyValue<T>>,
}

impl<T: Ord + ByteSerializable> Default for SortedIndex<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Ord + ByteSerializable> SortedIndex<T> {
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

impl<T: Ord + ByteSerializable> SearchableIndex<T> for SortedIndex<T> {
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

impl<T: Ord + ByteSerializable> IndexSerializable for SortedIndex<T> {
    fn serialize<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
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

impl<T> AnyIndex for SortedIndex<T>
where
    T: ByteSerializable + Ord + 'static,
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
    fn http_stream_query_exact<T: http_range_client::AsyncHttpRangeClient>(
        &self,
        client: &mut http_range_client::AsyncBufferedHttpRangeClient<T>,
        index_offset: usize,
        key: &[u8],
    ) -> std::io::Result<Vec<ValueOffset>>;

    /// Returns the offsets for a range query given optional lower and upper serialized keys.
    /// For use with HTTP range requests.
    #[cfg(feature = "http")]
    fn http_stream_query_range<T: http_range_client::AsyncHttpRangeClient>(
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
}

impl SortedIndexMeta {
    /// Read metadata from a reader positioned at the start of a serialized SortedIndex.
    pub fn from_reader<R: Read + Seek>(reader: &mut R) -> std::io::Result<Self> {
        let start_pos = reader.stream_position()?;

        // Read entry count
        let mut len_bytes = [0u8; 8];
        reader.read_exact(&mut len_bytes)?;
        let entry_count = u64::from_le_bytes(len_bytes);

        // Calculate total size by seeking to the end of the index
        let mut total_size = 8; // Size of entry_count

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
        })
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

        // Skip the entry count
        reader.seek(SeekFrom::Current(8))?;

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

            // Compare keys
            match key_buf.as_slice().cmp(key) {
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

        // Skip the entry count
        reader.seek(SeekFrom::Current(8))?;

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
                if key_buf.as_slice() >= upper_bound {
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
    fn http_stream_query_exact<T: http_range_client::AsyncHttpRangeClient>(
        &self,
        client: &mut http_range_client::AsyncBufferedHttpRangeClient<T>,
        index_offset: usize,
        key: &[u8],
    ) -> std::io::Result<Vec<ValueOffset>> {
        // Implementation for HTTP will be added later

        unimplemented!("HTTP streaming not yet implemented")
    }

    #[cfg(feature = "http")]
    fn http_stream_query_range<T: http_range_client::AsyncHttpRangeClient>(
        &self,
        client: &mut http_range_client::AsyncBufferedHttpRangeClient<T>,
        index_offset: usize,
        lower: Option<&[u8]>,
        upper: Option<&[u8]>,
    ) -> std::io::Result<Vec<ValueOffset>> {
        // Implementation for HTTP will be added later
        unimplemented!("HTTP streaming not yet implemented")
    }
}

impl SortedIndexMeta {
    /// Helper method to seek to a specific entry in the index.
    fn seek_to_entry<R: Read + Seek>(
        &self,
        reader: &mut R,
        entry_index: u64,
        start_pos: u64,
    ) -> std::io::Result<()> {
        // Reset to the beginning of the index
        reader.seek(SeekFrom::Start(start_pos))?;

        // Skip the entry count
        reader.seek(SeekFrom::Current(8))?;

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
            match key_buf.as_slice().cmp(lower_bound) {
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
