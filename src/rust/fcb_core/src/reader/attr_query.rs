use crate::static_btree::{
    FixedStringKey, Float, KeyType, MemoryIndex, MemoryMultiIndex, MultiIndex, Operator, Query,
    QueryCondition, StreamIndex, StreamMultiIndex,
};
use std::collections::HashMap;
use std::io::{self, Cursor, Read, Seek, SeekFrom};
use std::ops::Range;

use crate::error::{Error, Result};

use chrono::{DateTime, Utc};

use crate::fb::Column;
use crate::fb::ColumnType;
use crate::{AttributeIndex, FeatureOffset};

use super::{
    reader_trait::{NotSeekable, Seekable},
    FcbReader, FeatureIter,
};

pub type AttrQuery = Vec<(String, Operator, KeyType)>;

pub fn add_indices_to_multi_memory_index<R: Read>(
    mut data: R,
    multi_index: &mut MemoryMultiIndex,
    columns: &[Column],
    query: &AttrQuery,
    attr_info: &AttributeIndex,
) -> Result<()> {
    let length = attr_info.length();
    let mut buf = vec![0; length as usize];
    data.read_exact(&mut buf)?;
    let mut buf = Cursor::new(buf);
    if let Some(col) = columns.iter().find(|col| col.index() == attr_info.index()) {
        if query.iter().any(|(name, _, _)| col.name() == name) {
            match col.type_() {
                ColumnType::Int => {
                    let index = MemoryIndex::<i32>::from_buf(
                        &mut buf,
                        attr_info.num_unique_items() as usize,
                        attr_info.branching_factor(),
                    )?;
                    multi_index.add_i32_index(col.name().to_string(), index);
                }
                ColumnType::Float => {
                    let index = MemoryIndex::<Float<f32>>::from_buf(
                        &mut buf,
                        attr_info.num_unique_items() as usize,
                        attr_info.branching_factor(),
                    )?;
                    multi_index.add_f32_index(col.name().to_string(), index);
                }
                ColumnType::Double => {
                    let index = MemoryIndex::<Float<f64>>::from_buf(
                        &mut buf,
                        attr_info.num_unique_items() as usize,
                        attr_info.branching_factor(),
                    )?;
                    multi_index.add_f64_index(col.name().to_string(), index);
                }
                ColumnType::String => {
                    let index = MemoryIndex::<FixedStringKey<50>>::from_buf(
                        &mut buf,
                        attr_info.num_unique_items() as usize,
                        attr_info.branching_factor(),
                    )?;
                    multi_index.add_string_index50(col.name().to_string(), index);
                }
                ColumnType::Bool => {
                    let index = MemoryIndex::<bool>::from_buf(
                        &mut buf,
                        attr_info.num_unique_items() as usize,
                        attr_info.branching_factor(),
                    )?;
                    multi_index.add_bool_index(col.name().to_string(), index);
                }
                ColumnType::DateTime => {
                    let index = MemoryIndex::<DateTime<Utc>>::from_buf(
                        &mut buf,
                        attr_info.num_unique_items() as usize,
                        attr_info.branching_factor(),
                    )?;
                    multi_index.add_datetime_index(col.name().to_string(), index);
                }
                ColumnType::Short => {
                    let index = MemoryIndex::<i16>::from_buf(
                        &mut buf,
                        attr_info.num_unique_items() as usize,
                        attr_info.branching_factor(),
                    )?;
                    multi_index.add_i16_index(col.name().to_string(), index);
                }
                ColumnType::UShort => {
                    let index = MemoryIndex::<u16>::from_buf(
                        &mut buf,
                        attr_info.num_unique_items() as usize,
                        attr_info.branching_factor(),
                    )?;
                    multi_index.add_u16_index(col.name().to_string(), index);
                }
                ColumnType::UInt => {
                    let index = MemoryIndex::<u32>::from_buf(
                        &mut buf,
                        attr_info.num_unique_items() as usize,
                        attr_info.branching_factor(),
                    )?;
                    multi_index.add_u32_index(col.name().to_string(), index);
                }
                ColumnType::ULong => {
                    let index = MemoryIndex::<u64>::from_buf(
                        &mut buf,
                        attr_info.num_unique_items() as usize,
                        attr_info.branching_factor(),
                    )?;
                    multi_index.add_u64_index(col.name().to_string(), index);
                }
                ColumnType::Byte => {
                    let index = MemoryIndex::<i8>::from_buf(
                        &mut buf,
                        attr_info.num_unique_items() as usize,
                        attr_info.branching_factor(),
                    )?;
                    multi_index.add_i8_index(col.name().to_string(), index);
                }
                ColumnType::UByte => {
                    let index = MemoryIndex::<u8>::from_buf(
                        &mut buf,
                        attr_info.num_unique_items() as usize,
                        attr_info.branching_factor(),
                    )?;
                    multi_index.add_u8_index(col.name().to_string(), index);
                }
                _ => return Err(Error::UnsupportedColumnType(col.name().to_string())),
            }
        } else {
            println!("  - Skipping index for field: {}", col.name());
        }
    }
    Ok(())
}

