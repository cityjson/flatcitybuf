use std::collections::HashMap;

use bst::{BufferedIndex, ByteSerializable, IndexSerializable, KeyValue};
use chrono::NaiveDateTime;
use ordered_float::OrderedFloat;

use crate::ColumnType;

use super::{
    attribute::{AttributeIndexEntry, AttributeSchema},
    serializer::AttributeIndexInfo,
    AttributeFeatureOffset,
};

fn build_index_generic<T, F>(
    schema_index: u16,
    attribute_entries: &HashMap<usize, AttributeFeatureOffset>,
    extract: F,
) -> Option<(Vec<u8>, AttributeIndexInfo)>
where
    T: Ord + Clone + ByteSerializable + 'static,
    F: Fn(&AttributeIndexEntry) -> Option<T>,
{
    let mut entries: Vec<KeyValue<T>> = Vec::new();

    for feature in attribute_entries.values() {
        for entry in &feature.index_entries {
            if let Some(value) = extract(entry) {
                if let Some(kv) = entries.iter_mut().find(|kv| kv.key == value) {
                    kv.offsets.push(feature.offset as u64);
                } else {
                    entries.push(KeyValue {
                        key: value,
                        offsets: vec![feature.offset as u64],
                    });
                }
            }
        }
    }

    let mut sorted_index = BufferedIndex::new();
    sorted_index.build_index(entries);
    let mut buf = Vec::new();
    sorted_index.serialize(&mut buf).ok()?;
    let buf_length = buf.len();
    Some((
        buf,
        AttributeIndexInfo {
            index: schema_index,
            length: buf_length as u32,
        },
    ))
}

pub(super) fn build_attribute_index_for_attr(
    attr_name: &str,
    schema: &AttributeSchema,
    attribute_entries: &HashMap<usize, AttributeFeatureOffset>,
) -> Option<(Vec<u8>, AttributeIndexInfo)> {
    // Look up attribute info from schema; if not found, return None
    let (schema_index, coltype) = schema.get(attr_name)?;

    match *coltype {
        ColumnType::Bool => {
            build_index_generic::<bool, _>(*schema_index, attribute_entries, |entry| {
                if let AttributeIndexEntry::Bool { index, val } = entry {
                    if *index == *schema_index {
                        Some(*val)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
        }
        ColumnType::Int => {
            build_index_generic::<i32, _>(*schema_index, attribute_entries, |entry| {
                if let AttributeIndexEntry::Int { index, val } = entry {
                    if *index == *schema_index {
                        Some(*val)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
        }
        ColumnType::UInt => {
            build_index_generic::<u32, _>(*schema_index, attribute_entries, |entry| {
                if let AttributeIndexEntry::UInt { index, val } = entry {
                    if *index == *schema_index {
                        Some(*val)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
        }
        ColumnType::Long => {
            build_index_generic::<i64, _>(*schema_index, attribute_entries, |entry| {
                if let AttributeIndexEntry::Long { index, val } = entry {
                    if *index == *schema_index {
                        Some(*val)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
        }
        ColumnType::ULong => {
            build_index_generic::<u64, _>(*schema_index, attribute_entries, |entry| {
                if let AttributeIndexEntry::ULong { index, val } = entry {
                    if *index == *schema_index {
                        Some(*val)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
        }
        ColumnType::Float => {
            build_index_generic::<OrderedFloat<f32>, _>(*schema_index, attribute_entries, |entry| {
                if let AttributeIndexEntry::Float { index, val } = entry {
                    if *index == *schema_index {
                        Some(OrderedFloat(*val))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
        }
        ColumnType::Double => {
            build_index_generic::<OrderedFloat<f64>, _>(*schema_index, attribute_entries, |entry| {
                if let AttributeIndexEntry::Double { index, val } = entry {
                    if *index == *schema_index {
                        Some(OrderedFloat(*val))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
        }
        ColumnType::String => {
            build_index_generic::<String, _>(*schema_index, attribute_entries, |entry| {
                if let AttributeIndexEntry::String { index, val } = entry {
                    if *index == *schema_index {
                        Some(val.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
        }
        ColumnType::DateTime => {
            build_index_generic::<NaiveDateTime, _>(*schema_index, attribute_entries, |entry| {
                if let AttributeIndexEntry::DateTime { index, val } = entry {
                    if *index == *schema_index {
                        Some(*val)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
        }
        _ => {
            println!("Unsupported column type for indexing: {:?}", coltype);
            None
        }
    }
}
