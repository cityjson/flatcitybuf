use std::collections::HashMap;

use crate::error::{Error, Result};
use crate::fb::ColumnType;
use crate::static_btree::{Entry, FixedStringKey, Key, MemoryIndex};
use chrono::{DateTime, Utc};
use ordered_float::OrderedFloat;

use super::{
    attribute::{AttributeIndexEntry, AttributeSchema},
    serializer::AttributeIndexInfo,
    AttributeFeatureOffset,
};

fn build_index_generic<T, F>(
    schema_index: u16,
    attribute_entries: &HashMap<usize, AttributeFeatureOffset>,
    extract: F,
    branching_factor: u16,
) -> Result<(Vec<u8>, AttributeIndexInfo)>
where
    T: Key,
    F: Fn(&AttributeIndexEntry) -> Option<T>,
{
    let mut entries: Vec<Entry<T>> = Vec::new();

    for feature in attribute_entries.values() {
        for entry in &feature.index_entries {
            let Some(key) = extract(entry) else {
                continue;
            };
            entries.push(Entry {
                key,
                offset: feature.offset as u64,
            });
        }
    }

    let index = MemoryIndex::<T>::build(&entries, branching_factor)?;
    let mut buf = Vec::new();
    index.serialize(&mut buf)?;
    let buf_length = buf.len();
    Ok((
        buf,
        AttributeIndexInfo {
            index: schema_index,
            length: buf_length as u32,
            branching_factor: index.branching_factor(),
            num_unique_items: index.num_items() as u32,
        },
    ))
}

pub(super) fn build_attribute_index_for_attr(
    attr_name: &str,
    schema: &AttributeSchema,
    attribute_entries: &HashMap<usize, AttributeFeatureOffset>,
    branching_factor: u16,
) -> Result<(Vec<u8>, AttributeIndexInfo)> {
    // Look up attribute info from schema; if not found, return None
    let (schema_index, coltype) = schema.get(attr_name).ok_or(Error::AttributeIndexNotFound)?;

    match *coltype {
        ColumnType::Bool => build_index_generic::<bool, _>(
            *schema_index,
            attribute_entries,
            |entry| {
                if let AttributeIndexEntry::Bool { index, val } = entry {
                    if *index == *schema_index {
                        Some(*val)
                    } else {
                        None
                    }
                } else {
                    None
                }
            },
            branching_factor,
        ),
        ColumnType::Int => build_index_generic::<i32, _>(
            *schema_index,
            attribute_entries,
            |entry| {
                if let AttributeIndexEntry::Int { index, val } = entry {
                    if *index == *schema_index {
                        Some(*val)
                    } else {
                        None
                    }
                } else {
                    None
                }
            },
            branching_factor,
        ),
        ColumnType::UInt => build_index_generic::<u32, _>(
            *schema_index,
            attribute_entries,
            |entry| {
                if let AttributeIndexEntry::UInt { index, val } = entry {
                    if *index == *schema_index {
                        Some(*val)
                    } else {
                        None
                    }
                } else {
                    None
                }
            },
            branching_factor,
        ),
        ColumnType::Long => build_index_generic::<i64, _>(
            *schema_index,
            attribute_entries,
            |entry| {
                if let AttributeIndexEntry::Long { index, val } = entry {
                    if *index == *schema_index {
                        Some(*val)
                    } else {
                        None
                    }
                } else {
                    None
                }
            },
            branching_factor,
        ),
        ColumnType::ULong => build_index_generic::<u64, _>(
            *schema_index,
            attribute_entries,
            |entry| {
                if let AttributeIndexEntry::ULong { index, val } = entry {
                    if *index == *schema_index {
                        Some(*val)
                    } else {
                        None
                    }
                } else {
                    None
                }
            },
            branching_factor,
        ),
        ColumnType::Float => build_index_generic::<OrderedFloat<f32>, _>(
            *schema_index,
            attribute_entries,
            |entry| {
                if let AttributeIndexEntry::Float { index, val } = entry {
                    if *index == *schema_index {
                        Some(OrderedFloat(*val))
                    } else {
                        None
                    }
                } else {
                    None
                }
            },
            branching_factor,
        ),
        ColumnType::Double => build_index_generic::<OrderedFloat<f64>, _>(
            *schema_index,
            attribute_entries,
            |entry| {
                if let AttributeIndexEntry::Double { index, val } = entry {
                    if *index == *schema_index {
                        Some(OrderedFloat(*val))
                    } else {
                        None
                    }
                } else {
                    None
                }
            },
            branching_factor,
        ),
        ColumnType::String => build_index_generic::<FixedStringKey<50>, _>(
            *schema_index,
            attribute_entries,
            |entry| {
                if let AttributeIndexEntry::String { index, val } = entry {
                    if *index == *schema_index {
                        Some(FixedStringKey::from_str(val))
                    } else {
                        None
                    }
                } else {
                    None
                }
            },
            branching_factor,
        ),
        ColumnType::DateTime => build_index_generic::<DateTime<Utc>, _>(
            *schema_index,
            attribute_entries,
            |entry| {
                if let AttributeIndexEntry::DateTime { index, val } = entry {
                    if *index == *schema_index {
                        Some(*val)
                    } else {
                        None
                    }
                } else {
                    None
                }
            },
            branching_factor,
        ),
        ColumnType::Short => build_index_generic::<i16, _>(
            *schema_index,
            attribute_entries,
            |entry| {
                if let AttributeIndexEntry::Short { index, val } = entry {
                    if *index == *schema_index {
                        Some(*val)
                    } else {
                        None
                    }
                } else {
                    None
                }
            },
            branching_factor,
        ),
        ColumnType::UShort => build_index_generic::<u16, _>(
            *schema_index,
            attribute_entries,
            |entry| {
                if let AttributeIndexEntry::UShort { index, val } = entry {
                    if *index == *schema_index {
                        Some(*val)
                    } else {
                        None
                    }
                } else {
                    None
                }
            },
            branching_factor,
        ),
        ColumnType::Byte => build_index_generic::<u8, _>(
            *schema_index,
            attribute_entries,
            |entry| {
                if let AttributeIndexEntry::Byte { index, val } = entry {
                    if *index == *schema_index {
                        Some(*val)
                    } else {
                        None
                    }
                } else {
                    None
                }
            },
            branching_factor,
        ),
        ColumnType::UByte => build_index_generic::<u8, _>(
            *schema_index,
            attribute_entries,
            |entry| {
                if let AttributeIndexEntry::UByte { index, val } = entry {
                    if *index == *schema_index {
                        Some(*val)
                    } else {
                        None
                    }
                } else {
                    None
                }
            },
            branching_factor,
        ),
        ColumnType::Json => build_index_generic::<FixedStringKey<100>, _>(
            *schema_index,
            attribute_entries,
            |entry| {
                if let AttributeIndexEntry::Json { index, val } = entry {
                    if *index == *schema_index {
                        Some(FixedStringKey::from_str(val))
                    } else {
                        None
                    }
                } else {
                    None
                }
            },
            branching_factor,
        ),
        ColumnType::Binary => build_index_generic::<FixedStringKey<100>, _>(
            *schema_index,
            attribute_entries,
            |entry| {
                if let AttributeIndexEntry::Binary { index, val } = entry {
                    if *index == *schema_index {
                        Some(FixedStringKey::from_str(val))
                    } else {
                        None
                    }
                } else {
                    None
                }
            },
            branching_factor,
        ),
        _ => {
            println!("Unsupported column type for indexing: {coltype:?}");
            Err(Error::UnsupportedColumnType(format!("{coltype:?}")))
        }
    }
}