pub fn add_indices_to_multi_stream_index<R: Read + Seek>(
    multi_index: &mut StreamMultiIndex,
    columns: &[Column],
    attr_info: &AttributeIndex,
    index_begin: usize,
) -> Result<()> {
    if let Some(col) = columns.iter().find(|col| col.index() == attr_info.index()) {
        // TODO: now it assuming to add all indices to the multi_index. However, we should only add the indices that are used in the query. To do that, we need to change the implementation of StreamMultiIndex. Current StreamMultiIndex's `add_index` method assumes that all indices are added to the multi_index. We'll change it to take Range<usize> as an argument.
        let index_begin = index_begin as u64;
        match col.type_() {
            ColumnType::Int => {
                let index = StreamIndex::<i32>::new(
                    attr_info.num_unique_items() as usize,
                    attr_info.branching_factor(),
                    index_begin,
                    attr_info.length() as u64,
                );
                multi_index.add_i32_index(col.name().to_string(), index, attr_info.length() as u64);
            }
            ColumnType::Float => {
                let index = StreamIndex::<Float<f32>>::new(
                    attr_info.num_unique_items() as usize,
                    attr_info.branching_factor(),
                    index_begin,
                    attr_info.length() as u64,
                );
                multi_index.add_f32_index(col.name().to_string(), index, attr_info.length() as u64);
            }
            ColumnType::Double => {
                let index = StreamIndex::<Float<f64>>::new(
                    attr_info.num_unique_items() as usize,
                    attr_info.branching_factor(),
                    index_begin,
                    attr_info.length() as u64,
                );
                multi_index.add_f64_index(col.name().to_string(), index, attr_info.length() as u64);
            }
            ColumnType::String => {
                let index = StreamIndex::<FixedStringKey<50>>::new(
                    attr_info.num_unique_items() as usize,
                    attr_info.branching_factor(),
                    index_begin,
                    attr_info.length() as u64,
                );
                multi_index.add_string_index50(
                    col.name().to_string(),
                    index,
                    attr_info.length() as u64,
                );
            }
            ColumnType::Bool => {
                let index = StreamIndex::<bool>::new(
                    attr_info.num_unique_items() as usize,
                    attr_info.branching_factor(),
                    index_begin,
                    attr_info.length() as u64,
                );
                multi_index.add_bool_index(
                    col.name().to_string(),
                    index,
                    attr_info.length() as u64,
                );
            }
            ColumnType::DateTime => {
                let index = StreamIndex::<DateTime<Utc>>::new(
                    attr_info.num_unique_items() as usize,
                    attr_info.branching_factor(),
                    index_begin,
                    attr_info.length() as u64,
                );
                multi_index.add_datetime_index(
                    col.name().to_string(),
                    index,
                    attr_info.length() as u64,
                );
            }
            ColumnType::Short => {
                let index = StreamIndex::<i16>::new(
                    attr_info.num_unique_items() as usize,
                    attr_info.branching_factor(),
                    index_begin,
                    attr_info.length() as u64,
                );
                multi_index.add_i16_index(col.name().to_string(), index, attr_info.length() as u64);
            }
            ColumnType::UShort => {
                let index = StreamIndex::<u16>::new(
                    attr_info.num_unique_items() as usize,
                    attr_info.branching_factor(),
                    index_begin,
                    attr_info.length() as u64,
                );
                multi_index.add_u16_index(col.name().to_string(), index, attr_info.length() as u64);
            }
            ColumnType::UInt => {
                let index = StreamIndex::<u32>::new(
                    attr_info.num_unique_items() as usize,
                    attr_info.branching_factor(),
                    index_begin,
                    attr_info.length() as u64,
                );
                multi_index.add_u32_index(col.name().to_string(), index, attr_info.length() as u64);
            }
            ColumnType::ULong => {
                let index = StreamIndex::<u64>::new(
                    attr_info.num_unique_items() as usize,
                    attr_info.branching_factor(),
                    index_begin,
                    attr_info.length() as u64,
                );
                multi_index.add_u64_index(col.name().to_string(), index, attr_info.length() as u64);
            }
            ColumnType::Byte => {
                let index = StreamIndex::<i8>::new(
                    attr_info.num_unique_items() as usize,
                    attr_info.branching_factor(),
                    index_begin,
                    attr_info.length() as u64,
                );
                multi_index.add_i8_index(col.name().to_string(), index, attr_info.length() as u64);
            }
            ColumnType::UByte => {
                let index = StreamIndex::<u8>::new(
                    attr_info.num_unique_items() as usize,
                    attr_info.branching_factor(),
                    index_begin,
                    attr_info.length() as u64,
                );
                multi_index.add_u8_index(col.name().to_string(), index, attr_info.length() as u64);
            }
            _ => return Err(Error::UnsupportedColumnType(col.name().to_string())),
        }
        // }
        // else {
        //     println!("  - Skipping index for field: {}", col.name());
        // }
    }
    Ok(())
}

