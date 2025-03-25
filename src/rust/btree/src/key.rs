// Key encoding/decoding for B-tree indexes
//
// This module provides type-safe encoders for storing different key types in a B-tree.
// The encoders convert various types to fixed-width binary representations for efficient storage.
// Each encoder guarantees consistent binary representation and proper ordering semantics.

use crate::errors::{BTreeError, KeyError, Result};
use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, Utc};
use std::cmp::Ordering;

/// Trait for encoding/decoding and comparing keys in the B-tree.
///
/// Implementers of this trait provide methods to convert keys to and from byte representation
/// with fixed width for efficient storage and retrieval in B-tree nodes.
pub trait KeyEncoder<T> {
    /// Returns the fixed size of encoded keys in bytes.
    ///
    /// This is critical for B-tree nodes where keys must have consistent sizes.
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
    /// Used for binary search and maintaining order within B-tree nodes.
    fn compare(&self, a: &[u8], b: &[u8]) -> Ordering;
}

/// Integer key encoder (i64)
#[derive(Debug, Clone)]
struct I64KeyEncoder;

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
struct I32KeyEncoder;

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
struct I16KeyEncoder;

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
struct I8KeyEncoder;

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
struct U64KeyEncoder;

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

/// Integer key encoder for u32
#[derive(Debug, Clone)]
struct U32KeyEncoder;

impl KeyEncoder<u32> for U32KeyEncoder {
    fn encoded_size(&self) -> usize {
        4
    }

    fn encode(&self, key: &u32) -> Result<Vec<u8>> {
        // Encode integer as fixed 4 bytes in little-endian order
        let mut result = Vec::with_capacity(4);
        result.extend_from_slice(&key.to_le_bytes());
        Ok(result)
    }

    fn decode(&self, bytes: &[u8]) -> Result<u32> {
        // Decode integer from 4 bytes in little-endian order
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
        // Compare two encoded integers
        match (self.decode(a), self.decode(b)) {
            (Ok(a_val), Ok(b_val)) => a_val.cmp(&b_val),
            _ => Ordering::Equal, // Default in case of error
        }
    }
}

/// Integer key encoder for u16
#[derive(Debug, Clone)]
struct U16KeyEncoder;

impl KeyEncoder<u16> for U16KeyEncoder {
    fn encoded_size(&self) -> usize {
        2
    }

    fn encode(&self, key: &u16) -> Result<Vec<u8>> {
        // Encode integer as fixed 2 bytes in little-endian order
        let mut result = Vec::with_capacity(2);
        result.extend_from_slice(&key.to_le_bytes());
        Ok(result)
    }

    fn decode(&self, bytes: &[u8]) -> Result<u16> {
        // Decode integer from 2 bytes in little-endian order
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
        // Compare two encoded integers
        match (self.decode(a), self.decode(b)) {
            (Ok(a_val), Ok(b_val)) => a_val.cmp(&b_val),
            _ => Ordering::Equal, // Default in case of error
        }
    }
}

/// Integer key encoder for u8
#[derive(Debug, Clone)]
struct U8KeyEncoder;

impl KeyEncoder<u8> for U8KeyEncoder {
    fn encoded_size(&self) -> usize {
        1
    }

    fn encode(&self, key: &u8) -> Result<Vec<u8>> {
        // Encode integer as fixed 1 byte
        Ok(vec![*key])
    }

    fn decode(&self, bytes: &[u8]) -> Result<u8> {
        // Decode integer from 1 byte
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
        // Compare two encoded integers
        match (self.decode(a), self.decode(b)) {
            (Ok(a_val), Ok(b_val)) => a_val.cmp(&b_val),
            _ => Ordering::Equal, // Default in case of error
        }
    }
}

/// Boolean key encoder
#[derive(Debug, Clone)]
struct BoolKeyEncoder;

impl KeyEncoder<bool> for BoolKeyEncoder {
    fn encoded_size(&self) -> usize {
        1
    }

