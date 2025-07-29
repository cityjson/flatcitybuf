use crate::static_btree::error::{Error, Result};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use chrono::{DateTime, TimeZone, Utc};
use ordered_float::OrderedFloat; // Import OrderedFloat
use std::fmt::Debug;
use std::io::{Read, Write};
use std::mem;

/// Enum to hold different key types supported by the system
#[derive(Debug, Clone)]
pub enum KeyType {
    /// Fixed-size string keys (with different sizes as type parameters)
    StringKey20(FixedStringKey<20>),
    StringKey50(FixedStringKey<50>),
    StringKey100(FixedStringKey<100>),
    /// Integer keys
    Int32(i32),
    Int64(i64),
    UInt32(u32),
    UInt64(u64),
    Int8(i8),
    UInt8(u8),
    Int16(i16),
    UInt16(u16),
    /// Floating point keys (wrapped in OrderedFloat for total ordering)
    Float32(OrderedFloat<f32>),
    Float64(OrderedFloat<f64>),
    /// Boolean keys
    Bool(bool),
    /// DateTime keys
    DateTime(DateTime<Utc>),
}

/// Trait for types that have a maximum representable value.
///
/// This trait allows retrieval of the maximum value for a type,
/// which is useful for B-tree operations like range queries and bounds checking.
pub trait Max {
    /// Returns the maximum representable value for this type.
    fn max_value() -> Self;
}

pub trait Min {
    /// Returns the minimum representable value for this type.
    fn min_value() -> Self;
}

/// Trait defining requirements for keys used in the StaticBTree.
///
/// Keys must support ordering (`Ord`), cloning (`Clone`), debugging (`Debug`),
/// and have a fixed serialized size (`SERIALIZED_SIZE`). Variable-length types
/// like `String` must be adapted (e.g., using fixed-size prefixes) to conform.
pub trait Key: Sized + Ord + Clone + Debug + Default + Max + Min {
    /// The exact size of the key in bytes when serialized.
    /// This is crucial for calculating node sizes and offsets.
    const SERIALIZED_SIZE: usize;

    /// Serializes the key into the provided writer.
    ///
    /// # Arguments
    /// * `writer`: The `Write` target.
    ///
    /// # Returns
    /// Returns the number of bytes written, which is always SERIALIZED_SIZE
    /// `Err(Error)` if writing fails.
    fn write_to<W: Write>(&self, writer: &mut W) -> Result<usize>;

    /// Deserializes a key from the provided reader.
    ///
    /// # Arguments
    /// * `reader`: The `Read` source.
    ///
    /// # Returns
    /// `Ok(Self)` containing the deserialized key on success.
    /// `Err(Error)` if reading fails or the implementation cannot read exactly `SERIALIZED_SIZE` bytes.
    fn read_from<R: Read>(reader: &mut R) -> Result<Self>;

    /// Deserializes a key from the provided bytes.
    ///
    /// # Arguments
    /// * `bytes`: The bytes to deserialize the key from.
    ///
    /// # Returns
    /// `Ok(Self)` containing the deserialized key on success.
    /// `Err(Error)` if the bytes are not a valid key.
    fn from_bytes(bytes: &[u8]) -> Result<Self>;
}

// Implement Max for primitive integer types
impl Max for i8 {
    fn max_value() -> Self {
        i8::MAX
    }
}

impl Max for u8 {
    fn max_value() -> Self {
        u8::MAX
    }
}

impl Max for u16 {
    fn max_value() -> Self {
        u16::MAX
    }
}

impl Max for i16 {
    fn max_value() -> Self {
        i16::MAX
    }
}

impl Max for i32 {
    fn max_value() -> Self {
        i32::MAX
    }
}

impl Max for u32 {
    fn max_value() -> Self {
        u32::MAX
    }
}

impl Max for i64 {
    fn max_value() -> Self {
        i64::MAX
    }
}

impl Max for u64 {
    fn max_value() -> Self {
        u64::MAX
    }
}

// Implement Max for OrderedFloat
impl Max for OrderedFloat<f32> {
    fn max_value() -> Self {
        OrderedFloat(f32::INFINITY)
    }
}

impl Max for OrderedFloat<f64> {
    fn max_value() -> Self {
        OrderedFloat(f64::INFINITY)
    }
}

