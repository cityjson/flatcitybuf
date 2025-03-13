use anyhow::Result;
use std::collections::HashMap;
use std::io::{self, Read, Seek, SeekFrom};

use crate::error::Error;
use bst::{BufferedIndex, IndexSerializable, OrderedFloat};
pub use bst::{
    ByteSerializableValue, MultiIndex, Operator, Query, Query as AttributeQuery, QueryCondition,
    StreamableMultiIndex,
};

use chrono::{DateTime, Utc};

use crate::{AttributeIndex, Column, ColumnType, FeatureOffset};

use super::{
    reader_trait::{NotSeekable, Seekable},
    FcbReader, FeatureIter,
};

pub type AttrQuery = Vec<(String, Operator, ByteSerializableValue)>;

pub fn process_attr_index_entry<R: Read>(
    reader: &mut R,
    multi_index: &mut MultiIndex,
    columns: &[Column],
    query: &AttrQuery,
    attr_info: &AttributeIndex,
) -> Result<(), Error> {
    let length = attr_info.length();
    let mut buffer = vec![0; length as usize];
    reader.read_exact(&mut buffer)?;

    if let Some(col) = columns.iter().find(|col| col.index() == attr_info.index()) {
        if query.iter().any(|(name, _, _)| col.name() == name) {
            match col.type_() {
                ColumnType::Int => {
                    let index = BufferedIndex::<i32>::deserialize(&mut buffer.as_slice())?;
                    multi_index.add_index(col.name().to_string(), Box::new(index));
                }
                ColumnType::Long => {
                    let index = BufferedIndex::<i64>::deserialize(&mut buffer.as_slice())?;
                    multi_index.add_index(col.name().to_string(), Box::new(index));
                }
                ColumnType::Float => {
                    let index =
                        BufferedIndex::<OrderedFloat<f32>>::deserialize(&mut buffer.as_slice())?;
                    multi_index.add_index(col.name().to_string(), Box::new(index));
                }
                ColumnType::Double => {
                    let index =
                        BufferedIndex::<OrderedFloat<f64>>::deserialize(&mut buffer.as_slice())?;
                    multi_index.add_index(col.name().to_string(), Box::new(index));
                }
                ColumnType::String => {
                    let index = BufferedIndex::<String>::deserialize(&mut buffer.as_slice())?;
                    multi_index.add_index(col.name().to_string(), Box::new(index));
                }
                ColumnType::Bool => {
                    let index = BufferedIndex::<bool>::deserialize(&mut buffer.as_slice())?;
                    multi_index.add_index(col.name().to_string(), Box::new(index));
                }
                ColumnType::DateTime => {
                    let index =
                        BufferedIndex::<DateTime<Utc>>::deserialize(&mut buffer.as_slice())?;
                    multi_index.add_index(col.name().to_string(), Box::new(index));
                }
                _ => return Err(Error::UnsupportedColumnType(col.name().to_string())),
            }
        } else {
            println!("  - Skipping index for field: {}", col.name());
        }
    }
    Ok(())
}

pub fn build_query(query: &AttrQuery) -> Query {
    let conditions = query
        .iter()
        .map(|(field, operator, value)| QueryCondition {
            field: field.clone(),
            operator: *operator,
            key: value.to_bytes(),
        })
        .collect();
    Query { conditions }
}

impl<R: Read + Seek> FcbReader<R> {
    pub fn select_attr_query(
        mut self,
        query: AttrQuery,
    ) -> Result<FeatureIter<R, Seekable>, Error> {
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

        let columns = header.columns().ok_or(Error::NoColumnsInHeader)?;
        let columns: Vec<Column> = columns.iter().collect();

        // Get the current position (should be at the start of the file)
        let start_pos = self.reader.stream_position()?;

        // Skip the rtree index bytes; we know the correct offset for that
        let rtree_offset = self.rtree_index_size();
        self.reader.seek(SeekFrom::Current(rtree_offset as i64))?;

        // Now we should be at the start of the attribute indices
        let attr_index_start_pos = self.reader.stream_position()?;

        // Create a mapping from field names to index offsets
        let mut index_offsets = HashMap::new();
        let mut current_offset = 0;

        // First pass: build the index_offsets map and skip over all indices
        for attr_info in attr_index_entries.iter() {
            let column_idx = attr_info.index();
            let field_name = columns
                .iter()
                .find(|col| col.index() == column_idx)
                .ok_or(Error::AttributeIndexNotFound)?
                .name()
                .to_string();
            let index_size = attr_info.length() as u64;

            // Store the offset for this field
            index_offsets.insert(field_name, attr_index_start_pos + current_offset);

            // Skip over this index to position at the next one
            current_offset += index_size;
            self.reader.seek(SeekFrom::Current(index_size as i64))?;
        }

        // Reset reader position to the start of attribute indices
        self.reader.seek(SeekFrom::Start(attr_index_start_pos))?;

        // Try to create the StreamableMultiIndex with detailed error handling
        let streamable_index =
            match StreamableMultiIndex::from_reader(&mut self.reader, &index_offsets) {
                Ok(index) => index,
                Err(e) => {
                    return Err(Error::IndexCreationError(format!(
                        "Failed to create streamable index: {}",
                        e
                    )));
                }
            };

        // Create a query from the AttrQuery
        let query_obj = build_query(&query);

        let result = match streamable_index.stream_query(&mut self.reader, &query_obj) {
            Ok(res) => res,
            Err(e) => {
                return Err(Error::QueryExecutionError(format!(
                    "Failed to execute streaming query: {}",
                    e
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
            attributes: self.attr_index_size(),
        };

        let total_feat_count = result_vec.len() as u64;

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
    ) -> anyhow::Result<FeatureIter<R, NotSeekable>> {
        // query: vec<(field_name, operator, value)>
        let header = self.buffer.header();
        let attr_index_entries = header
            .attribute_index()
            .ok_or_else(|| anyhow::anyhow!("attribute index not found"))?;
        let columns: Vec<Column> = header
            .columns()
            .ok_or_else(|| anyhow::anyhow!("no columns found in header"))?
            .iter()
            .collect();

        // Instead of seeking, read and discard the rtree index bytes; we know the correct offset for that.
        let rtree_offset = self.rtree_index_size();
        io::copy(&mut (&mut self.reader).take(rtree_offset), &mut io::sink())?;

        // Since we can't use StreamableMultiIndex with a non-seekable reader,
        // we'll still use MultiIndex but optimize the process to minimize memory usage
        let mut multi_index = MultiIndex::new();

        // Process each attribute index entry, but only load the ones needed for our query
        let query_fields: Vec<String> = query.iter().map(|(field, _, _)| field.clone()).collect();

        for attr_info in attr_index_entries.iter() {
            let column_idx = attr_info.index();
            let field_name = columns[column_idx as usize].name().to_string();

            // Only process this attribute if it's used in the query
            if query_fields.contains(&field_name) {
                process_attr_index_entry(
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
        let mut result = multi_index.query(query_obj);
        result.sort();

        let header_size = self.buffer.header_buf.len();
        let feature_offset = FeatureOffset {
            magic_bytes: 8,
            header: header_size as u64,
            rtree_index: self.rtree_index_size(),
            attributes: self.attr_index_size(),
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
