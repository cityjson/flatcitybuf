use crate::errors::{KeyError, Result};
use std::cmp::Ordering;

/// Trait for encoding/decoding and comparing keys in the B-tree
pub trait KeyEncoder<T> {
    /// Returns the fixed size of encoded keys in bytes
    fn encoded_size(&self) -> usize;

    /// Encodes a key into bytes with fixed width
    fn encode(&self, key: &T) -> Result<Vec<u8>>;

    /// Decodes a key from bytes
    fn decode(&self, bytes: &[u8]) -> Result<T>;

    /// Compares two encoded keys
    fn compare(&self, a: &[u8], b: &[u8]) -> Ordering;
}

/// Integer key encoder for numeric types
pub struct IntegerKeyEncoder;

impl KeyEncoder<i64> for IntegerKeyEncoder {
    fn encoded_size(&self) -> usize {
        8
    }

    fn encode(&self, key: &i64) -> Result<Vec<u8>> {
        // Encode integer as fixed 8 bytes in little-endian order
        let mut result = Vec::with_capacity(8);
        result.extend_from_slice(&key.to_le_bytes());
        Ok(result)
    }

    fn decode(&self, bytes: &[u8]) -> Result<i64> {
        // Decode integer from 8 bytes in little-endian order
        if bytes.len() < 8 {
            return Err(KeyError::InvalidSize {
                expected: 8,
                actual: bytes.len(),
            }
            .into());
        }

        let value = i64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]);
        Ok(value)
    }

    fn compare(&self, a: &[u8], b: &[u8]) -> Ordering {
        // Compare two encoded integers
        match (self.decode(a), self.decode(b)) {
            (Ok(a_val), Ok(b_val)) => a_val.cmp(&b_val),
            _ => Ordering::Equal, // Default in case of error
        }
    }
}

/// String key encoder with fixed prefix
pub struct StringKeyEncoder {
    /// Length of the prefix to use for string keys
    pub prefix_length: usize,
}

impl KeyEncoder<String> for StringKeyEncoder {
    fn encoded_size(&self) -> usize {
        self.prefix_length
    }

    fn encode(&self, key: &String) -> Result<Vec<u8>> {
        // Take prefix of string and encode with null padding if needed
        let mut result = vec![0u8; self.prefix_length];
        let bytes = key.as_bytes();
        let copy_len = std::cmp::min(bytes.len(), self.prefix_length);

        // Copy string prefix (with null padding if needed)
        result[..copy_len].copy_from_slice(&bytes[..copy_len]);
        Ok(result)
    }

    fn decode(&self, bytes: &[u8]) -> Result<String> {
        // Decode string from bytes, removing null padding
        if bytes.len() < self.prefix_length {
            return Err(KeyError::InvalidSize {
                expected: self.prefix_length,
                actual: bytes.len(),
            }
            .into());
        }

        // Find end of string (first null byte or end of prefix)
        let end = bytes
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(self.prefix_length);

        let string = String::from_utf8_lossy(&bytes[..end]).to_string();
        Ok(string)
    }

    fn compare(&self, a: &[u8], b: &[u8]) -> Ordering {
        // Compare two encoded string prefixes
        a.cmp(b)
    }
}

/// Float key encoder with NaN handling
pub struct FloatKeyEncoder;

impl KeyEncoder<f64> for FloatKeyEncoder {
    fn encoded_size(&self) -> usize {
        8
    }

    fn encode(&self, key: &f64) -> Result<Vec<u8>> {
        // Encode float with proper NaN handling
        let bits = if key.is_nan() {
            // Handle NaN: Use a specific bit pattern
            u64::MAX
        } else {
            key.to_bits()
        };

        let mut result = Vec::with_capacity(8);
        result.extend_from_slice(&bits.to_le_bytes());
        Ok(result)
    }

    fn decode(&self, bytes: &[u8]) -> Result<f64> {
        // Decode float with proper NaN handling
        if bytes.len() < 8 {
            return Err(KeyError::InvalidSize {
                expected: 8,
                actual: bytes.len(),
            }
            .into());
        }

        let bits = u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]);

        let value = if bits == u64::MAX {
            // Special case for NaN
            f64::NAN
        } else {
            f64::from_bits(bits)
        };

        Ok(value)
    }

    fn compare(&self, a: &[u8], b: &[u8]) -> Ordering {
        // Compare two encoded floats with proper NaN handling
        match (self.decode(a), self.decode(b)) {
            (Ok(a_val), Ok(b_val)) => {
                // Special handling for NaN
                match (a_val.is_nan(), b_val.is_nan()) {
                    (true, true) => Ordering::Equal,
                    (true, false) => Ordering::Greater, // NaN is greater than anything
                    (false, true) => Ordering::Less,
                    (false, false) => a_val.partial_cmp(&b_val).unwrap_or(Ordering::Equal),
                }
            }
            _ => Ordering::Equal, // Default in case of error
        }
    }
}