    fn encode(&self, key: &bool) -> Result<Vec<u8>> {
        // Encode bool as a single byte: 1 for true, 0 for false
        Ok(vec![if *key { 1u8 } else { 0u8 }])
    }

    fn decode(&self, bytes: &[u8]) -> Result<bool> {
        // Decode bool from a single byte
        if bytes.is_empty() {
            return Err(KeyError::InvalidSize {
                expected: 1,
                actual: bytes.len(),
            }
            .into());
        }

        Ok(bytes[0] != 0)
    }

    fn compare(&self, a: &[u8], b: &[u8]) -> Ordering {
        // Compare two encoded booleans
        match (self.decode(a), self.decode(b)) {
            (Ok(a_val), Ok(b_val)) => a_val.cmp(&b_val),
            _ => Ordering::Equal, // Default in case of error
        }
    }
}

/// String key encoder with fixed prefix
#[derive(Debug, Clone)]
struct StringKeyEncoder {
    /// Length of the prefix to use for string keys
    prefix_length: usize,
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
#[derive(Debug, Clone)]
struct FloatKeyEncoder;

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

/// Float32 key encoder with NaN handling
#[derive(Debug, Clone)]
struct F32KeyEncoder;

impl KeyEncoder<f32> for F32KeyEncoder {
    fn encoded_size(&self) -> usize {
        4
    }

    fn encode(&self, key: &f32) -> Result<Vec<u8>> {
        // Encode float with proper NaN handling
        let bits = if key.is_nan() {
            // Handle NaN: Use a specific bit pattern
            u32::MAX
        } else {
            key.to_bits()
        };

        let mut result = Vec::with_capacity(4);
        result.extend_from_slice(&bits.to_le_bytes());
        Ok(result)
    }

    fn decode(&self, bytes: &[u8]) -> Result<f32> {
        // Decode float from 4 bytes with proper NaN handling
        if bytes.len() < 4 {
            return Err(KeyError::InvalidSize {
                expected: 4,
                actual: bytes.len(),
            }
            .into());
        }

        let bits = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);

        let value = if bits == u32::MAX {
            // Special case for NaN
            f32::NAN
        } else {
            f32::from_bits(bits)
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

/// Date encoder for NaiveDate
#[derive(Debug, Clone)]
struct NaiveDateKeyEncoder;

impl KeyEncoder<NaiveDate> for NaiveDateKeyEncoder {
    fn encoded_size(&self) -> usize {
        12 // 4 bytes for year, 4 for month, 4 for day
    }

    fn encode(&self, key: &NaiveDate) -> Result<Vec<u8>> {
        let mut result = Vec::with_capacity(12);
        result.extend_from_slice(&key.year().to_le_bytes());
        result.extend_from_slice(&key.month().to_le_bytes());
        result.extend_from_slice(&key.day().to_le_bytes());
        Ok(result)
    }

    fn decode(&self, bytes: &[u8]) -> Result<NaiveDate> {
        if bytes.len() < 12 {
            return Err(KeyError::InvalidSize {
                expected: 12,
                actual: bytes.len(),
            }
            .into());
        }

        let year = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let month = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        let day = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);

        NaiveDate::from_ymd_opt(year, month, day)
            .ok_or_else(|| KeyError::Decoding("invalid date".to_string()).into())
    }

    fn compare(&self, a: &[u8], b: &[u8]) -> Ordering {
        match (self.decode(a), self.decode(b)) {
            (Ok(a_val), Ok(b_val)) => a_val.cmp(&b_val),
            _ => Ordering::Equal, // Default in case of error
        }
    }
}

/// DateTime encoder for NaiveDateTime (timestamp seconds + nanoseconds)
#[derive(Debug, Clone)]
struct NaiveDateTimeKeyEncoder;

impl KeyEncoder<NaiveDateTime> for NaiveDateTimeKeyEncoder {
    fn encoded_size(&self) -> usize {
        12 // 8 bytes for timestamp seconds, 4 for nanoseconds
    }

