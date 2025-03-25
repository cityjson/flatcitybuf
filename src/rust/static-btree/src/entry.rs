// Entry in a static B+tree node
//
// This module defines the Entry struct which represents a key-value pair in a static B+tree node.
// Entries are the fundamental units of data stored in the tree, consisting of a fixed-width
// key and a 64-bit value that typically points to the actual data.

use crate::errors::{KeyError, Result};
use std::mem::size_of;

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

    /// The value (typically an offset into the file)
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
    pub fn encoded_size(&self, key_size: usize) -> usize {
        // Return fixed size of key + 8 bytes for value
        key_size + size_of::<u64>()
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
    pub fn decode(bytes: &[u8], key_size: usize) -> Result<Self> {
        // Decode key and value from bytes
        if bytes.len() < key_size + size_of::<u64>() {
            return Err(KeyError::InvalidSize {
                expected: key_size + size_of::<u64>(),
                actual: bytes.len(),
            }
            .into());
        }

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
}