// Implement Max for bool
impl Max for bool {
    fn max_value() -> Self {
        true
    }
}

// Implement Max for DateTime<Utc>
impl Max for DateTime<Utc> {
    fn max_value() -> Self {
        // A date far in the future (year 9999)
        Utc.timestamp_opt(253402300799, 999_999_999)
            .single()
            .unwrap()
    }
}

// Implement Max for FixedStringKey
impl<const N: usize> Max for FixedStringKey<N> {
    fn max_value() -> Self {
        // For strings, a byte array filled with 0xFF represents the maximum lexicographical value
        Self([0xFF; N])
    }
}

// Implement Min for primitive integer types
impl Min for i8 {
    fn min_value() -> Self {
        i8::MIN
    }
}

impl Min for u8 {
    fn min_value() -> Self {
        u8::MIN
    }
}
impl Min for i16 {
    fn min_value() -> Self {
        i16::MIN
    }
}

impl Min for u16 {
    fn min_value() -> Self {
        u16::MIN
    }
}

impl Min for i32 {
    fn min_value() -> Self {
        i32::MIN
    }
}

impl Min for u32 {
    fn min_value() -> Self {
        u32::MIN
    }
}

impl Min for i64 {
    fn min_value() -> Self {
        i64::MIN
    }
}

impl Min for u64 {
    fn min_value() -> Self {
        u64::MIN
    }
}

impl Min for OrderedFloat<f32> {
    fn min_value() -> Self {
        OrderedFloat(f32::NEG_INFINITY)
    }
}

impl Min for OrderedFloat<f64> {
    fn min_value() -> Self {
        OrderedFloat(f64::NEG_INFINITY)
    }
}

impl Min for bool {
    fn min_value() -> Self {
        false
    }
}

impl Min for DateTime<Utc> {
    fn min_value() -> Self {
        Utc.timestamp_opt(0, 0).single().unwrap()
    }
}

impl<const N: usize> Min for FixedStringKey<N> {
    fn min_value() -> Self {
        FixedStringKey([0u8; N])
    }
}

// Macro to implement Key for primitive integer types easily
macro_rules! impl_key_for_int {
    ($T:ty, $write_method:ident) => {
        impl Key for $T {
            const SERIALIZED_SIZE: usize = mem::size_of::<$T>();

            #[inline]
            fn write_to<W: Write>(&self, writer: &mut W) -> Result<usize> {
                writer.$write_method::<LittleEndian>(*self)?;
                Ok(Self::SERIALIZED_SIZE)
            }

            #[inline]
            fn read_from<R: Read>(reader: &mut R) -> Result<Self> {
                let mut bytes = [0u8; Self::SERIALIZED_SIZE];
                reader.read_exact(&mut bytes)?;
                Ok(<$T>::from_le_bytes(bytes))
            }

            #[inline]
            fn from_bytes(bytes: &[u8]) -> Result<Self> {
                let mut array = [0u8; Self::SERIALIZED_SIZE];
                array.copy_from_slice(&bytes[0..Self::SERIALIZED_SIZE]);
                Ok(<$T>::from_le_bytes(array))
            }
        }
    };
}

// Macro for single-byte types that don't need endianness specifiers
macro_rules! impl_key_for_byte {
    ($T:ty, $write_method:ident) => {
        impl Key for $T {
            const SERIALIZED_SIZE: usize = mem::size_of::<$T>();

            #[inline]
            fn write_to<W: Write>(&self, writer: &mut W) -> Result<usize> {
                writer.$write_method(*self)?;
                Ok(Self::SERIALIZED_SIZE)
            }

            #[inline]
            fn read_from<R: Read>(reader: &mut R) -> Result<Self> {
                let mut bytes = [0u8; Self::SERIALIZED_SIZE];
                reader.read_exact(&mut bytes)?;
                Ok(<$T>::from_le_bytes(bytes))
            }

            #[inline]
            fn from_bytes(bytes: &[u8]) -> Result<Self> {
                let mut array = [0u8; Self::SERIALIZED_SIZE];
                array.copy_from_slice(&bytes[0..Self::SERIALIZED_SIZE]);
                Ok(<$T>::from_le_bytes(array))
            }
        }
    };
}

