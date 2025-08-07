use crate::fb::ColumnType;
use byteorder::{ByteOrder, LittleEndian};
use chrono::{DateTime, Utc};
use cjseq::CityJSONFeature;
use serde_json::Value;
use std::collections::HashMap;

// Schema for attributes. The key is the attribute name, the value is a tuple of the column index and the column type.
pub type AttributeSchema = HashMap<String, (u16, ColumnType)>;

pub trait AttributeSchemaMethods {
    fn add_attributes(&mut self, attrs: &Value);
}

impl AttributeSchemaMethods for AttributeSchema {
    fn add_attributes(&mut self, attrs: &Value) {
        if !attrs.is_object() {
            self.insert("json".to_string(), (self.len() as u16, ColumnType::Json));
            return;
        }

        let map = attrs.as_object().unwrap();
        for (key, val) in map.iter() {
            if !self.contains_key(key) && !val.is_null() {
                if let Some(coltype) = guess_type(val) {
                    self.insert(key.clone(), (self.len() as u16, coltype));
                }
            }
        }
    }
}

/// Naive type-guessing. You could use your schema or logic as in your Python code.
fn guess_type(value: &Value) -> Option<ColumnType> {
    match value {
        Value::Bool(_) => Some(ColumnType::Bool),
        Value::Number(n) => {
            if n.is_f64() {
                Some(ColumnType::Double)
            } else if n.is_u64() {
                Some(ColumnType::ULong)
            } else if n.is_i64() {
                Some(ColumnType::Long)
            } else {
                Some(ColumnType::ULong) // Fallback for unknown number type.
            }
        }
        Value::String(s) => {
            // Attempt to parse the string as an RFC3339 date.
            if chrono::DateTime::parse_from_rfc3339(s).is_ok() {
                Some(ColumnType::DateTime)
            } else {
                Some(ColumnType::String)
            }
        }
        Value::Array(_) => Some(ColumnType::Json),
        Value::Object(_) => Some(ColumnType::Json),
        _ => None,
    }
}

pub(crate) fn attr_size(coltype: &ColumnType, colval: &Value) -> usize {
    match *coltype {
        ColumnType::Byte => size_of::<i8>(),
        ColumnType::UByte => size_of::<u8>(),
        ColumnType::Bool => size_of::<u8>(),
        ColumnType::Short => size_of::<i16>(),
        ColumnType::UShort => size_of::<u16>(),
        ColumnType::Int => size_of::<i32>(),
        ColumnType::UInt => size_of::<u32>(),
        ColumnType::Long => size_of::<i64>(),
        ColumnType::ULong => size_of::<u64>(),
        ColumnType::Float => size_of::<f32>(),
        ColumnType::Double => size_of::<f64>(),
        ColumnType::String | ColumnType::DateTime => {
            size_of::<u32>() + colval.as_str().unwrap().len()
        }
        ColumnType::Json => {
            let json = serde_json::to_string(colval).unwrap_or_default();
            size_of::<u32>() + json.len()
        }
        ColumnType::Binary => size_of::<u32>() + colval.as_str().unwrap().len(), //TODO: check if this is correct
        _ => unreachable!(),
    }
}

