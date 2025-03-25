// Key encoding/decoding for static B+tree indexes
//
// This module provides type-safe encoders for storing different key types in a static B+tree.
// The encoders convert various types to fixed-width binary representations for efficient storage.
// Each encoder guarantees consistent binary representation and proper ordering semantics.

use crate::errors::{KeyError, Result};
use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, Utc};
use std::cmp::Ordering;

/// Marker type for different key types
///
/// This enum is used to select the appropriate key encoder for a specific data type.
/// It is not meant to be instantiated, but used as a type parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyType {
    /// 8-bit signed integer
    I8,
    /// 16-bit signed integer
    I16,
    /// 32-bit signed integer
    I32,
    /// 64-bit signed integer
    I64,
    /// 8-bit unsigned integer
    U8,
    /// 16-bit unsigned integer
    U16,
    /// 32-bit unsigned integer
    U32,
    /// 64-bit unsigned integer
    U64,
    /// 32-bit floating point
    F32,
    /// 64-bit floating point
    F64,
    /// Null-terminated string
    String,
    /// Fixed-size byte array
    Bytes,
    /// Date (year, month, day)
    Date,
    /// DateTime (seconds since epoch)
    DateTime,
    /// Custom type with custom encoder
    Custom,
}

/// Trait for encoding/decoding and comparing keys in the static B+tree.
///
/// Implementers of this trait provide methods to convert keys to and from byte representation
/// with fixed width for efficient storage and retrieval in B+tree nodes.
pub trait KeyEncoder<T> {
    /// Returns the fixed size of encoded keys in bytes.
    ///
    /// This is critical for B+tree nodes where keys must have consistent sizes.
    fn encoded_size(&self) -> usize;

    /// Encodes a key into bytes with fixed width.
    ///
    /// The returned byte vector will always have a length equal to `encoded_size()`.
    fn encode(&self, key: &T) -> Result<Vec<u8>>;

    /// Decodes a key from bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the byte slice is too small or the content is invalid.
    fn decode(&self, bytes: &[u8]) -> Result<T>;

    /// Compares two encoded keys.
    ///
    /// Used for binary search and maintaining order within B+tree nodes.
    fn compare(&self, a: &[u8], b: &[u8]) -> Ordering;
}

//
// Integer key encoders
//

/// Integer key encoder (i64)
#[derive(Debug, Clone)]
pub struct I64KeyEncoder;

impl KeyEncoder<i64> for I64KeyEncoder {
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

/// Integer key encoder (i32)
#[derive(Debug, Clone)]
pub struct I32KeyEncoder;

impl KeyEncoder<i32> for I32KeyEncoder {
    fn encoded_size(&self) -> usize {
        4
    }

    fn encode(&self, key: &i32) -> Result<Vec<u8>> {
        // Encode integer as fixed 4 bytes in little-endian order
        let mut result = Vec::with_capacity(4);
        result.extend_from_slice(&key.to_le_bytes());
        Ok(result)
    }

