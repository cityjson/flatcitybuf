use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, Utc};

pub use ordered_float::OrderedFloat;
/// A trait for converting types to and from bytes.
pub trait ByteSerializable {
    /// Convert self into a vector of bytes.
    fn to_bytes(&self) -> Vec<u8>;

    /// Construct an instance from the given bytes.
    fn from_bytes(bytes: &[u8]) -> Self;
}

#[derive(Debug, Clone)]
pub enum ByteSerializableValue {
    I64(i64),
    I32(i32),
    I16(i16),
    I8(i8),
    U64(u64),
    U32(u32),
    U16(u16),
    U8(u8),
    F64(OrderedFloat<f64>),
    F32(OrderedFloat<f32>),
    Bool(bool),
    String(String),
    NaiveDateTime(NaiveDateTime),
    NaiveDate(NaiveDate),
    DateTime(DateTime<Utc>),
}

impl ByteSerializable for i64 {
    fn to_bytes(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }
    fn from_bytes(bytes: &[u8]) -> Self {
        let mut array = [0u8; 8];
        array.copy_from_slice(&bytes[0..8]);
        i64::from_le_bytes(array)
    }
}

impl ByteSerializable for i32 {
    fn to_bytes(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }
    fn from_bytes(bytes: &[u8]) -> Self {
        let mut array = [0u8; 4];
        array.copy_from_slice(&bytes[0..4]);
        i32::from_le_bytes(array)
    }
}

impl ByteSerializable for i16 {
    fn to_bytes(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }
    fn from_bytes(bytes: &[u8]) -> Self {
        let mut array = [0u8; 2];
        array.copy_from_slice(&bytes[0..2]);
        i16::from_le_bytes(array)
    }
}

impl ByteSerializable for i8 {
    fn to_bytes(&self) -> Vec<u8> {
        vec![*self as u8]
    }
    fn from_bytes(bytes: &[u8]) -> Self {
        bytes[0] as i8
    }
}
impl ByteSerializable for u64 {
    fn to_bytes(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }
    fn from_bytes(bytes: &[u8]) -> Self {
        let mut array = [0u8; 8];
        array.copy_from_slice(&bytes[0..8]);
        u64::from_le_bytes(array)
    }
}
impl ByteSerializable for u32 {
    fn to_bytes(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }
    fn from_bytes(bytes: &[u8]) -> Self {
        let mut array = [0u8; 4];
        array.copy_from_slice(&bytes[0..4]);
        u32::from_le_bytes(array)
    }
}

impl ByteSerializable for u16 {
    fn to_bytes(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }
    fn from_bytes(bytes: &[u8]) -> Self {
        let mut array = [0u8; 2];
        array.copy_from_slice(&bytes[0..2]);
        u16::from_le_bytes(array)
    }
}

impl ByteSerializable for u8 {
    fn to_bytes(&self) -> Vec<u8> {
        vec![*self]
    }
    fn from_bytes(bytes: &[u8]) -> Self {
        bytes[0]
    }
}

impl ByteSerializable for String {
    fn to_bytes(&self) -> Vec<u8> {
        self.as_bytes().to_vec()
    }
    fn from_bytes(bytes: &[u8]) -> Self {
        String::from_utf8(bytes.to_vec()).unwrap()
    }
}

impl ByteSerializable for f64 {
    fn to_bytes(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }
    fn from_bytes(bytes: &[u8]) -> Self {
        let mut array = [0u8; 8];
        array.copy_from_slice(&bytes[0..8]);
        f64::from_le_bytes(array)
    }
}

impl ByteSerializable for f32 {
    fn to_bytes(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }
    fn from_bytes(bytes: &[u8]) -> Self {
        let mut array = [0u8; 4];
        array.copy_from_slice(&bytes[0..4]);
        f32::from_le_bytes(array)
    }
}

