use std::io::{Read, Write};

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