// Implement Key for standard integer types with the correct write method
impl_key_for_byte!(u8, write_u8);
impl_key_for_byte!(i8, write_i8);
impl_key_for_int!(i16, write_i16);
impl_key_for_int!(u16, write_u16);
impl_key_for_int!(i32, write_i32);
impl_key_for_int!(u32, write_u32);
impl_key_for_int!(i64, write_i64);
impl_key_for_int!(u64, write_u64);

// Implement Key for OrderedFloat<f32>
impl Key for OrderedFloat<f32> {
    const SERIALIZED_SIZE: usize = mem::size_of::<f32>();

    #[inline]
    fn write_to<W: Write>(&self, writer: &mut W) -> Result<usize> {
        writer.write_f32::<LittleEndian>(self.into_inner())?;
        Ok(Self::SERIALIZED_SIZE)
    }

    #[inline]
    fn read_from<R: Read>(reader: &mut R) -> Result<Self> {
        let mut bytes = [0u8; Self::SERIALIZED_SIZE];
        reader.read_exact(&mut bytes)?;
        Ok(OrderedFloat::from(f32::from_le_bytes(bytes)))
    }

    #[inline]
    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut array = [0u8; Self::SERIALIZED_SIZE];
        array.copy_from_slice(&bytes[0..Self::SERIALIZED_SIZE]);
        Ok(OrderedFloat::from(f32::from_le_bytes(array)))
    }
}

// Implement Key for OrderedFloat<f64>
impl Key for OrderedFloat<f64> {
    const SERIALIZED_SIZE: usize = mem::size_of::<f64>();

    #[inline]
    fn write_to<W: Write>(&self, writer: &mut W) -> Result<usize> {
        writer.write_f64::<LittleEndian>(self.into_inner())?;
        Ok(Self::SERIALIZED_SIZE)
    }

    #[inline]
    fn read_from<R: Read>(reader: &mut R) -> Result<Self> {
        let mut bytes = [0u8; Self::SERIALIZED_SIZE];
        reader.read_exact(&mut bytes)?;
        Ok(OrderedFloat::from(f64::from_le_bytes(bytes)))
    }

    #[inline]
    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut array = [0u8; Self::SERIALIZED_SIZE];
        array.copy_from_slice(&bytes[0..Self::SERIALIZED_SIZE]);
        Ok(OrderedFloat::from(f64::from_le_bytes(array)))
    }
}

// Implement Key for bool
impl Key for bool {
    const SERIALIZED_SIZE: usize = 1;

    #[inline]
    fn write_to<W: Write>(&self, writer: &mut W) -> Result<usize> {
        writer.write_all(&[*self as u8]).map_err(Error::from)?;
        Ok(Self::SERIALIZED_SIZE)
    }

    #[inline]
    fn read_from<R: Read>(reader: &mut R) -> Result<Self> {
        let mut byte = [0u8];
        reader.read_exact(&mut byte)?;
        Ok(byte[0] != 0)
    }

    #[inline]
    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Ok(bytes[0] != 0)
    }
}

// Implement Key for DateTime<Utc>
impl Key for DateTime<Utc> {
    const SERIALIZED_SIZE: usize = 12; // 8 bytes for seconds + 4 bytes for nanoseconds

    #[inline]
    fn write_to<W: Write>(&self, writer: &mut W) -> Result<usize> {
        // Write timestamp seconds (i64)
        writer.write_i64::<LittleEndian>(self.timestamp())?;
        // Write nanoseconds (u32)
        writer.write_u32::<LittleEndian>(self.timestamp_subsec_nanos())?;
        Ok(Self::SERIALIZED_SIZE)
    }

    #[inline]
    fn read_from<R: Read>(reader: &mut R) -> Result<Self> {
        let secs = reader.read_i64::<LittleEndian>()?;
        let nanos = reader.read_u32::<LittleEndian>()?;
        let dt = DateTime::<Utc>::from_timestamp(secs, nanos).expect("invalid datetime value");
        Ok(dt)
    }

    #[inline]
    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut array = [0u8; Self::SERIALIZED_SIZE];
        array.copy_from_slice(&bytes[0..Self::SERIALIZED_SIZE]);
        let secs = i64::from_le_bytes(array[0..8].try_into().unwrap());
        let nanos = u32::from_le_bytes(array[8..12].try_into().unwrap());
        let dt = DateTime::<Utc>::from_timestamp(secs, nanos).expect("invalid datetime value");
        Ok(dt)
    }
}

