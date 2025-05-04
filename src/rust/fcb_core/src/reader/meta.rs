use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    pub columns: Vec<Column>,
    #[serde(rename = "featureCount")]
    pub feature_count: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Column {
    pub index: u16,
    pub name: String,
    #[serde(rename = "type")]
    pub _type: ColumnType,
    pub title: Option<String>,
    pub description: Option<String>,
    pub precision: Option<i32>,
    pub scale: Option<i32>,
    pub nullable: Option<bool>,
    pub unique: Option<bool>,
    pub primary_key: Option<bool>,
    pub metadata: Option<String>,
    #[serde(rename = "attrIndex")]
    pub attr_index: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ColumnType {
    Byte,     // Signed 8-bit integer
    UByte,    // Unsigned 8-bit integer
    Bool,     // Boolean
    Short,    // Signed 16-bit integer
    UShort,   // Unsigned 16-bit integer
    Int,      // Signed 32-bit integer
    UInt,     // Unsigned 32-bit integer
    Long,     // Signed 64-bit integer
    ULong,    // Unsigned 64-bit integer
    Float,    // Single precision floating point number
    Double,   // Double precision floating point number
    String,   // UTF8 string
    Json,     // General JSON type intended to be application specific
    DateTime, // ISO 8601 date time
    Binary,   // General binary type intended to be application specific
}
