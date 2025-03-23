use crate::errors::{KeyError, Result};
use std::mem::size_of;

/// Entry in a B-tree node, consisting of a key and value
#[derive(Debug, Clone)]
pub struct Entry {
    /// The encoded key with fixed width
    pub key: Vec<u8>,

    /// The value (typically an offset into the file)
    pub value: u64,
}

impl Entry {
    /// Creates a new entry with the given key and value
    pub fn new(key: Vec<u8>, value: u64) -> Self {
        Self { key, value }
    }

    /// Returns the encoded size of this entry in bytes
    pub fn encoded_size(&self, key_size: usize) -> usize {
        // Return fixed size of key + 8 bytes for value
        key_size + size_of::<u64>()
    }

    /// Encodes the entry into bytes
    pub fn encode(&self) -> Vec<u8> {
        // Encode key and value into bytes
        let mut result = Vec::with_capacity(self.key.len() + size_of::<u64>());
        result.extend_from_slice(&self.key);
        result.extend_from_slice(&self.value.to_le_bytes());
        result
    }

    /// Decodes an entry from bytes
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