/// A fixed-size key based on a string, suitable for use in the StaticBTree.
///
/// It stores the string's bytes in a fixed-size array `[u8; N]`.
/// If the input string is shorter than `N`, it's padded with null bytes (`\0`).
/// If the input string is longer than `N`, it's truncated.
/// Comparison (`Ord`) is based on the byte array content.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FixedStringKey<const N: usize>([u8; N]);

impl<const N: usize> Default for FixedStringKey<N> {
    fn default() -> Self {
        Self([0u8; N])
    }
}

impl<const N: usize> Key for FixedStringKey<N> {
    const SERIALIZED_SIZE: usize = N;

    #[inline]
    fn write_to<W: Write>(&self, writer: &mut W) -> Result<usize> {
        writer.write_all(&self.0).map_err(Error::from)?;
        Ok(Self::SERIALIZED_SIZE)
    }

    #[inline]
    fn read_from<R: Read>(reader: &mut R) -> Result<Self> {
        let mut bytes = [0u8; N];
        reader.read_exact(&mut bytes)?;
        Ok(FixedStringKey(bytes))
    }

    #[inline]
    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut array = [0u8; N];
        array.copy_from_slice(&bytes[0..N]);
        Ok(FixedStringKey(array))
    }
}

impl<const N: usize> FixedStringKey<N> {
    /// Creates a key from a string slice, padding with 0 bytes
    /// or truncating if necessary to fit exactly N bytes.
    ///
    /// # Examples
    /// ```
    /// # use static_btree::key::FixedStringKey; // Adjust path if needed
    /// let key_short = FixedStringKey::<10>::from_str("hello");
    /// assert_eq!(key_short.to_string_lossy(), "hello");
    ///
    /// let key_long = FixedStringKey::<3>::from_str("world");
    /// assert_eq!(key_long.to_string_lossy(), "wor");
    ///
    /// let key_exact = FixedStringKey::<5>::from_str("exact");
    /// assert_eq!(key_exact.to_string_lossy(), "exact");
    /// ```
    pub fn from_str(s: &str) -> Self {
        let mut bytes = [0u8; N];
        let source_bytes = s.as_bytes();
        let len_to_copy = std::cmp::min(source_bytes.len(), N);
        bytes[..len_to_copy].copy_from_slice(&source_bytes[..len_to_copy]);
        // Remaining bytes are already 0 due to initialization.
        FixedStringKey(bytes)
    }

    /// Attempts to convert back to a String, stopping at the first null byte
    /// or using all N bytes if no null byte is found.
    ///
    /// Note: This conversion is lossy if the original string contained null bytes
    /// before the Nth byte, or if it was truncated.
    ///
    /// # Examples
    /// ```
    /// # use static_btree::key::FixedStringKey; // Adjust path if needed
    /// let key1 = FixedStringKey::<10>::from_str("test");
    /// assert_eq!(key1.to_string_lossy(), "test");
    ///
    /// let key2 = FixedStringKey::<5>::from_str("example"); // truncated to "examp"
    /// assert_eq!(key2.to_string_lossy(), "examp");
    ///
    /// let s_with_null = "null\0xy"; // String containing null byte
    /// let key3 = FixedStringKey::<8>::from_str(s_with_null);
    /// assert_eq!(key3.to_string_lossy(), "null"); // Stops at null byte
    /// ```
    pub fn to_string_lossy(&self) -> String {
        // Find the first null byte, or take the whole array if none exists.
        let first_null = self.0.iter().position(|&b| b == 0).unwrap_or(N);
        // Convert the slice up to the null byte (or end) into a String.
        String::from_utf8_lossy(&self.0[..first_null]).into_owned()
    }
}

#[cfg(test)]
mod tests {
    use chrono::Datelike;

    use super::*;
    use std::cmp::Ordering;
    use std::f32;
    use std::f64;
    use std::io::Cursor;

