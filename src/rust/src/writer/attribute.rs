use crate::header_generated::ColumnType;
use byteorder::{ByteOrder, LittleEndian};
use serde_json::Value;
use std::{collections::HashMap, u16};

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
                Some(ColumnType::ULong) //TODO: check if this is correct. To accurately guess the type, we need to know the range of the value. But, to do that, we need to read all the data.
            }
        }
        Value::String(_) => Some(ColumnType::String),
        Value::Array(_) => Some(ColumnType::Json),
        Value::Object(_) => Some(ColumnType::Json),
        _ => None,
    }
}

pub fn attr_size(coltype: &ColumnType, colval: &Value) -> usize {
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
            size_of::<u32>() + json.as_bytes().len()
        }
        ColumnType::Binary => size_of::<u32>() + colval.as_str().unwrap().len(), //TODO: check if this is correct
        _ => unreachable!(),
    }
}

pub fn encode_attributes_with_schema(attr: &Value, schema: &AttributeSchema) -> Vec<u8> {
    let mut out = Vec::new();
    let mut sorted_schema: Vec<_> = schema.iter().collect();
    sorted_schema.sort_by_key(|(_, (index, _))| *index);

    for (name, (index, coltype)) in sorted_schema {
        let (_, val) = attr
            .as_object()
            .unwrap()
            .iter()
            .find(|(k, _)| *k == name)
            .unwrap();

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

#[cfg(test)]
mod tests {
    use super::*;

    use anyhow::Result;
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
        assert_eq!(attr_schema.get("int").unwrap().1, ColumnType::Int);
        assert_eq!(attr_schema.get("uint").unwrap().1, ColumnType::UInt);
        assert_eq!(attr_schema.get("bool").unwrap().1, ColumnType::Bool);
        assert_eq!(attr_schema.get("float").unwrap().1, ColumnType::Float);
        assert_eq!(attr_schema.get("string").unwrap().1, ColumnType::String);
        assert_eq!(attr_schema.get("array").unwrap().1, ColumnType::Json);
        assert_eq!(attr_schema.get("json").unwrap().1, ColumnType::Json);

        Ok(())
    }
}