pub(crate) fn encode_attributes_with_schema(attr: &Value, schema: &AttributeSchema) -> Vec<u8> {
    if !attr.is_object() || attr.as_object().unwrap().is_empty() || attr.is_null() {
        return Vec::new();
    }

    let mut out = Vec::new();
    let mut sorted_schema: Vec<_> = schema.iter().collect();
    sorted_schema.sort_by_key(|(_, (index, _))| *index);

    for (name, (index, coltype)) in sorted_schema {
        let (_, val) = {
            let attr_obj = attr.as_object();
            if let Some(attr_obj) = attr_obj {
                let value = attr_obj.iter().find(|(k, _)| *k == name);
                if let Some(value) = value {
                    (value.0, value.1)
                } else {
                    continue;
                }
            } else {
                return Vec::new();
            }
        };

        if val.is_null() {
            continue;
        }

        let mut offset = out.len();
        let attr_size = attr_size(coltype, val);

        // Reserve space for index and value
        out.resize(offset + size_of::<u16>() + attr_size, 0);

        // Write index
        LittleEndian::write_u16(&mut out[offset..], *index);
        offset += size_of::<u16>();

        match *coltype {
            ColumnType::Bool => {
                let b = val.as_bool().unwrap_or(false);
                out[offset] = b as u8;
            }
            ColumnType::Int => {
                let i = val.as_i64().unwrap_or(0);
                LittleEndian::write_i32(&mut out[offset..], i as i32);
            }
            ColumnType::UInt => {
                let i = val.as_u64().unwrap_or(0);
                LittleEndian::write_u32(&mut out[offset..], i as u32);
            }
            ColumnType::Byte => {
                let b = val.as_i64().unwrap_or(0);
                out[offset] = b as u8;
            }
            ColumnType::UByte => {
                let b = val.as_u64().unwrap_or(0);
                out[offset] = b as u8;
            }

            ColumnType::Short => {
                let i = val.as_i64().unwrap_or(0);
                LittleEndian::write_i16(&mut out[offset..], i as i16);
            }
            ColumnType::UShort => {
                let i = val.as_u64().unwrap_or(0);
                LittleEndian::write_u16(&mut out[offset..], i as u16);
            }

            ColumnType::Long => {
                let i = val.as_i64().unwrap_or(0);
                LittleEndian::write_i64(&mut out[offset..], i);
            }
            ColumnType::ULong => {
                let i = val.as_u64().unwrap_or(0);
                LittleEndian::write_u64(&mut out[offset..], i);
            }
            ColumnType::Float => {
                let f = val.as_f64().unwrap_or(0.0);
                LittleEndian::write_f32(&mut out[offset..], f as f32);
            }
            ColumnType::Double => {
                let f = val.as_f64().unwrap_or(0.0);
                LittleEndian::write_f64(&mut out[offset..], f);
            }
            ColumnType::String | ColumnType::DateTime => {
                let s = val.as_str().unwrap_or("");
                LittleEndian::write_u32(&mut out[offset..], s.len() as u32);
                out[offset + size_of::<u32>()..offset + size_of::<u32>() + s.len()]
                    .copy_from_slice(s.as_bytes());
            }
            ColumnType::Json => {
                let json = serde_json::to_string(val).unwrap_or_default();
                LittleEndian::write_u32(&mut out[offset..], json.len() as u32);
                out[offset + size_of::<u32>()..offset + size_of::<u32>() + json.len()]
                    .copy_from_slice(json.as_bytes());
            }
            ColumnType::Binary => {
                let s = val.as_str().unwrap_or("");
                LittleEndian::write_u32(&mut out[offset..], s.len() as u32);
                out[offset + size_of::<u32>()..offset + size_of::<u32>() + s.len()]
                    .copy_from_slice(s.as_bytes());
            }
            _ => unreachable!(),
        }
    }
    out
}

#[derive(Clone, PartialEq, Debug)]
pub enum AttributeIndexEntry {
    Bool { index: u16, val: bool },
    Int { index: u16, val: i32 },
    UInt { index: u16, val: u32 },
    Long { index: u16, val: i64 },
    ULong { index: u16, val: u64 },
    Float { index: u16, val: f32 },
    Double { index: u16, val: f64 },
    String { index: u16, val: String },
    DateTime { index: u16, val: DateTime<Utc> },
    Short { index: u16, val: i16 },
    UShort { index: u16, val: u16 },
    Byte { index: u16, val: u8 },
    UByte { index: u16, val: u8 },
    Json { index: u16, val: String },
    Binary { index: u16, val: String },
}

pub fn cityfeature_to_index_entries(
    cityfeature: &CityJSONFeature,
    schema: &AttributeSchema,
    indexing_attr: &[String],
) -> Vec<AttributeIndexEntry> {
    let mut index_entries = Vec::new();
    for object in cityfeature.city_objects.values() {
        if let Some(attr) = &object.attributes {
            let attr_index_entries = attribute_to_index_entries(attr, schema, indexing_attr);
            index_entries.extend(attr_index_entries);
        }
    }

    index_entries
}