    fn test_key_impl<T: Key + Eq + Debug>(key_val: T) {
        let mut buffer = Vec::new();
        key_val.write_to(&mut buffer).expect("write should succeed");
        assert_eq!(buffer.len(), T::SERIALIZED_SIZE);

        let mut cursor = Cursor::new(buffer);
        let deserialized_key = T::read_from(&mut cursor).expect("read should succeed");
        assert_eq!(key_val, deserialized_key);

        // Test short read error
        if T::SERIALIZED_SIZE > 0 {
            // Avoid panic for zero-sized types if any
            let short_buffer = vec![0u8; T::SERIALIZED_SIZE - 1];
            let mut short_cursor = Cursor::new(short_buffer);
            let result = T::read_from(&mut short_cursor);
            assert!(result.is_err());
            match result.err().unwrap() {
                Error::IoError(e) => assert_eq!(e.kind(), std::io::ErrorKind::UnexpectedEof),
                _ => panic!("expected io error for short read"),
            }
        }
    }

    #[test]
    fn test_max_values() {
        // Test Max implementation for integers
        assert_eq!(i32::max_value(), i32::MAX);
        assert_eq!(u32::max_value(), u32::MAX);
        assert_eq!(i64::max_value(), i64::MAX);
        assert_eq!(u64::max_value(), u64::MAX);

        // Test Max implementation for floats
        assert_eq!(
            OrderedFloat::<f32>::max_value(),
            OrderedFloat(f32::INFINITY)
        );
        assert_eq!(
            OrderedFloat::<f64>::max_value(),
            OrderedFloat(f64::INFINITY)
        );

        // Test Max implementation for bool
        assert!(bool::max_value());

        // Test Max implementation for DateTime
        let max_date = DateTime::<Utc>::max_value();
        assert!(max_date.year() >= 9999); // Should be far in the future

        // Test Max implementation for FixedStringKey
        let max_str_key = FixedStringKey::<5>::max_value();
        assert_eq!(max_str_key.0, [0xFF; 5]);

        // Verify max values are actually maximum
        assert!(5_i32 < i32::max_value());
        assert!(OrderedFloat(1000.0f64) < OrderedFloat::<f64>::max_value());
        assert!(bool::max_value());
        assert!(Utc::now() < DateTime::<Utc>::max_value());
        assert!(FixedStringKey::<5>::from_str("zzzzz") < FixedStringKey::<5>::max_value());
    }

    #[test]
    fn test_int_keys() {
        test_key_impl(12345i32);
        test_key_impl(-54321i32);
        test_key_impl(0i32);
        test_key_impl(i32::MAX);
        test_key_impl(i32::MIN);

        test_key_impl(12345u32);
        test_key_impl(0u32);
        test_key_impl(u32::MAX);

        test_key_impl(123456789012345i64);
        test_key_impl(-98765432109876i64);
        test_key_impl(0i64);
        test_key_impl(i64::MAX);
        test_key_impl(i64::MIN);

        test_key_impl(123456789012345u64);
        test_key_impl(0u64);
        test_key_impl(u64::MAX);
    }

    #[test]
    fn test_float_keys() {
        test_key_impl(OrderedFloat(123.45f32));
        test_key_impl(OrderedFloat(-987.65f32));
        test_key_impl(OrderedFloat(0.0f32));
        test_key_impl(OrderedFloat(f32::MAX));
        test_key_impl(OrderedFloat(f32::MIN));
        test_key_impl(OrderedFloat(f32::INFINITY));
        test_key_impl(OrderedFloat(f32::NEG_INFINITY));
        test_key_impl(OrderedFloat(f32::NAN)); // Test NaN serialization/deserialization

        test_key_impl(OrderedFloat(123456.789012f64));
        test_key_impl(OrderedFloat(-987654.321098f64));
        test_key_impl(OrderedFloat(0.0f64));
        test_key_impl(OrderedFloat(f64::MAX));
        test_key_impl(OrderedFloat(f64::MIN));
        test_key_impl(OrderedFloat(f64::INFINITY));
        test_key_impl(OrderedFloat(f64::NEG_INFINITY));
        test_key_impl(OrderedFloat(f64::NAN)); // Test NaN serialization/deserialization
    }

    #[test]
    fn test_float_ordering() {
        // Test normal ordering
        assert!(OrderedFloat(1.0f32) < OrderedFloat(2.0f32));
        assert!(OrderedFloat(-1.0f64) < OrderedFloat(1.0f64));

        // Test infinity ordering
        assert!(OrderedFloat(f32::MAX) < OrderedFloat(f32::INFINITY));
        assert!(OrderedFloat(f64::NEG_INFINITY) < OrderedFloat(f64::MIN));

        // Test NaN ordering (ordered-float puts NaN greater than all other numbers)
        assert!(OrderedFloat(f32::INFINITY) < OrderedFloat(f32::NAN));
        assert!(OrderedFloat(f64::MAX) < OrderedFloat(f64::NAN));
        assert!(OrderedFloat(f32::NAN).cmp(&OrderedFloat(f32::NAN)) == Ordering::Equal);
    }