pub fn build_query(query: &AttrQuery) -> Query {
    let conditions = query
        .iter()
        .map(|(field, operator, key)| {
            let owned_key = key.clone();
            QueryCondition {
                field: field.clone(),
                operator: *operator,
                key: owned_key,
            }
        })
        .collect();
    Query { conditions }
}

impl<R: Read + Seek> FcbReader<R> {
    pub fn select_attr_query(mut self, query: AttrQuery) -> Result<FeatureIter<R, Seekable>> {
        // query: vec<(field_name, operator, value)>
        let header = self.buffer.header();
        let attr_index_entries = header
            .attribute_index()
            .ok_or(Error::AttributeIndexNotFound)?;
        if attr_index_entries.is_empty() {
            return Err(Error::AttributeIndexNotFound);
        }

        let mut attr_index_entries: Vec<&AttributeIndex> = attr_index_entries.iter().collect();
        attr_index_entries.sort_by_key(|attr| attr.index());

        let columns = header
            .columns()
            .ok_or(Error::NoColumnsInHeader)?
            .iter()
            .collect::<Vec<_>>();

        // Range of attribute indices to be processed. HashMap<field_name, Range<usize>>
        let mut attr_index_range = HashMap::<String, Range<usize>>::new();
        let mut current_index = 0;
        for attr_info in attr_index_entries.iter() {
            let column = columns
                .iter()
                .find(|c| c.index() == attr_info.index())
                .ok_or(Error::AttributeIndexNotFound)?;
            let field_name = column.name().to_string();
            let index_begin = current_index;
            let index_end = index_begin + attr_info.length() as usize;
            attr_index_range.insert(
                field_name,
                Range {
                    start: index_begin,
                    end: index_end,
                },
            );
            current_index = index_end;
        }

        // Get the current position (should be at the start of the file)
        // let start_pos = self.reader.stream_position()?;

        // Skip the rtree index bytes; we know the correct offset for that
        let rtree_offset = self.rtree_index_size();
        self.reader.seek(SeekFrom::Current(rtree_offset as i64))?;

        // Now we should be at the start of the attribute indices
        let attr_index_start_pos = self.reader.stream_position()?;

        // Reset reader position to the start of attribute indices
        self.reader.seek(SeekFrom::Start(attr_index_start_pos))?;

        // Create a query from the AttrQuery
        let query_obj = build_query(&query);

        let mut multi_index = StreamMultiIndex::new();
        // iterate over the columens which are used in the query and is in columns and in attr_index_entries
        for attr_info in attr_index_entries.iter() {
            let column_idx = attr_info.index();
            let column = columns
                .iter()
                .find(|c| c.index() == column_idx)
                .ok_or(Error::AttributeIndexNotFound)?;
            // if query
            //     .iter()
            //     .any(|(name, _, _)| name.as_str() == column.name())

            let index_range = attr_index_range
                .get(column.name())
                .ok_or(Error::AttributeIndexNotFound)?;
            add_indices_to_multi_stream_index::<R>(
                &mut multi_index,
                &columns,
                attr_info,
                index_range.start,
            )?;
        }

        let result = match multi_index.query(&mut self.reader, &query_obj.conditions) {
            Ok(res) => res,
            Err(e) => {
                return Err(Error::QueryExecutionError(format!(
                    "Failed to execute streaming query: {e}"
                )));
            }
        };

        // Sort the results
        let mut result_vec: Vec<u64> = result.into_iter().collect();
        result_vec.sort();

        let header_size = self.buffer.header_buf.len();
        let feature_offset = FeatureOffset {
            magic_bytes: 8,
            header: header_size as u64,
            rtree_index: self.rtree_index_size(),
            attributes: self.attr_index_size() as u64,
        };

        let total_feat_count = result_vec.len() as u64;

        let attr_index_size = self.attr_index_size();
        self.reader.seek(SeekFrom::Start(
            attr_index_start_pos + attr_index_size as u64,
        ))?;

        Ok(FeatureIter::<R, Seekable>::new(
            self.reader,
            self.verify,
            self.buffer,
            None,
            Some(result_vec),
            feature_offset,
            total_feat_count,
        ))
    }
}