// this attr should be a json object with attribute name as key and attribute value as value
pub fn attribute_to_index_entries(
    attr: &Value,
    schema: &AttributeSchema,
    indexing_attr: &[String],
) -> Vec<AttributeIndexEntry> {
    if !attr.is_object() || attr.is_null() || attr.as_object().unwrap().is_empty() {
        return Vec::new();
    }

    let mut index_entries = Vec::new();

    let map = attr.as_object().unwrap();
    for attr in indexing_attr {
        let val: &Value = match map.get(attr) {
            Some(val) => val,
            None => {
                println!("Attribute {attr} not found in schema");
                continue;
            }
        };

        let index_coltype = schema.get(attr);
        if let Some((index, coltype)) = index_coltype {
            match *coltype {
                ColumnType::Bool => {
                    let b = val.as_bool().unwrap_or(false);
                    index_entries.push(AttributeIndexEntry::Bool {
                        index: *index,
                        val: b,
                    });
                }
                ColumnType::Int => {
                    let i = val.as_i64().unwrap_or(0);
                    index_entries.push(AttributeIndexEntry::Int {
                        index: *index,
                        val: i as i32,
                    });
                }
                ColumnType::UInt => {
                    let i = val.as_u64().unwrap_or(0);
                    index_entries.push(AttributeIndexEntry::UInt {
                        index: *index,
                        val: i as u32,
                    });
                }
                ColumnType::Long => {
                    let i = val.as_i64().unwrap_or(0);
                    index_entries.push(AttributeIndexEntry::Long {
                        index: *index,
                        val: i as i64,
                    });
                }
                ColumnType::ULong => {
                    let i = val.as_u64().unwrap_or(0);
                    index_entries.push(AttributeIndexEntry::ULong {
                        index: *index,
                        val: i as u64,
                    });
                }
                ColumnType::Float => {
                    let f = val.as_f64().unwrap_or(0.0);
                    index_entries.push(AttributeIndexEntry::Float {
                        index: *index,
                        val: f as f32,
                    });
                }
                ColumnType::Double => {
                    let f = val.as_f64().unwrap_or(0.0);
                    index_entries.push(AttributeIndexEntry::Double {
                        index: *index,
                        val: f,
                    });
                }
                ColumnType::String => {
                    index_entries.push(AttributeIndexEntry::String {
                        index: *index,
                        val: val.as_str().unwrap_or("").to_string(),
                    });
                }
                ColumnType::DateTime => {
                    let dt = match chrono::DateTime::parse_from_rfc3339(val.as_str().unwrap_or(""))
                    {
                        Ok(dt) => dt.to_utc(),
                        Err(e) => {
                            eprintln!("Failed to parse DateTime: {e}");
                            // Choose whether to skip, default, or handle differently
                            // For example, default to 1970-01-01:
                            DateTime::<Utc>::from_timestamp(0, 0).unwrap()
                        }
                    };
                    index_entries.push(AttributeIndexEntry::DateTime {
                        index: *index,
                        val: dt,
                    });
                }
                _ => {
                    //Byte, Ubyte,
                    println!("Attribute {attr} is not supported for indexing");
                }
            }
        }
    }

    index_entries
}

#[cfg(test)]
mod tests {
    use crate::{
        deserializer::decode_attributes,
        root_as_city_feature, root_as_header,
        serializer::{to_columns, to_fcb_attribute},
        CityFeature, CityFeatureArgs, CityObject, CityObjectArgs, Header, HeaderArgs,
    };

    use super::*;

    use anyhow::Result;
    use flatbuffers::FlatBufferBuilder;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    #[test]
    fn test_add_attributes() -> Result<()> {
        let json_data = json!({
            "attributes": {
                "int": -10,
                "uint": 5,
                "bool": true,
                "float": 1.0,
                "string": "hoge",
                "array": [1, 2, 3],
                "json": {
                    "hoge": "fuga"
                },
                "null": null
            }
        });

        let mut attr_schema: AttributeSchema = AttributeSchema::new();

        attr_schema.add_attributes(&json_data["attributes"]);

        // Check if the schema contains the expected keys and types
        assert_eq!(attr_schema.get("int").unwrap().1, ColumnType::Long);
        assert_eq!(attr_schema.get("uint").unwrap().1, ColumnType::ULong);
        assert_eq!(attr_schema.get("bool").unwrap().1, ColumnType::Bool);
        assert_eq!(attr_schema.get("float").unwrap().1, ColumnType::Double);
        assert_eq!(attr_schema.get("string").unwrap().1, ColumnType::String);
        assert_eq!(attr_schema.get("array").unwrap().1, ColumnType::Json); //TODO: check if this is correct
        assert_eq!(attr_schema.get("json").unwrap().1, ColumnType::Json);

        Ok(())
    }