    fn encode(&self, key: &NaiveDateTime) -> Result<Vec<u8>> {
        let mut result = Vec::with_capacity(12);
        // Convert to timestamp and encode seconds + nanoseconds
        let timestamp = key.and_utc().timestamp();
        let nano = key.and_utc().timestamp_subsec_nanos();

        result.extend_from_slice(&timestamp.to_le_bytes());
        result.extend_from_slice(&nano.to_le_bytes());
        Ok(result)
    }

    fn decode(&self, bytes: &[u8]) -> Result<NaiveDateTime> {
        if bytes.len() < 12 {
            return Err(KeyError::InvalidSize {
                expected: 12,
                actual: bytes.len(),
            }
            .into());
        }

        let timestamp = i64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]);
        let nano = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);

        match NaiveDateTime::from_timestamp_opt(timestamp, nano) {
            Some(dt) => Ok(dt),
            None => Err(KeyError::Decoding("invalid datetime".to_string()).into()),
        }
    }

    fn compare(&self, a: &[u8], b: &[u8]) -> Ordering {
        match (self.decode(a), self.decode(b)) {
            (Ok(a_val), Ok(b_val)) => a_val.cmp(&b_val),
            _ => Ordering::Equal, // Default in case of error
        }
    }
}

/// DateTime encoder for DateTime<Utc> (same as NaiveDateTime internally)
#[derive(Debug, Clone)]
struct DateTimeKeyEncoder;

impl KeyEncoder<DateTime<Utc>> for DateTimeKeyEncoder {
    fn encoded_size(&self) -> usize {
        12 // 8 bytes for timestamp seconds, 4 for nanoseconds
    }

    fn encode(&self, key: &DateTime<Utc>) -> Result<Vec<u8>> {
        let mut result = Vec::with_capacity(12);
        // Convert to timestamp and encode seconds + nanoseconds
        let timestamp = key.timestamp();
        let nano = key.timestamp_subsec_nanos();

        result.extend_from_slice(&timestamp.to_le_bytes());
        result.extend_from_slice(&nano.to_le_bytes());
        Ok(result)
    }

    fn decode(&self, bytes: &[u8]) -> Result<DateTime<Utc>> {
        if bytes.len() < 12 {
            return Err(KeyError::InvalidSize {
                expected: 12,
                actual: bytes.len(),
            }
            .into());
        }

        let timestamp = i64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]);
        let nano = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);

        match DateTime::from_timestamp(timestamp, nano) {
            Some(dt) => Ok(dt),
            None => Err(KeyError::Decoding("invalid utc datetime".to_string()).into()),
        }
    }

    fn compare(&self, a: &[u8], b: &[u8]) -> Ordering {
        match (self.decode(a), self.decode(b)) {
            (Ok(a_val), Ok(b_val)) => a_val.cmp(&b_val),
            _ => Ordering::Equal, // Default in case of error
        }
    }
}

