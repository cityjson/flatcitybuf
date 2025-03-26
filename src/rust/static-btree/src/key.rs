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
    pub fn for_type<T: 'static>(key_type: KeyType) -> Box<dyn KeyEncoder<T>> {
        match key_type {
            KeyType::I8 => {
                if std::any::TypeId::of::<T>() == std::any::TypeId::of::<i8>() {
                    // Safety: we've verified T is i8
                    unsafe {
                        std::mem::transmute(Box::new(I8KeyEncoder) as Box<dyn KeyEncoder<i8>>)
                    }
                } else {
                    panic!("Type mismatch: expected i8")
                }
            }
            KeyType::I16 => {
                if std::any::TypeId::of::<T>() == std::any::TypeId::of::<i16>() {
                    unsafe {
                        std::mem::transmute(Box::new(I16KeyEncoder) as Box<dyn KeyEncoder<i16>>)
                    }
                } else {
                    panic!("Type mismatch: expected i16")
                }
            }
            KeyType::I32 => {
                if std::any::TypeId::of::<T>() == std::any::TypeId::of::<i32>() {
                    unsafe {
                        std::mem::transmute(Box::new(I32KeyEncoder) as Box<dyn KeyEncoder<i32>>)
                    }
                } else {
                    panic!("Type mismatch: expected i32")
                }
            }
            KeyType::I64 => {
                if std::any::TypeId::of::<T>() == std::any::TypeId::of::<i64>() {
                    unsafe {
                        std::mem::transmute(Box::new(I64KeyEncoder) as Box<dyn KeyEncoder<i64>>)
                    }
                } else {
                    panic!("Type mismatch: expected i64")
                }
            }
            KeyType::U8 => {
                if std::any::TypeId::of::<T>() == std::any::TypeId::of::<u8>() {
                    unsafe {
                        std::mem::transmute(Box::new(U8KeyEncoder) as Box<dyn KeyEncoder<u8>>)
                    }
                } else {
                    panic!("Type mismatch: expected u8")
                }
            }
            KeyType::U16 => {
                if std::any::TypeId::of::<T>() == std::any::TypeId::of::<u16>() {
                    unsafe {
                        std::mem::transmute(Box::new(U16KeyEncoder) as Box<dyn KeyEncoder<u16>>)
                    }
                } else {
                    panic!("Type mismatch: expected u16")
                }
            }
            KeyType::U32 => {
                if std::any::TypeId::of::<T>() == std::any::TypeId::of::<u32>() {
                    unsafe {
                        std::mem::transmute(Box::new(U32KeyEncoder) as Box<dyn KeyEncoder<u32>>)
                    }
                } else {
                    panic!("Type mismatch: expected u32")
                }
            }
            KeyType::U64 => {
                if std::any::TypeId::of::<T>() == std::any::TypeId::of::<u64>() {
                    unsafe {
                        std::mem::transmute(Box::new(U64KeyEncoder) as Box<dyn KeyEncoder<u64>>)
                    }
                } else {
                    panic!("Type mismatch: expected u64")
                }
            }
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

mod tests {
    use super::*;
    use std::cmp::Ordering;

    #[test]
    fn test_i64_key_encoder() {
        let encoder = KeyEncoderFactory::i64();

        // Test encoding
        let key = 42i64;
        let encoded = encoder.encode(&key).unwrap();
        assert_eq!(encoded.len(), 8);
        assert_eq!(encoded, key.to_le_bytes());

        // Test decoding
        let decoded = encoder.decode(&encoded).unwrap();
        assert_eq!(decoded, key);

        // Test comparison
        let key1 = 10i64;
        let key2 = 20i64;
        let encoded1 = encoder.encode(&key1).unwrap();
        let encoded2 = encoder.encode(&key2).unwrap();

        assert_eq!(encoder.compare(&encoded1, &encoded2), Ordering::Less);
        assert_eq!(encoder.compare(&encoded2, &encoded1), Ordering::Greater);
        assert_eq!(encoder.compare(&encoded1, &encoded1), Ordering::Equal);
    }

    #[test]
    fn test_i32_key_encoder() {
        let encoder = KeyEncoderFactory::i32();

        // Test encoding
        let key = 42i32;
        let encoded = encoder.encode(&key).unwrap();
        assert_eq!(encoded.len(), 4);
        assert_eq!(encoded, key.to_le_bytes());

        // Test decoding
        let decoded = encoder.decode(&encoded).unwrap();
        assert_eq!(decoded, key);

        // Test comparison
        let key1 = 10i32;
        let key2 = 20i32;
        let encoded1 = encoder.encode(&key1).unwrap();
        let encoded2 = encoder.encode(&key2).unwrap();

        assert_eq!(encoder.compare(&encoded1, &encoded2), Ordering::Less);
        assert_eq!(encoder.compare(&encoded2, &encoded1), Ordering::Greater);
        assert_eq!(encoder.compare(&encoded1, &encoded1), Ordering::Equal);
    }

    #[test]
    fn test_i16_key_encoder() {
        let encoder = KeyEncoderFactory::i16();

        // Test encoding
        let key = 42i16;
        let encoded = encoder.encode(&key).unwrap();
        assert_eq!(encoded.len(), 2);
        assert_eq!(encoded, key.to_le_bytes());

        // Test decoding
        let decoded = encoder.decode(&encoded).unwrap();
        assert_eq!(decoded, key);
    }

    #[test]
    fn test_i8_key_encoder() {
        let encoder = KeyEncoderFactory::i8();

        // Test encoding
        let key = 42i8;
        let encoded = encoder.encode(&key).unwrap();
        assert_eq!(encoded.len(), 1);
        assert_eq!(encoded[0], key as u8);

        // Test decoding
        let decoded = encoder.decode(&encoded).unwrap();
        assert_eq!(decoded, key);
    }

    #[test]
    fn test_u64_key_encoder() {
        let encoder = KeyEncoderFactory::u64();

        // Test encoding
        let key = 42u64;
        let encoded = encoder.encode(&key).unwrap();
        assert_eq!(encoded.len(), 8);
        assert_eq!(encoded, key.to_le_bytes());

        // Test decoding
        let decoded = encoder.decode(&encoded).unwrap();
        assert_eq!(decoded, key);
    }

    #[test]
    fn test_u32_key_encoder() {
        let encoder = KeyEncoderFactory::u32();

        // Test encoding
        let key = 42u32;
        let encoded = encoder.encode(&key).unwrap();
        assert_eq!(encoded.len(), 4);
        assert_eq!(encoded, key.to_le_bytes());

        // Test decoding
        let decoded = encoder.decode(&encoded).unwrap();
        assert_eq!(decoded, key);
    }

    #[test]
    fn test_u16_key_encoder() {
        let encoder = KeyEncoderFactory::u16();

        // Test encoding
        let key = 42u16;
        let encoded = encoder.encode(&key).unwrap();
        assert_eq!(encoded.len(), 2);
        assert_eq!(encoded, key.to_le_bytes());

        // Test decoding
        let decoded = encoder.decode(&encoded).unwrap();
        assert_eq!(decoded, key);
    }

    #[test]
    fn test_u8_key_encoder() {
        let encoder = KeyEncoderFactory::u8();

        // Test encoding
        let key = 42u8;
        let encoded = encoder.encode(&key).unwrap();
        assert_eq!(encoded.len(), 1);
        assert_eq!(encoded[0], key);

        // Test decoding
        let decoded = encoder.decode(&encoded).unwrap();
        assert_eq!(decoded, key);
    }

    #[test]
    fn test_error_handling() {
        let encoder = KeyEncoderFactory::i64();

        // Test decoding with too small buffer
        let too_small = vec![1, 2, 3];
        let result = encoder.decode(&too_small);
        assert!(result.is_err());
    }

    #[test]
    fn test_factory_encoder_creation() {
        // Test creating encoders from the factory for different types
        let i8_encoder = KeyEncoderFactory::for_type::<i8>(KeyType::I8);
        let i16_encoder = KeyEncoderFactory::for_type::<i16>(KeyType::I16);
        let i32_encoder = KeyEncoderFactory::for_type::<i32>(KeyType::I32);
        let i64_encoder = KeyEncoderFactory::for_type::<i64>(KeyType::I64);

        assert_eq!(i8_encoder.encoded_size(), 1);
        assert_eq!(i16_encoder.encoded_size(), 2);
        assert_eq!(i32_encoder.encoded_size(), 4);
        assert_eq!(i64_encoder.encoded_size(), 8);
    }
}
