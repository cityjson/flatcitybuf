// Entry in a static B+tree node
//
// This module defines the Entry struct which represents a key-value pair in a static B+tree node.
// Entries are the fundamental units of data stored in the tree, consisting of a fixed-width
// key and a 64-bit value that typically points to the actual data.

use crate::{
    error::{KeyError, Result},
    key::KeyEncoder,
};
use std::{
    cmp::Ordering,
    io::{Read, Write},
    mem::size_of,
};

/// Entry in a static B+tree node, consisting of a key and value.
///
/// An entry is a key-value pair stored in a B+tree node. The key is stored as a fixed-width
/// byte array (the width is defined by the key encoder), and the value is a 64-bit unsigned
/// integer that typically represents an offset or pointer to the actual data in storage.
///
/// # Binary Format
///
/// When serialized, an entry consists of:
/// - N bytes: key (where N is the fixed key size)
/// - 8 bytes: value (stored as little-endian u64)
#[derive(Debug, Clone)]
pub struct Entry {
    /// The encoded key with fixed width
    pub key: Vec<u8>,

    /// The value (typically an offset into the city feature data)
    pub value: u64,
}

impl Entry {
    /// Creates a new entry with the given key and value.
    ///
    /// # Parameters
    ///
    /// * `key` - The encoded key as a byte vector
    /// * `value` - The value as a 64-bit unsigned integer
    ///
    /// # Examples
    ///
    /// ```
    /// use static_btree::entry::Entry;
    ///
    /// // Create an entry with a key of [1, 2, 3] and value 42
    /// let entry = Entry::new(vec![1, 2, 3], 42);
    /// ```
    pub fn new(key: Vec<u8>, value: u64) -> Self {
        Self { key, value }
    }

    /// Returns the encoded size of this entry in bytes.
    ///
    /// This is the sum of the fixed key size and the size of a u64 (8 bytes).
    ///
    /// # Parameters
    ///
    /// * `key_size` - The fixed size of keys in bytes
    ///
    /// # Returns
    ///
    /// The total size of the encoded entry in bytes
    pub fn encoded_size(&self) -> usize {
        // Return fixed size of key + 8 bytes for value
        self.key.len() + size_of::<u64>()
    }

    /// Encodes the entry into bytes.
    ///
    /// This serializes the entry into a byte vector by concatenating the key
    /// and the little-endian representation of the value.
    ///
    /// # Returns
    ///
    /// A byte vector containing the encoded entry
    pub fn encode(&self) -> Vec<u8> {
        // Encode key and value into bytes
        let mut result = Vec::with_capacity(self.key.len() + size_of::<u64>());
        result.extend_from_slice(&self.key);
        result.extend_from_slice(&self.value.to_le_bytes());
        result
    }

    /// Encodes the entry into a writer.
    ///
    /// # Parameters
    ///
    /// * `writer` - The writer to encode the entry into
    ///
    /// # Returns
    ///
    /// The encoded entry
    ///
    /// # Errors
    ///
    /// Returns an error if the writer is not writable
    pub fn encode_to_writer(&self, writer: &mut impl Write) -> Result<()> {
        writer.write_all(&self.encode())?;
        Ok(())
    }

    /// Decodes an entry from a byte slice.
    ///
    /// # Parameters
    ///
    /// * `bytes` - The byte slice containing the encoded entry
    /// * `key_size` - The fixed size of keys in bytes
    ///
    /// # Returns
    ///
    /// The decoded entry
    ///
    /// # Errors
    ///
    /// Returns an error if the byte slice is too small to contain a valid entry
    pub fn decode_from_slice(bytes: &[u8], key_size: usize) -> Result<Self> {
        // Decode key and value from bytes
        let key = bytes[..key_size].to_vec();
        let value = u64::from_le_bytes([
            bytes[key_size],
            bytes[key_size + 1],
            bytes[key_size + 2],
            bytes[key_size + 3],
            bytes[key_size + 4],
            bytes[key_size + 5],
            bytes[key_size + 6],
            bytes[key_size + 7],
        ]);

        Ok(Self { key, value })
    }

    /// Decodes an entry from bytes.
    ///
    /// # Parameters
    ///
    /// * `bytes` - The byte slice containing the encoded entry
    /// * `key_size` - The fixed size of keys in bytes
    ///
    /// # Returns
    ///
    /// The decoded entry
    ///
    /// # Errors
    ///
    /// Returns an error if the byte slice is too small to contain a valid entry
    pub fn decode_from_reader(reader: &mut impl Read, key_size: usize) -> Result<Self> {
        // Decode key and value from bytes
        let mut key = vec![0; key_size];
        reader.read_exact(&mut key)?;
        let mut value = [0; size_of::<u64>()];
        reader.read_exact(&mut value)?;
        let value = u64::from_le_bytes(value);

        Ok(Self { key, value })
    }