/// Enum wrapper for all supported key encoder types that provides a unified interface.
///
/// This type allows for dynamic selection of key encoders while maintaining type safety.
/// Use the factory methods (e.g., `i64()`, `string()`) to create specific encoders.
///
/// # Examples
///
/// ```
/// use btree::key::{AnyKeyEncoder, KeyType};
///
/// // Create an encoder for i64 values
/// let encoder = AnyKeyEncoder::i64();
///
/// // Encode a key
/// let key = KeyType::I64(42);
/// let encoded = encoder.encode(&key).unwrap();
///
/// // Decode the key
/// let decoded = encoder.decode(&encoded).unwrap();
/// assert!(matches!(decoded, KeyType::I64(42)));
/// ```
#[derive(Debug, Clone)]
pub enum AnyKeyEncoder {
    /// Integer (i64) key encoder
    I64(I64KeyEncoder),
    /// i32 integer key encoder
    I32(I32KeyEncoder),
    /// i16 integer key encoder
    I16(I16KeyEncoder),
    /// i8 integer key encoder
    I8(I8KeyEncoder),
    /// u64 unsigned integer key encoder
    U64(U64KeyEncoder),
    /// u32 unsigned integer key encoder
    U32(U32KeyEncoder),
    /// u16 unsigned integer key encoder
    U16(U16KeyEncoder),
    /// u8 unsigned integer key encoder
    U8(U8KeyEncoder),
    /// f64 floating point key encoder with NaN handling
    F64(FloatKeyEncoder),
    /// f32 floating point key encoder with NaN handling
    F32(F32KeyEncoder),
    /// Boolean key encoder
    Bool(BoolKeyEncoder),
    /// String key encoder with a specific prefix length
    String(StringKeyEncoder),
    /// Naive date key encoder
    NaiveDate(NaiveDateKeyEncoder),
    /// Naive datetime key encoder
    NaiveDateTime(NaiveDateTimeKeyEncoder),
    /// UTC datetime key encoder
    DateTime(DateTimeKeyEncoder),
}

/// Helper type to represent any encodable key type.
///
/// This enum provides a unified representation for all key types that can be used
/// with the `AnyKeyEncoder`. It allows for dynamic type selection while maintaining
/// type safety.
///
/// Note: This type implements `PartialEq` but not `Eq` because floating-point types
/// don't satisfy the requirements for `Eq` due to NaN comparisons.
#[derive(Debug, Clone, PartialEq)]
pub enum KeyType {
    /// Integer (i64)
    I64(i64),
    /// i32 integer
    I32(i32),
    /// i16 integer
    I16(i16),
    /// i8 integer
    I8(i8),
    /// u64 unsigned integer
    U64(u64),
    /// u32 unsigned integer
    U32(u32),
    /// u16 unsigned integer
    U16(u16),
    /// u8 unsigned integer
    U8(u8),
    /// f64 floating point
    F64(f64),
    /// f32 floating point
    F32(f32),
    /// Boolean
    Bool(bool),
    /// String
    String(String),
    /// NaiveDate
    NaiveDate(NaiveDate),
    /// NaiveDateTime
    NaiveDateTime(NaiveDateTime),
    /// UTC DateTime
    DateTime(DateTime<Utc>),
}

/// Factory methods for AnyKeyEncoder
impl AnyKeyEncoder {
    /// Create a new integer key encoder for i64 values.
    ///
    /// # Examples
    ///
    /// ```
    /// use btree::key::AnyKeyEncoder;
    ///
    /// let encoder = AnyKeyEncoder::i64();
    /// ```
    pub fn i64() -> Self {
        Self::I64(I64KeyEncoder)
    }

    /// Create a new i32 key encoder
    pub fn i32() -> Self {
        Self::I32(I32KeyEncoder)
    }

    /// Create a new i16 key encoder
    pub fn i16() -> Self {
        Self::I16(I16KeyEncoder)
    }

    /// Create a new i8 key encoder
    pub fn i8() -> Self {
        Self::I8(I8KeyEncoder)
    }

    /// Create a new u64 key encoder
    pub fn u64() -> Self {
        Self::U64(U64KeyEncoder)
    }

    /// Create a new u32 key encoder
    pub fn u32() -> Self {
        Self::U32(U32KeyEncoder)
    }

    /// Create a new u16 key encoder
    pub fn u16() -> Self {
        Self::U16(U16KeyEncoder)
    }

    /// Create a new u8 key encoder
    pub fn u8() -> Self {
        Self::U8(U8KeyEncoder)
    }

    /// Create a new f64 key encoder
    pub fn f64() -> Self {
        Self::F64(FloatKeyEncoder)
    }

    /// Create a new f32 key encoder
    pub fn f32() -> Self {
        Self::F32(F32KeyEncoder)
    }

    /// Create a new boolean key encoder
    pub fn bool() -> Self {
        Self::Bool(BoolKeyEncoder)
    }