// Implement ByteSerializable for OrderedFloat<f64> because f64 doesn't implement Ord trait because of NaN values.
impl ByteSerializable for OrderedFloat<f64> {
    fn to_bytes(&self) -> Vec<u8> {
        self.0.to_le_bytes().to_vec()
    }

    fn from_bytes(bytes: &[u8]) -> Self {
        let mut array = [0u8; 8];
        array.copy_from_slice(&bytes[0..8]);
        OrderedFloat(f64::from_le_bytes(array))
    }
}

// Implement ByteSerializable for OrderedFloat<f64> because f64 doesn't implement Ord trait because of NaN values.
impl ByteSerializable for OrderedFloat<f32> {
    fn to_bytes(&self) -> Vec<u8> {
        self.0.to_le_bytes().to_vec()
    }
    fn from_bytes(bytes: &[u8]) -> Self {
        let mut array = [0u8; 4];
        array.copy_from_slice(&bytes[0..4]);
        OrderedFloat(f32::from_le_bytes(array))
    }
}

impl ByteSerializable for bool {
    fn to_bytes(&self) -> Vec<u8> {
        // Represent true as 1 and false as 0.
        vec![if *self { 1u8 } else { 0u8 }]
    }
    fn from_bytes(bytes: &[u8]) -> Self {
        bytes.first().is_some_and(|&b| b != 0)
    }
}

/// We serialize a NaiveDateTime as 12 bytes:
/// - 8 bytes for the timestamp (seconds since epoch, as i64, little endian)
/// - 4 bytes for the nanosecond part (u32, little endian)
impl ByteSerializable for NaiveDateTime {
    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = self.and_utc().timestamp().to_le_bytes().to_vec();
        bytes.extend(&self.and_utc().timestamp_subsec_nanos().to_le_bytes());
        bytes
    }

    fn from_bytes(bytes: &[u8]) -> Self {
        // Ensure there are at least 12 bytes.
        assert!(bytes.len() >= 12, "Not enough bytes for NaiveDateTime");
        let mut ts_bytes = [0u8; 8];
        ts_bytes.copy_from_slice(&bytes[0..8]);
        let timestamp = i64::from_le_bytes(ts_bytes);

        let mut nano_bytes = [0u8; 4];
        nano_bytes.copy_from_slice(&bytes[8..12]);
        let nanosecond = u32::from_le_bytes(nano_bytes);

        NaiveDateTime::from_timestamp(timestamp, nanosecond)
    }
}

/// We serialize a NaiveDate as 4 bytes:
/// - 4 bytes for the year (u32, little endian)
/// - 2 bytes for the month (u16, little endian)
/// - 2 bytes for the day (u16, little endian)
impl ByteSerializable for NaiveDate {
    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = self.year().to_le_bytes().to_vec();
        bytes.extend(&self.month().to_le_bytes());
        bytes.extend(&self.day().to_le_bytes());
        bytes
    }

    fn from_bytes(bytes: &[u8]) -> Self {
        let mut array = [0u8; 12];
        array.copy_from_slice(&bytes[0..12]);
        let mut y = [0u8; 4];
        let mut m = [0u8; 4];
        let mut d = [0u8; 4];
        y.copy_from_slice(&array[0..4]);
        m.copy_from_slice(&array[4..8]);
        d.copy_from_slice(&array[8..12]);

        NaiveDate::from_ymd_opt(
            i32::from_le_bytes(y),
            u32::from_le_bytes(m),
            u32::from_le_bytes(d),
        )
        .unwrap()
    }
}

/// Since DateTime<Utc> is essentially a NaiveDateTime with an offset,
/// we delegate the conversion to the NaiveDateTime implementation.
impl ByteSerializable for DateTime<Utc> {
    fn to_bytes(&self) -> Vec<u8> {
        self.naive_utc().to_bytes()
    }

    fn from_bytes(bytes: &[u8]) -> Self {
        let naive = <NaiveDateTime as ByteSerializable>::from_bytes(bytes);
        DateTime::<Utc>::from_utc(naive, Utc)
    }
}