    pub fn key(&self) -> &[u8] {
        &self.key
    }

    pub fn value(&self) -> u64 {
        self.value
    }
}

#[cfg(test)]
mod tests {
    use crate::key::{AnyKeyEncoder, KeyType};

    use super::*;
    use std::io::{Cursor, Read, Seek, SeekFrom};

    #[test]
    fn test_entry_new() {
        let key = vec![1, 2, 3, 4];
        let value = 42;
        let entry: Entry = Entry::new(key.clone(), value);

        assert_eq!(entry.key, key);
        assert_eq!(entry.value, value);
    }

    #[test]
    fn test_entry_encoded_size() {
        let key = vec![1, 2, 3, 4];
        let entry: Entry = Entry::new(key, 42);

        assert_eq!(entry.encoded_size(), 4 + 8); // key size + u64 size
    }

    #[test]
    fn test_entry_encode() {
        let key = vec![1, 2, 3, 4];
        let value = 42;
        let entry: Entry = Entry::new(key, value);

        let encoded = entry.encode();

        // First 4 bytes should be the key
        assert_eq!(&encoded[0..4], &[1, 2, 3, 4]);

        // Last 8 bytes should be the value (42) in little-endian
        assert_eq!(&encoded[4..12], &value.to_le_bytes());
    }

    #[test]
    fn test_entry_encode_to_writer() -> Result<()> {
        let key = vec![1, 2, 3, 4];
        let value = 42;
        let entry: Entry = Entry::new(key, value);

        let mut buffer = Vec::new();
        entry.encode_to_writer(&mut buffer)?;

        assert_eq!(buffer, entry.encode());
        Ok(())
    }

    #[test]
    fn test_entry_decode_from_slice() -> Result<()> {
        let key = vec![1, 2, 3, 4];
        let value = 42;
        let entry: Entry = Entry::new(key.clone(), value);

        let encoded = entry.encode();
        let decoded = Entry::decode_from_slice(&encoded, key.len())?;

        assert_eq!(decoded.key, key);
        assert_eq!(decoded.value, value);
        Ok(())
    }

    #[test]
    fn test_entry_decode_from_reader() -> Result<()> {
        let key = vec![1, 2, 3, 4];
        let value = 42;
        let entry: Entry = Entry::new(key.clone(), value);

        let encoded = entry.encode();
        let mut cursor = Cursor::new(encoded);
        let decoded = Entry::decode_from_reader(&mut cursor, key.len())?;

        assert_eq!(decoded.key, key);
        assert_eq!(decoded.value, value);
        Ok(())
    }

    #[test]
    fn test_entry_key_and_value_getters() {
        let key = vec![1, 2, 3, 4];
        let value = 42;
        let entry: Entry = Entry::new(key.clone(), value);

        assert_eq!(entry.key(), &key);
        assert_eq!(entry.value(), value);
    }

    #[test]
    fn test_entry_cmp() -> Result<()> {
        let encoder = AnyKeyEncoder::u32();

        // Create entries with u32 keys
        let key1 = encoder.encode(&KeyType::U32(10))?;
        let key2 = encoder.encode(&KeyType::U32(20))?;

        let entry1: Entry = Entry::new(key1, 42);
        let entry2: Entry = Entry::new(key2, 42);

        assert_eq!(encoder.compare(&entry1.key, &entry2.key), Ordering::Less);
        assert_eq!(encoder.compare(&entry2.key, &entry1.key), Ordering::Greater);
        assert_eq!(encoder.compare(&entry1.key, &entry1.key), Ordering::Equal);
        Ok(())
    }

    #[test]
    fn test_entry_round_trip_multiple_sizes() -> Result<()> {
        // Test with different key sizes
        let key_sizes = vec![2, 4, 8, 16, 32];

        for size in key_sizes {
            let key = vec![0xAA; size]; // Fill with same value for simplicity
            let value = 0xDEADBEEF;
            let entry: Entry = Entry::new(key.clone(), value);

            // Test encode/decode using slice
            let encoded = entry.encode();
            let decoded = Entry::decode_from_slice(&encoded, size)?;
            assert_eq!(decoded.key, key);
            assert_eq!(decoded.value, value);

            // Test encode/decode using reader
            let mut cursor = Cursor::new(encoded);
            let decoded = Entry::decode_from_reader(&mut cursor, size)?;
            assert_eq!(decoded.key, key);
            assert_eq!(decoded.value, value);
        }

        Ok(())
    }
}