    /// Create a new string key encoder with a specific prefix length.
    ///
    /// If the prefix_length is None, a default of 10 bytes is used.
    /// String keys longer than the prefix length will be truncated.
    pub fn string(prefix_length: Option<usize>) -> Self {
        Self::String(StringKeyEncoder {
            prefix_length: prefix_length.unwrap_or(10),
        })
    }

    /// Create a new naive date key encoder
    pub fn naive_date() -> Self {
        Self::NaiveDate(NaiveDateKeyEncoder)
    }

    /// Create a new naive datetime key encoder
    pub fn naive_datetime() -> Self {
        Self::NaiveDateTime(NaiveDateTimeKeyEncoder)
    }

    /// Create a new UTC datetime key encoder
    pub fn datetime() -> Self {
        Self::DateTime(DateTimeKeyEncoder)
    }
}

/// Implementation of KeyEncoder trait for AnyKeyEncoder.
///
/// This allows users to use AnyKeyEncoder directly with APIs that require KeyEncoder,
/// making the interface more consistent and user-friendly.
impl KeyEncoder<KeyType> for AnyKeyEncoder {
    fn encoded_size(&self) -> usize {
        // Delegate to the inner encoder
        match self {
            Self::I64(encoder) => encoder.encoded_size(),
            Self::I32(encoder) => encoder.encoded_size(),
            Self::I16(encoder) => encoder.encoded_size(),
            Self::I8(encoder) => encoder.encoded_size(),
            Self::U64(encoder) => encoder.encoded_size(),
            Self::U32(encoder) => encoder.encoded_size(),
            Self::U16(encoder) => encoder.encoded_size(),
            Self::U8(encoder) => encoder.encoded_size(),
            Self::F64(encoder) => encoder.encoded_size(),
            Self::F32(encoder) => encoder.encoded_size(),
            Self::Bool(encoder) => encoder.encoded_size(),
            Self::String(encoder) => encoder.encoded_size(),
            Self::NaiveDate(encoder) => encoder.encoded_size(),
            Self::NaiveDateTime(encoder) => encoder.encoded_size(),
            Self::DateTime(encoder) => encoder.encoded_size(),
        }
    }

    fn encode(&self, key: &KeyType) -> Result<Vec<u8>> {
        // Type-check and encode the key
        match (self, key) {
            (Self::I64(encoder), KeyType::I64(value)) => encoder.encode(value),
            (Self::I32(encoder), KeyType::I32(value)) => encoder.encode(value),
            (Self::I16(encoder), KeyType::I16(value)) => encoder.encode(value),
            (Self::I8(encoder), KeyType::I8(value)) => encoder.encode(value),
            (Self::U64(encoder), KeyType::U64(value)) => encoder.encode(value),
            (Self::U32(encoder), KeyType::U32(value)) => encoder.encode(value),
            (Self::U16(encoder), KeyType::U16(value)) => encoder.encode(value),
            (Self::U8(encoder), KeyType::U8(value)) => encoder.encode(value),
            (Self::F64(encoder), KeyType::F64(value)) => encoder.encode(value),
            (Self::F32(encoder), KeyType::F32(value)) => encoder.encode(value),
            (Self::Bool(encoder), KeyType::Bool(value)) => encoder.encode(value),
            (Self::String(encoder), KeyType::String(value)) => encoder.encode(value),
            (Self::NaiveDate(encoder), KeyType::NaiveDate(value)) => encoder.encode(value),
            (Self::NaiveDateTime(encoder), KeyType::NaiveDateTime(value)) => encoder.encode(value),
            (Self::DateTime(encoder), KeyType::DateTime(value)) => encoder.encode(value),
            _ => Err(BTreeError::TypeMismatch {
                expected: format!("{:?}", self),
                actual: format!("{:?}", key),
            }),
        }
    }

