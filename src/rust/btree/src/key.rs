use crate::errors::{KeyError, Result};
use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, Utc};
use ordered_float::OrderedFloat;
use std::cmp::Ordering;

/// Type alias for ordered floating point to ensure total ordering (handles NaN correctly)
pub type Float<T> = OrderedFloat<T>;

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

/// Integer key encoder for i64 numeric types
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

/// Integer key encoder for i32
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

/// Integer key encoder for u32
pub struct U32KeyEncoder;

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
pub struct U16KeyEncoder;

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
pub struct U8KeyEncoder;

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
pub struct BoolKeyEncoder;

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

/// Float32 key encoder with NaN handling
pub struct F32KeyEncoder;

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

/// Ordered Float64 key encoder
pub struct OrderedF64KeyEncoder;

impl KeyEncoder<Float<f64>> for OrderedF64KeyEncoder {
    fn encoded_size(&self) -> usize {
        8
    }

    fn encode(&self, key: &Float<f64>) -> Result<Vec<u8>> {
        // Convert to f64 and encode
        let inner_val = key.0;
        let mut result = Vec::with_capacity(8);
        result.extend_from_slice(&inner_val.to_bits().to_le_bytes());
        Ok(result)
    }

    fn decode(&self, bytes: &[u8]) -> Result<Float<f64>> {
        // Decode float from 8 bytes
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
        let value = f64::from_bits(bits);
        Ok(OrderedFloat(value))
    }

    fn compare(&self, a: &[u8], b: &[u8]) -> Ordering {
        // OrderedFloat implements Ord, so we can directly compare
        match (self.decode(a), self.decode(b)) {
            (Ok(a_val), Ok(b_val)) => a_val.cmp(&b_val),
            _ => Ordering::Equal, // Default in case of error
        }
    }
}

/// Ordered Float32 key encoder
pub struct OrderedF32KeyEncoder;

impl KeyEncoder<Float<f32>> for OrderedF32KeyEncoder {
    fn encoded_size(&self) -> usize {
        4
    }

    fn encode(&self, key: &Float<f32>) -> Result<Vec<u8>> {
        // Convert to f32 and encode
        let inner_val = key.0;
        let mut result = Vec::with_capacity(4);
        result.extend_from_slice(&inner_val.to_bits().to_le_bytes());
        Ok(result)
    }

    fn decode(&self, bytes: &[u8]) -> Result<Float<f32>> {
        // Decode float from 4 bytes
        if bytes.len() < 4 {
            return Err(KeyError::InvalidSize {
                expected: 4,
                actual: bytes.len(),
            }
            .into());
        }

        let bits = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let value = f32::from_bits(bits);
        Ok(OrderedFloat(value))
    }

    fn compare(&self, a: &[u8], b: &[u8]) -> Ordering {
        // OrderedFloat implements Ord, so we can directly compare
        match (self.decode(a), self.decode(b)) {
            (Ok(a_val), Ok(b_val)) => a_val.cmp(&b_val),
            _ => Ordering::Equal, // Default in case of error
        }
    }
}

/// Date encoder for NaiveDate
pub struct NaiveDateKeyEncoder;

impl KeyEncoder<NaiveDate> for NaiveDateKeyEncoder {
    fn encoded_size(&self) -> usize {
        12 // 4 bytes for year, 4 for month, 4 for day
    }

    fn encode(&self, key: &NaiveDate) -> Result<Vec<u8>> {
        let mut result = Vec::with_capacity(12);
        result.extend_from_slice(&key.year().to_le_bytes());
        result.extend_from_slice(&(key.month() as u32).to_le_bytes());
        result.extend_from_slice(&(key.day() as u32).to_le_bytes());
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
pub struct NaiveDateTimeKeyEncoder;

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
pub struct DateTimeKeyEncoder;

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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
    use ordered_float::OrderedFloat;
    use std::cmp::Ordering;

    #[test]
    fn test_i64_encoder() {
        println!("testing i64 encoder...");
        let encoder = IntegerKeyEncoder;
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
        let encoder = OrderedF64KeyEncoder;
        let val = OrderedFloat(std::f64::consts::PI);
        let encoded = encoder.encode(&val).unwrap();
        let decoded = encoder.decode(&encoded).unwrap();
        assert_eq!(val, decoded);
        println!("f64 encoder passed");
    }

    #[test]
    fn test_ordered_float_encoder() {
        println!("testing ordered float encoder...");
        let encoder = OrderedF32KeyEncoder;
        let val = OrderedFloat(std::f32::consts::PI);
        let encoded = encoder.encode(&val).unwrap();
        let decoded = encoder.decode(&encoded).unwrap();
        assert_eq!(val, decoded);
        println!("ordered float encoder passed");
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
}
