use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, Utc};
pub use ordered_float::OrderedFloat;

use crate::error;

pub type Float<T> = OrderedFloat<T>;

/// A trait for converting types to and from bytes.
pub trait ByteSerializable: Send + Sync {
    /// Convert self into a vector of bytes.
    fn to_bytes(&self) -> Vec<u8>;

    /// Construct an instance from the given bytes.
    fn from_bytes(bytes: &[u8]) -> Self;

    /// Return the type of the value.
    fn value_type(&self) -> ByteSerializableType;
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
    F64(Float<f64>),
    F32(Float<f32>),
    Bool(bool),
    String(String),
    NaiveDateTime(NaiveDateTime),
    NaiveDate(NaiveDate),
    DateTime(DateTime<Utc>),
}

impl ByteSerializableValue {
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            ByteSerializableValue::I64(i) => i.to_bytes(),
            ByteSerializableValue::I32(i) => i.to_bytes(),
            ByteSerializableValue::I16(i) => i.to_bytes(),
            ByteSerializableValue::I8(i) => i.to_bytes(),
            ByteSerializableValue::U64(i) => i.to_bytes(),
            ByteSerializableValue::U32(i) => i.to_bytes(),
            ByteSerializableValue::U16(i) => i.to_bytes(),
            ByteSerializableValue::U8(i) => i.to_bytes(),
            ByteSerializableValue::F64(i) => i.to_bytes(),
            ByteSerializableValue::F32(i) => i.to_bytes(),
            ByteSerializableValue::Bool(i) => i.to_bytes(),
            ByteSerializableValue::String(s) => s.to_bytes(),
            ByteSerializableValue::NaiveDateTime(dt) => dt.to_bytes(),
            ByteSerializableValue::NaiveDate(d) => d.to_bytes(),
            ByteSerializableValue::DateTime(dt) => dt.to_bytes(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ByteSerializableType {
    I64,
    I32,
    I16,
    I8,
    U64,
    U32,
    U16,
    U8,
    F64,
    F32,
    Bool,
    String,
    NaiveDateTime,
    NaiveDate,
    DateTime,
}
impl ByteSerializableType {
    pub fn to_bytes(&self) -> Vec<u8> {
        // Use u32 to represent the type and serialize in little endian
        let type_id: u32 = match self {
            ByteSerializableType::I64 => 0,
            ByteSerializableType::I32 => 1,
            ByteSerializableType::I16 => 2,
            ByteSerializableType::I8 => 3,
            ByteSerializableType::U64 => 4,
            ByteSerializableType::U32 => 5,
            ByteSerializableType::U16 => 6,
            ByteSerializableType::U8 => 7,
            ByteSerializableType::F64 => 8,
            ByteSerializableType::F32 => 9,
            ByteSerializableType::Bool => 10,
            ByteSerializableType::String => 11,
            ByteSerializableType::NaiveDateTime => 12,
            ByteSerializableType::NaiveDate => 13,
            ByteSerializableType::DateTime => 14,
        };
        type_id.to_le_bytes().to_vec()
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, error::Error> {
        if bytes.len() < 4 {
            return Err(error::Error::InvalidType(
                "not enough bytes to deserialize type".to_string(),
            ));
        }

        // Read u32 in little endian format
        let mut type_id_bytes = [0u8; 4];
        type_id_bytes.copy_from_slice(&bytes[0..4]);
        let type_id = u32::from_le_bytes(type_id_bytes);

        match type_id {
            0 => Ok(ByteSerializableType::I64),
            1 => Ok(ByteSerializableType::I32),
            2 => Ok(ByteSerializableType::I16),
            3 => Ok(ByteSerializableType::I8),
            4 => Ok(ByteSerializableType::U64),
            5 => Ok(ByteSerializableType::U32),
            6 => Ok(ByteSerializableType::U16),
            7 => Ok(ByteSerializableType::U8),
            8 => Ok(ByteSerializableType::F64),
            9 => Ok(ByteSerializableType::F32),
            10 => Ok(ByteSerializableType::Bool),
            11 => Ok(ByteSerializableType::String),
            12 => Ok(ByteSerializableType::NaiveDateTime),
            13 => Ok(ByteSerializableType::NaiveDate),
            14 => Ok(ByteSerializableType::DateTime),
            _ => Err(error::Error::InvalidType(format!(
                "invalid type id: {}",
                type_id
            ))),
        }
    }

    /// Convert a type ID to the corresponding ByteSerializableType
    pub fn from_type_id(type_id: u32) -> Result<Self, error::Error> {
        match type_id {
            0 => Ok(ByteSerializableType::I64),
            1 => Ok(ByteSerializableType::I32),
            2 => Ok(ByteSerializableType::I16),
            3 => Ok(ByteSerializableType::I8),
            4 => Ok(ByteSerializableType::U64),
            5 => Ok(ByteSerializableType::U32),
            6 => Ok(ByteSerializableType::U16),
            7 => Ok(ByteSerializableType::U8),
            8 => Ok(ByteSerializableType::F64),
            9 => Ok(ByteSerializableType::F32),
            10 => Ok(ByteSerializableType::Bool),
            11 => Ok(ByteSerializableType::String),
            12 => Ok(ByteSerializableType::NaiveDateTime),
            13 => Ok(ByteSerializableType::NaiveDate),
            14 => Ok(ByteSerializableType::DateTime),
            _ => Err(error::Error::InvalidType(format!(
                "invalid type id: {}",
                type_id
            ))),
        }
    }
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
    fn value_type(&self) -> ByteSerializableType {
        ByteSerializableType::I64
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
    fn value_type(&self) -> ByteSerializableType {
        ByteSerializableType::I32
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
    fn value_type(&self) -> ByteSerializableType {
        ByteSerializableType::I16
    }
}

impl ByteSerializable for i8 {
    fn to_bytes(&self) -> Vec<u8> {
        vec![*self as u8]
    }
    fn from_bytes(bytes: &[u8]) -> Self {
        bytes[0] as i8
    }
    fn value_type(&self) -> ByteSerializableType {
        ByteSerializableType::I8
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
    fn value_type(&self) -> ByteSerializableType {
        ByteSerializableType::U64
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
    fn value_type(&self) -> ByteSerializableType {
        ByteSerializableType::U32
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
    fn value_type(&self) -> ByteSerializableType {
        ByteSerializableType::U16
    }
}

impl ByteSerializable for u8 {
    fn to_bytes(&self) -> Vec<u8> {
        vec![*self]
    }
    fn from_bytes(bytes: &[u8]) -> Self {
        bytes[0]
    }
    fn value_type(&self) -> ByteSerializableType {
        ByteSerializableType::U8
    }
}

impl ByteSerializable for String {
    fn to_bytes(&self) -> Vec<u8> {
        self.as_bytes().to_vec()
    }
    fn from_bytes(bytes: &[u8]) -> Self {
        String::from_utf8(bytes.to_vec()).unwrap()
    }
    fn value_type(&self) -> ByteSerializableType {
        ByteSerializableType::String
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
    fn value_type(&self) -> ByteSerializableType {
        ByteSerializableType::F64
    }
}

impl ByteSerializable for f32 {
    fn to_bytes(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }

    fn from_bytes(bytes: &[u8]) -> Self {
        // If the byte slice is empty, return a default value
        if bytes.is_empty() {
            return 0.0;
        }

        // Otherwise, convert the bytes to an f32
        let mut array = [0u8; 4];
        array.copy_from_slice(&bytes[0..4]);
        f32::from_le_bytes(array)
    }

    fn value_type(&self) -> ByteSerializableType {
        ByteSerializableType::F32
    }
}

// Implement ByteSerializable for Float<f64> because f64 doesn't implement Ord trait because of NaN values.
impl ByteSerializable for Float<f64> {
    fn to_bytes(&self) -> Vec<u8> {
        self.0.to_le_bytes().to_vec()
    }

    fn from_bytes(bytes: &[u8]) -> Self {
        let mut array = [0u8; 8];
        array.copy_from_slice(&bytes[0..8]);
        OrderedFloat(f64::from_le_bytes(array))
    }

    fn value_type(&self) -> ByteSerializableType {
        ByteSerializableType::F64
    }
}

// Implement ByteSerializable for Float<f32> because f32 doesn't implement Ord trait because of NaN values.
impl ByteSerializable for Float<f32> {
    fn to_bytes(&self) -> Vec<u8> {
        self.0.to_le_bytes().to_vec()
    }

    fn from_bytes(bytes: &[u8]) -> Self {
        // If the byte slice is empty, return a default value
        if bytes.is_empty() {
            return OrderedFloat(0.0);
        }

        // Otherwise, convert the bytes to an f32
        let mut array = [0u8; 4];
        array.copy_from_slice(&bytes[0..4]);
        OrderedFloat(f32::from_le_bytes(array))
    }

    fn value_type(&self) -> ByteSerializableType {
        ByteSerializableType::F32
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

    fn value_type(&self) -> ByteSerializableType {
        ByteSerializableType::Bool
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
        let mut ts_bytes = [0u8; 8];
        ts_bytes.copy_from_slice(&bytes[0..8]);
        let timestamp = i64::from_le_bytes(ts_bytes);

        let mut nano_bytes = [0u8; 4];
        nano_bytes.copy_from_slice(&bytes[8..12]);
        let nanosecond = u32::from_le_bytes(nano_bytes);

        NaiveDateTime::from_timestamp(timestamp, nanosecond)
    }

    fn value_type(&self) -> ByteSerializableType {
        ByteSerializableType::NaiveDateTime
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

    fn value_type(&self) -> ByteSerializableType {
        ByteSerializableType::NaiveDate
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

    fn value_type(&self) -> ByteSerializableType {
        ByteSerializableType::DateTime
    }
}