    #[test]
    fn test_fixed_string_key_from_str() {
        // Test shorter string (padding)
        let key_short = FixedStringKey::<10>::from_str("hello");
        assert_eq!(key_short.0[0..5], *b"hello");
        assert_eq!(key_short.0[5..], [0u8; 5]);
        assert_eq!(key_short.to_string_lossy(), "hello");

        // Test longer string (truncation)
        let key_long = FixedStringKey::<3>::from_str("world");
        assert_eq!(key_long.0, *b"wor");
        assert_eq!(key_long.to_string_lossy(), "wor");

        // Test exact length string
        let key_exact = FixedStringKey::<5>::from_str("exact");
        assert_eq!(key_exact.0, *b"exact");
        assert_eq!(key_exact.to_string_lossy(), "exact");

        // Test empty string
        let key_empty = FixedStringKey::<4>::from_str("");
        assert_eq!(key_empty.0, [0u8; 4]);
        assert_eq!(key_empty.to_string_lossy(), "");
    }

    #[test]
    fn test_fixed_string_key_to_string_lossy() {
        let key1 = FixedStringKey::<10>::from_str("test\0ing"); // Contains null byte
        assert_eq!(key1.to_string_lossy(), "test"); // Stops at null

        let key2 = FixedStringKey::<5>::from_str("abcde");
        assert_eq!(key2.to_string_lossy(), "abcde"); // No null byte

        let key3 = FixedStringKey::<3>::from_str("xyz123"); // Truncated
        assert_eq!(key3.to_string_lossy(), "xyz");
    }

    #[test]
    fn test_fixed_string_key_serialization() {
        test_key_impl(FixedStringKey::<8>::from_str("testkey"));
        test_key_impl(FixedStringKey::<4>::from_str("longkey")); // truncated
        test_key_impl(FixedStringKey::<12>::from_str("short")); // padded
        test_key_impl(FixedStringKey::<5>::from_str("")); // empty
    }

    #[test]
    fn test_fixed_string_key_ordering() {
        let key1 = FixedStringKey::<10>::from_str("apple");
        let key2 = FixedStringKey::<10>::from_str("apply");
        let key3 = FixedStringKey::<10>::from_str("banana");
        let key4 = FixedStringKey::<10>::from_str("apple"); // Equal to key1
        let key5 = FixedStringKey::<10>::from_str("app"); // Shorter, padded

        assert!(key1 < key2);
        assert!(key2 < key3);
        assert!(key1 < key3);
        assert_eq!(key1.cmp(&key4), Ordering::Equal);
        assert!(key5 < key1); // "app\0..." < "apple..."
    }

    #[test]
    fn test_bool_keys() {
        test_key_impl(true);
        test_key_impl(false);
    }

    #[test]
    fn test_datetime_keys() {
        // Test current time
        // test_key_impl(Utc::now());

        // // Test epoch
        // test_key_impl(Utc.timestamp_opt(0, 0).single().unwrap());

        // // Test future date
        // test_key_impl(Utc.timestamp_opt(32503680000, 999999999).single().unwrap()); // Year 3000

        // // Test past date
        // test_key_impl(Utc.timestamp_opt(-62135596800, 0).single().unwrap()); // Year 0

        // // Test ordering
        // let dt1 = Utc.timestamp_opt(1000, 0).single().unwrap();
        // let dt2 = Utc.timestamp_opt(2000, 0).single().unwrap();
        // assert!(dt1 < dt2);

        // // Test subsecond precision
        // let dt3 = Utc.timestamp_opt(1000, 500).single().unwrap();
        // let dt4 = Utc.timestamp_opt(1000, 1000).single().unwrap();
        // assert!(dt3 < dt4);

        // Test actual datetime 2010-10-13T12:43:04Z

        let dt = chrono::DateTime::parse_from_rfc3339("2010-10-13T12:43:04Z")
            .unwrap()
            .to_utc();
        test_key_impl(dt);
    }
}