    fn decode(&self, bytes: &[u8]) -> Result<KeyType> {
        // Decode to the appropriate KeyType variant
        match self {
            Self::I64(encoder) => encoder.decode(bytes).map(KeyType::I64),
            Self::I32(encoder) => encoder.decode(bytes).map(KeyType::I32),
            Self::I16(encoder) => encoder.decode(bytes).map(KeyType::I16),
            Self::I8(encoder) => encoder.decode(bytes).map(KeyType::I8),
            Self::U64(encoder) => encoder.decode(bytes).map(KeyType::U64),
            Self::U32(encoder) => encoder.decode(bytes).map(KeyType::U32),
            Self::U16(encoder) => encoder.decode(bytes).map(KeyType::U16),
            Self::U8(encoder) => encoder.decode(bytes).map(KeyType::U8),
            Self::F64(encoder) => encoder.decode(bytes).map(KeyType::F64),
            Self::F32(encoder) => encoder.decode(bytes).map(KeyType::F32),
            Self::Bool(encoder) => encoder.decode(bytes).map(KeyType::Bool),
            Self::String(encoder) => encoder.decode(bytes).map(KeyType::String),
            Self::NaiveDate(encoder) => encoder.decode(bytes).map(KeyType::NaiveDate),
            Self::NaiveDateTime(encoder) => encoder.decode(bytes).map(KeyType::NaiveDateTime),
            Self::DateTime(encoder) => encoder.decode(bytes).map(KeyType::DateTime),
        }
    }