impl<R: Read> FcbReader<R> {
    pub fn select_attr_query_seq(
        mut self,
        query: AttrQuery,
    ) -> Result<FeatureIter<R, NotSeekable>> {
        // query: vec<(field_name, operator, value)>
        let header = self.buffer.header();
        let attr_index_entries = header
            .attribute_index()
            .ok_or(Error::AttributeIndexNotFound)?;
        let columns: Vec<Column> = header
            .columns()
            .ok_or(Error::NoColumnsInHeader)?
            .iter()
            .collect();

        // Instead of seeking, read and discard the rtree index bytes; we know the correct offset for that.
        let rtree_offset = self.rtree_index_size();
        io::copy(&mut (&mut self.reader).take(rtree_offset), &mut io::sink())?;

        // Since we can't use StreamableMultiIndex with a non-seekable reader,
        // we'll still use MultiIndex but optimize the process to minimize memory usage
        let mut multi_index = MemoryMultiIndex::new();

        // Process each attribute index entry, but only load the ones needed for our query
        let query_fields: Vec<String> = query.iter().map(|(field, _, _)| field.clone()).collect();

        for attr_info in attr_index_entries.iter() {
            let column_idx = attr_info.index();
            let field_name = columns[column_idx as usize].name().to_string();

            // Only process this attribute if it's used in the query
            if query_fields.contains(&field_name) {
                add_indices_to_multi_memory_index(
                    &mut self.reader,
                    &mut multi_index,
                    &columns,
                    &query,
                    attr_info,
                )?;
            } else {
                // Skip this attribute index if not needed
                let index_size = attr_info.length();
                io::copy(
                    &mut (&mut self.reader).take(index_size as u64),
                    &mut io::sink(),
                )?;
            }
        }

        // Build and execute the query
        let query_obj = build_query(&query);
        let mut result = multi_index.query(&query_obj.conditions)?;
        result.sort();

        let header_size = self.buffer.header_buf.len();
        let feature_offset = FeatureOffset {
            magic_bytes: 8,
            header: header_size as u64,
            rtree_index: self.rtree_index_size(),
            attributes: self.attr_index_size() as u64,
        };

        let total_feat_count = result.len() as u64;

        // Create and return the FeatureIter
        Ok(FeatureIter::<R, NotSeekable>::new(
            self.reader,
            self.verify,
            self.buffer,
            None,
            Some(result),
            feature_offset,
            total_feat_count,
        ))
    }
}
