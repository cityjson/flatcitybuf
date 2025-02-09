use std::collections::HashMap;

use bst::{IndexSerializable, KeyValue, SortedIndex};
use chrono::NaiveDateTime;
use ordered_float::OrderedFloat;

use crate::ColumnType;

use super::{
    attribute::{AttributeIndexEntry, AttributeSchema},
    serializer::AttributeIndexInfo,
    AttributeFeatureOffset,
};

pub(super) fn build_attribute_index_for_attr(
    attr_name: &str,
    schema: &AttributeSchema,
    attribute_entries: &HashMap<usize, AttributeFeatureOffset>,
) -> Option<(Vec<u8>, AttributeIndexInfo)> {
    // Look up attribute info from schema. For example, suppose schema.get returns Option<(u16, ColumnType)>
    let (schema_index, coltype) = schema.get(attr_name)?; // if not found, return None

    match *coltype {
        ColumnType::Bool => {
            let mut entries: Vec<KeyValue<bool>> = Vec::new();
            // Iterate over each feature's attribute data.
            for feature in attribute_entries.values() {
                // Look for a matching attribute entry in this feature.
                for entry in &feature.index_entries {
                    if let AttributeIndexEntry::Bool { index, val } = entry {
                        if index == schema_index {
                            // Check if we already have an entry with this key.
                            if let Some(kv) = entries.iter_mut().find(|kv| kv.key == *val) {
                                kv.offsets.push(feature.offset as u64);
                            } else {
                                entries.push(KeyValue {
                                    key: *val,
                                    offsets: vec![feature.offset as u64],
                                });
                            }
                        }
                    }
                }
            }
            let mut sorted_index = SortedIndex::new();
            sorted_index.build_index(entries);
            // Serialize sorted_index into a Vec<u8>
            let mut buf = Vec::new();
            sorted_index.serialize(&mut buf).unwrap();
            let buf_length = buf.len();
            Some((
                buf,
                AttributeIndexInfo {
                    index: *schema_index,
                    length: buf_length as u32,
                },
            ))
        }
        ColumnType::Int => {
            let mut entries: Vec<KeyValue<i32>> = Vec::new();
            for feature in attribute_entries.values() {
                for entry in &feature.index_entries {
                    if let AttributeIndexEntry::Int { index, val } = entry {
                        if index == schema_index {
                            if let Some(kv) = entries.iter_mut().find(|kv| kv.key == *val) {
                                kv.offsets.push(feature.offset as u64);
                            } else {
                                entries.push(KeyValue {
                                    key: *val,
                                    offsets: vec![feature.offset as u64],
                                });
                            }
                        }
                    }
                }
            }
            let mut sorted_index = SortedIndex::new();
            sorted_index.build_index(entries);
            let mut buf = Vec::new();
            sorted_index.serialize(&mut buf).unwrap();
            let buf_length = buf.len();
            Some((
                buf,
                AttributeIndexInfo {
                    index: *schema_index,
                    length: buf_length as u32,
                },
            ))
        }
        ColumnType::UInt => {
            let mut entries: Vec<KeyValue<u32>> = Vec::new();
            for feature in attribute_entries.values() {
                for entry in &feature.index_entries {
                    if let AttributeIndexEntry::UInt { index, val } = entry {
                        if index == schema_index {
                            if let Some(kv) = entries.iter_mut().find(|kv| kv.key == *val) {
                                kv.offsets.push(feature.offset as u64);
                            } else {
                                entries.push(KeyValue {
                                    key: *val,
                                    offsets: vec![feature.offset as u64],
                                });
                            }
                        }
                    }
                }
            }
            let mut sorted_index = SortedIndex::new();
            sorted_index.build_index(entries);
            let mut buf = Vec::new();
            sorted_index.serialize(&mut buf).unwrap();
            let buf_length = buf.len();
            Some((
                buf,
                AttributeIndexInfo {
                    index: *schema_index,
                    length: buf_length as u32,
                },
            ))
        }
        ColumnType::Long => {
            let mut entries: Vec<KeyValue<i64>> = Vec::new();
            for feature in attribute_entries.values() {
                for entry in &feature.index_entries {
                    if let AttributeIndexEntry::Long { index, val } = entry {
                        if index == schema_index {
                            if let Some(kv) = entries.iter_mut().find(|kv| kv.key == *val) {
                                kv.offsets.push(feature.offset as u64);
                            } else {
                                entries.push(KeyValue {
                                    key: *val,
                                    offsets: vec![feature.offset as u64],
                                });
                            }
                        }
                    }
                }
            }
            let mut sorted_index = SortedIndex::new();
            sorted_index.build_index(entries);
            let mut buf = Vec::new();
            sorted_index.serialize(&mut buf).unwrap();
            let buf_length = buf.len();
            Some((
                buf,
                AttributeIndexInfo {
                    index: *schema_index,
                    length: buf_length as u32,
                },
            ))
        }
        ColumnType::ULong => {
            let mut entries: Vec<KeyValue<u64>> = Vec::new();
            for feature in attribute_entries.values() {
                for entry in &feature.index_entries {
                    if let AttributeIndexEntry::ULong { index, val } = entry {
                        if index == schema_index {
                            if let Some(kv) = entries.iter_mut().find(|kv| kv.key == *val) {
                                kv.offsets.push(feature.offset as u64);
                            } else {
                                entries.push(KeyValue {
                                    key: *val,
                                    offsets: vec![feature.offset as u64],
                                });
                            }
                        }
                    }
                }
            }
            let mut sorted_index = SortedIndex::new();
            sorted_index.build_index(entries);
            let mut buf = Vec::new();
            sorted_index.serialize(&mut buf).unwrap();
            let buf_length = buf.len();
            Some((
                buf,
                AttributeIndexInfo {
                    index: *schema_index,
                    length: buf_length as u32,
                },
            ))
        }
        ColumnType::Float => {
            let mut entries: Vec<KeyValue<OrderedFloat<f32>>> = Vec::new();
            for feature in attribute_entries.values() {
                for entry in &feature.index_entries {
                    if let AttributeIndexEntry::Float { index, val } = entry {
                        if index == schema_index {
                            if let Some(kv) = entries.iter_mut().find(|kv| kv.key == *val) {
                                kv.offsets.push(feature.offset as u64);
                            } else {
                                entries.push(KeyValue {
                                    key: OrderedFloat(*val),
                                    offsets: vec![feature.offset as u64],
                                });
                            }
                        }
                    }
                }
            }
            let mut sorted_index = SortedIndex::new();
            sorted_index.build_index(entries);
            let mut buf = Vec::new();
            sorted_index.serialize(&mut buf).unwrap();
            let buf_length = buf.len();
            Some((
                buf,
                AttributeIndexInfo {
                    index: *schema_index,
                    length: buf_length as u32,
                },
            ))
        }
        ColumnType::Double => {
            let mut entries: Vec<KeyValue<OrderedFloat<f64>>> = Vec::new();
            for feature in attribute_entries.values() {
                for entry in &feature.index_entries {
                    if let AttributeIndexEntry::Double { index, val } = entry {
                        if index == schema_index {
                            if let Some(kv) =
                                entries.iter_mut().find(|kv| kv.key == OrderedFloat(*val))
                            {
                                kv.offsets.push(feature.offset as u64);
                            } else {
                                entries.push(KeyValue {
                                    key: OrderedFloat(*val),
                                    offsets: vec![feature.offset as u64],
                                });
                            }
                        }
                    }
                }
            }
            let mut sorted_index = SortedIndex::new();
            sorted_index.build_index(entries);
            let mut buf = Vec::new();
            sorted_index.serialize(&mut buf).unwrap();
            let buf_length = buf.len();
            Some((
                buf,
                AttributeIndexInfo {
                    index: *schema_index,
                    length: buf_length as u32,
                },
            ))
        }
        ColumnType::String => {
            let mut entries: Vec<KeyValue<String>> = Vec::new();
            for feature in attribute_entries.values() {
                for entry in &feature.index_entries {
                    if let AttributeIndexEntry::String { index, val } = entry {
                        if index == schema_index {
                            if let Some(kv) = entries.iter_mut().find(|kv| kv.key == val.clone()) {
                                kv.offsets.push(feature.offset as u64);
                            } else {
                                entries.push(KeyValue {
                                    key: val.clone(),
                                    offsets: vec![feature.offset as u64],
                                });
                            }
                        }
                    }
                }
            }
            let mut sorted_index = SortedIndex::new();
            sorted_index.build_index(entries);
            let mut buf = Vec::new();
            sorted_index.serialize(&mut buf).unwrap();
            let buf_length = buf.len();
            Some((
                buf,
                AttributeIndexInfo {
                    index: *schema_index,
                    length: buf_length as u32,
                },
            ))
        }
        ColumnType::DateTime => {
            // Assuming you use NaiveDateTime for DateTime
            let mut entries: Vec<KeyValue<NaiveDateTime>> = Vec::new();
            for feature in attribute_entries.values() {
                for entry in &feature.index_entries {
                    if let AttributeIndexEntry::DateTime { index, val } = entry {
                        if index == schema_index {
                            if let Some(kv) = entries.iter_mut().find(|kv| kv.key == *val) {
                                kv.offsets.push(feature.offset as u64);
                            } else {
                                entries.push(KeyValue {
                                    key: *val,
                                    offsets: vec![feature.offset as u64],
                                });
                            }
                        }
                    }
                }
            }
            let mut sorted_index = SortedIndex::new();
            sorted_index.build_index(entries);
            let mut buf = Vec::new();
            sorted_index.serialize(&mut buf).unwrap();
            let buf_length = buf.len();
            Some((
                buf,
                AttributeIndexInfo {
                    index: *schema_index,
                    length: buf_length as u32,
                },
            ))
        }
        _ => {
            println!("Unsupported column type for indexing: {:?}", coltype);
            None
        }
    }
}