    fn compare(&self, a: &[u8], b: &[u8]) -> Ordering {
        // Delegate comparison to the inner encoder
        match self {
            Self::I64(encoder) => encoder.compare(a, b),
            Self::I32(encoder) => encoder.compare(a, b),
            Self::I16(encoder) => encoder.compare(a, b),
            Self::I8(encoder) => encoder.compare(a, b),
            Self::U64(encoder) => encoder.compare(a, b),
            Self::U32(encoder) => encoder.compare(a, b),
            Self::U16(encoder) => encoder.compare(a, b),
            Self::U8(encoder) => encoder.compare(a, b),
            Self::F64(encoder) => encoder.compare(a, b),
            Self::F32(encoder) => encoder.compare(a, b),
            Self::Bool(encoder) => encoder.compare(a, b),
            Self::String(encoder) => encoder.compare(a, b),
            Self::NaiveDate(encoder) => encoder.compare(a, b),
            Self::NaiveDateTime(encoder) => encoder.compare(a, b),
            Self::DateTime(encoder) => encoder.compare(a, b),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, NaiveDate, NaiveDateTime};

    #[test]
    fn test_i64_encoder() {
        println!("testing i64 encoder...");
        let encoder = I64KeyEncoder;
        let val = 42i64;
        let encoded = encoder.encode(&val).unwrap();
        let decoded = encoder.decode(&encoded).unwrap();
        assert_eq!(val, decoded);
        println!("i64 encoder passed");
    }

    #[test]
    fn test_i32_encoder() {
        println!("testing i32 encoder...");
        let encoder = I32KeyEncoder;
        let val = 42i32;
        let encoded = encoder.encode(&val).unwrap();
        let decoded = encoder.decode(&encoded).unwrap();
        assert_eq!(val, decoded);
        println!("i32 encoder passed");
    }

    #[test]
    fn test_i16_encoder() {
        println!("testing i16 encoder...");
        let encoder = I16KeyEncoder;
        let val = 42i16;
        let encoded = encoder.encode(&val).unwrap();
        let decoded = encoder.decode(&encoded).unwrap();
        assert_eq!(val, decoded);
        println!("i16 encoder passed");
    }

    #[test]
    fn test_i8_encoder() {
        println!("testing i8 encoder...");
        let encoder = I8KeyEncoder;
        let val = 42i8;
        let encoded = encoder.encode(&val).unwrap();
        let decoded = encoder.decode(&encoded).unwrap();
        assert_eq!(val, decoded);
        println!("i8 encoder passed");
    }

    #[test]
    fn test_bool_encoder() {
        println!("testing bool encoder...");
        let encoder = BoolKeyEncoder;
        let val = true;
        let encoded = encoder.encode(&val).unwrap();
        let decoded = encoder.decode(&encoded).unwrap();
        assert_eq!(val, decoded);

        let val = false;
        let encoded = encoder.encode(&val).unwrap();
        let decoded = encoder.decode(&encoded).unwrap();
        assert_eq!(val, decoded);
        println!("bool encoder passed");
    }

    #[test]
    fn test_string_encoder() {
        println!("testing string encoder...");
        let encoder = StringKeyEncoder { prefix_length: 10 };
        let val = "test".to_string();
        let encoded = encoder.encode(&val).unwrap();
        let decoded = encoder.decode(&encoded).unwrap();
        assert_eq!(val, decoded);

        // Test prefix truncation
        let long_val = "this is a long string that should be truncated".to_string();
        let encoded = encoder.encode(&long_val).unwrap();
        let decoded = encoder.decode(&encoded).unwrap();
        assert_eq!(&decoded, "this is a ");
        println!("string encoder passed");
    }

    #[test]
    fn test_float_encoder() {
        println!("testing f64 encoder...");
        let encoder = FloatKeyEncoder;
        let val = std::f64::consts::PI;
        let encoded = encoder.encode(&val).unwrap();
        let decoded = encoder.decode(&encoded).unwrap();
        assert_eq!(val, decoded);
        println!("f64 encoder passed");
    }

    #[test]
    fn test_date_encoder() {
        println!("testing date encoder...");
        let encoder = NaiveDateKeyEncoder;
        let val = NaiveDate::from_ymd_opt(2023, 5, 15).unwrap();
        let encoded = encoder.encode(&val).unwrap();
        let decoded = encoder.decode(&encoded).unwrap();
        assert_eq!(val, decoded);
        println!("date encoder passed");
    }

    #[test]
    fn test_datetime_encoder() {
        println!("testing datetime encoder...");
        let encoder = DateTimeKeyEncoder;
        let val = DateTime::from_timestamp(1716153600, 0).unwrap();
        let encoded = encoder.encode(&val).unwrap();
        let decoded = encoder.decode(&encoded).unwrap();
        assert_eq!(val, decoded);
        println!("datetime encoder passed");
    }

    #[test]
    fn test_naive_datetime_encoder() {
        println!("testing naive datetime encoder...");
        let encoder = NaiveDateTimeKeyEncoder;
        let val = NaiveDateTime::from_timestamp(1716153600, 0);
        let encoded = encoder.encode(&val).unwrap();
        let decoded = encoder.decode(&encoded).unwrap();
        assert_eq!(val, decoded);
        println!("naive datetime encoder passed");
    }

    #[test]
    fn test_any_key_encoder_as_encoder() {
        // Test using AnyKeyEncoder as a KeyEncoder implementation
        use super::*;

        // Create AnyKeyEncoder
        let encoder = AnyKeyEncoder::i64();

        // Create a key
        let key = KeyType::I64(42);

        // Encode
        let encoded = encoder.encode(&key).unwrap();

        // Decode
        let decoded = encoder.decode(&encoded).unwrap();

        // Verify
        match decoded {
            KeyType::I64(value) => assert_eq!(value, 42),
            _ => panic!("Decoded to wrong type"),
        }

        // Test comparison
        let key1 = KeyType::I64(10);
        let key2 = KeyType::I64(20);

        let encoded1 = encoder.encode(&key1).unwrap();
        let encoded2 = encoder.encode(&key2).unwrap();

        assert_eq!(encoder.compare(&encoded1, &encoded2), Ordering::Less);
        assert_eq!(encoder.compare(&encoded2, &encoded1), Ordering::Greater);
        assert_eq!(encoder.compare(&encoded1, &encoded1), Ordering::Equal);
    }
}