    #[test]
    fn test_attribute_serialization() -> Result<()> {
        let test_cases = vec![
            // Case 1: Same schema
            (
                json!({
                        "int": -10,
                        "uint": 5,
                        "bool": true,
                        "float": 1.0,
                        "string": "hoge",
                        "array": [1, 2, 3],
                        "json": {
                            "hoge": "fuga"
                        }
                }),
                json!({
                        "int": -10,
                        "uint": 5,
                            "bool": true,
                        "float": 1.0,
                        "string": "hoge",
                        "array": [1, 2, 3],
                        "json": {
                            "hoge": "fuga"
                    },
                }),
                json!({
                    "attributes": {
                        "int": -10,
                        "uint": 5,
                        "bool": true,
                        "float": 1.0,
                        "string": "hoge",
                        "array": [1, 2, 3],
                        "json": {
                            "hoge": "fuga"
                        }
                    }
                }),
                "same schema",
            ),
            // Case 2: JSON with null value
            (
                json!({
                            "int": -10,
                        "uint": 5,
                        "bool": true,
                        "float": 1.0,
                        "string": "hoge",
                        "array": [1, 2, 3],
                        "json": {
                            "hoge": "fuga"
                        },
                        "exception": null
                }),
                json!({
                            "int": -10,
                        "uint": 5,
                        "bool": true,
                        "float": 1.0,
                        "string": "hoge",
                        "array": [1, 2, 3],
                        "json": {
                            "hoge": "fuga"
                        },
                }),
                json!({
                    "attributes": {
                        "int": -10,
                        "uint": 5,
                        "bool": true,
                        "float": 1.0,
                        "string": "hoge",
                        "array": [1, 2, 3],
                        "json": {
                            "hoge": "fuga"
                        },
                        "exception": 1000
                    }
                }),
                "JSON with null value",
            ),
            // Case 3: JSON is empty
            (
                json!({}),
                json!({}),
                json!({
                    "attributes": {
                        "int": -10,
                        "uint": 5,
                        "bool": true,
                        "float": 1.0,
                        "string": "hoge",
                        "array": [1, 2, 3],
                        "json": {
                            "hoge": "fuga"
                        },
                        "exception": 1000
                    }
                }),
                "JSON is empty",
            ),
        ];

        for (input, expected, schema, test_name) in test_cases {
            println!("Testing case: {test_name}");

            let attrs = &input;
            let attr_schema = &schema["attributes"];

            // Create and encode with schema
            let mut fbb = FlatBufferBuilder::new();
            let mut common_schema = AttributeSchema::new();
            common_schema.add_attributes(attr_schema);

            let columns = to_columns(&mut fbb, &common_schema);
            let header = {
                let version = fbb.create_string("1.0.0");
                Header::create(
                    &mut fbb,
                    &HeaderArgs {
                        version: Some(version),
                        columns: Some(columns),
                        ..Default::default()
                    },
                )
            };
            fbb.finish(header, None);

            // Decode and verify
            let finished_data = fbb.finished_data();
            let header_buf = root_as_header(finished_data).unwrap();

            let mut fbb = FlatBufferBuilder::new();
            let feature = {
                let (attr_buf, _) = to_fcb_attribute(&mut fbb, attrs, &common_schema);
                let city_object = {
                    let id = fbb.create_string("test");
                    CityObject::create(
                        &mut fbb,
                        &CityObjectArgs {
                            id: Some(id),
                            attributes: Some(attr_buf),
                            ..Default::default()
                        },
                    )
                };
                let objects = fbb.create_vector(&[city_object]);
                let cf_id = fbb.create_string("test_feature");
                CityFeature::create(
                    &mut fbb,
                    &CityFeatureArgs {
                        id: Some(cf_id),
                        objects: Some(objects),
                        ..Default::default()
                    },
                )
            };

            fbb.finish(feature, None);

            let finished_data = fbb.finished_data();
            let feature_buf = root_as_city_feature(finished_data).unwrap();
            let attributes = feature_buf.objects().unwrap().get(0).attributes().unwrap();

            let decoded = decode_attributes(&header_buf.columns().unwrap(), attributes);

            assert_eq!(
                expected, decoded,
                "decoded data should match original for {}",
                test_name
            );
        }

        Ok(())
    }
}