    fn decode(&self, bytes: &[u8]) -> Result<i32> {
        // Decode integer from 4 bytes in little-endian order
        if bytes.len() < 4 {
            return Err(KeyError::InvalidSize {
                expected: 4,
                actual: bytes.len(),
            }
            .into());
        }

        let value = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
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

/// Integer key encoder for i16
#[derive(Debug, Clone)]
pub struct I16KeyEncoder;

impl KeyEncoder<i16> for I16KeyEncoder {
    fn encoded_size(&self) -> usize {
        2
    }

    fn encode(&self, key: &i16) -> Result<Vec<u8>> {
        // Encode integer as fixed 2 bytes in little-endian order
        let mut result = Vec::with_capacity(2);
        result.extend_from_slice(&key.to_le_bytes());
        Ok(result)
    }

    fn decode(&self, bytes: &[u8]) -> Result<i16> {
        // Decode integer from 2 bytes in little-endian order
        if bytes.len() < 2 {
            return Err(KeyError::InvalidSize {
                expected: 2,
                actual: bytes.len(),
            }
            .into());
        }

        let value = i16::from_le_bytes([bytes[0], bytes[1]]);
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

/// Integer key encoder for i8
#[derive(Debug, Clone)]
pub struct I8KeyEncoder;

impl KeyEncoder<i8> for I8KeyEncoder {
    fn encoded_size(&self) -> usize {
        1
    }

    fn encode(&self, key: &i8) -> Result<Vec<u8>> {
        // Encode integer as fixed 1 byte
        Ok(vec![*key as u8])
    }

    fn decode(&self, bytes: &[u8]) -> Result<i8> {
        // Decode integer from 1 byte
        if bytes.is_empty() {
            return Err(KeyError::InvalidSize {
                expected: 1,
                actual: bytes.len(),
            }
            .into());
        }

        Ok(bytes[0] as i8)
    }

    fn compare(&self, a: &[u8], b: &[u8]) -> Ordering {
        // Compare two encoded integers
        match (self.decode(a), self.decode(b)) {
            (Ok(a_val), Ok(b_val)) => a_val.cmp(&b_val),
            _ => Ordering::Equal, // Default in case of error
        }
    }
}

/// Integer key encoder for u64
#[derive(Debug, Clone)]
pub struct U64KeyEncoder;

impl KeyEncoder<u64> for U64KeyEncoder {
    fn encoded_size(&self) -> usize {
        8
    }

    fn encode(&self, key: &u64) -> Result<Vec<u8>> {
        // Encode integer as fixed 8 bytes in little-endian order
        let mut result = Vec::with_capacity(8);
        result.extend_from_slice(&key.to_le_bytes());
        Ok(result)
    }

    fn decode(&self, bytes: &[u8]) -> Result<u64> {
        // Decode integer from 8 bytes in little-endian order
        if bytes.len() < 8 {
            return Err(KeyError::InvalidSize {
                expected: 8,
                actual: bytes.len(),
            }
            .into());
        }

        let value = u64::from_le_bytes([
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

/// Factory functions for creating key encoders
pub struct KeyEncoderFactory;

impl KeyEncoderFactory {
    /// Creates a key encoder for the given key type
    pub fn for_type<T>(key_type: KeyType) -> Box<dyn KeyEncoder<T>> {
        match key_type {
            KeyType::I8 => Box::new(I8KeyEncoder) as Box<dyn KeyEncoder<T>>,
            KeyType::I16 => Box::new(I16KeyEncoder) as Box<dyn KeyEncoder<T>>,
            KeyType::I32 => Box::new(I32KeyEncoder) as Box<dyn KeyEncoder<T>>,
            KeyType::I64 => Box::new(I64KeyEncoder) as Box<dyn KeyEncoder<T>>,
            KeyType::U8 => Box::new(U8KeyEncoder) as Box<dyn KeyEncoder<T>>,
            KeyType::U16 => Box::new(U16KeyEncoder) as Box<dyn KeyEncoder<T>>,
            KeyType::U32 => Box::new(U32KeyEncoder) as Box<dyn KeyEncoder<T>>,
            KeyType::U64 => Box::new(U64KeyEncoder) as Box<dyn KeyEncoder<T>>,
            _ => panic!("Unsupported key type for generic factory method"),
        }
    }

    /// Creates an i8 key encoder
    pub fn i8() -> Box<dyn KeyEncoder<i8>> {
        Box::new(I8KeyEncoder)
    }

    /// Creates an i16 key encoder
    pub fn i16() -> Box<dyn KeyEncoder<i16>> {
        Box::new(I16KeyEncoder)
    }

    /// Creates an i32 key encoder
    pub fn i32() -> Box<dyn KeyEncoder<i32>> {
        Box::new(I32KeyEncoder)
    }

    /// Creates an i64 key encoder
    pub fn i64() -> Box<dyn KeyEncoder<i64>> {
        Box::new(I64KeyEncoder)
    }

    /// Creates a u8 key encoder
    pub fn u8() -> Box<dyn KeyEncoder<u8>> {
        Box::new(U8KeyEncoder)
    }

    /// Creates a u16 key encoder
    pub fn u16() -> Box<dyn KeyEncoder<u16>> {
        Box::new(U16KeyEncoder)
    }

    /// Creates a u32 key encoder
    pub fn u32() -> Box<dyn KeyEncoder<u32>> {
        Box::new(U32KeyEncoder)
    }

    /// Creates a u64 key encoder
    pub fn u64() -> Box<dyn KeyEncoder<u64>> {
        Box::new(U64KeyEncoder)
    }
}

// Additional unsigned integer encoders

/// Unsigned 8-bit integer key encoder
#[derive(Debug, Clone)]
pub struct U8KeyEncoder;

impl KeyEncoder<u8> for U8KeyEncoder {
    fn encoded_size(&self) -> usize {
        1
    }

    fn encode(&self, key: &u8) -> Result<Vec<u8>> {
        Ok(vec![*key])
    }

    fn decode(&self, bytes: &[u8]) -> Result<u8> {
        if bytes.is_empty() {
            return Err(KeyError::InvalidSize {
                expected: 1,
                actual: bytes.len(),
            }
            .into());
        }
        Ok(bytes[0])
    }

    fn compare(&self, a: &[u8], b: &[u8]) -> Ordering {
        match (self.decode(a), self.decode(b)) {
            (Ok(a_val), Ok(b_val)) => a_val.cmp(&b_val),
            _ => Ordering::Equal,
        }
    }
}

/// Unsigned 16-bit integer key encoder
#[derive(Debug, Clone)]
pub struct U16KeyEncoder;

impl KeyEncoder<u16> for U16KeyEncoder {
    fn encoded_size(&self) -> usize {
        2
    }

    fn encode(&self, key: &u16) -> Result<Vec<u8>> {
        let mut result = Vec::with_capacity(2);
        result.extend_from_slice(&key.to_le_bytes());
        Ok(result)
    }

    fn decode(&self, bytes: &[u8]) -> Result<u16> {
        if bytes.len() < 2 {
            return Err(KeyError::InvalidSize {
                expected: 2,
                actual: bytes.len(),
            }
            .into());
        }
        let value = u16::from_le_bytes([bytes[0], bytes[1]]);
        Ok(value)
    }

    fn compare(&self, a: &[u8], b: &[u8]) -> Ordering {
        match (self.decode(a), self.decode(b)) {
            (Ok(a_val), Ok(b_val)) => a_val.cmp(&b_val),
            _ => Ordering::Equal,
        }
    }
}

/// Unsigned 32-bit integer key encoder
#[derive(Debug, Clone)]
pub struct U32KeyEncoder;

impl KeyEncoder<u32> for U32KeyEncoder {
    fn encoded_size(&self) -> usize {
        4
    }

    fn encode(&self, key: &u32) -> Result<Vec<u8>> {
        let mut result = Vec::with_capacity(4);
        result.extend_from_slice(&key.to_le_bytes());
        Ok(result)
    }

    fn decode(&self, bytes: &[u8]) -> Result<u32> {
        if bytes.len() < 4 {
            return Err(KeyError::InvalidSize {
                expected: 4,
                actual: bytes.len(),
            }
            .into());
        }
        let value = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        Ok(value)
    }

    fn compare(&self, a: &[u8], b: &[u8]) -> Ordering {
        match (self.decode(a), self.decode(b)) {
            (Ok(a_val), Ok(b_val)) => a_val.cmp(&b_val),
            _ => Ordering::Equal,
        }
    }
}
