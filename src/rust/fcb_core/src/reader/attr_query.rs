use anyhow::Result;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::io::{self, Read, Seek, SeekFrom};

use crate::error::Error;
use crate::{attribute, Header};
use bst::{BufferedIndex, ByteSerializableType, IndexSerializable, OrderedFloat};
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
            println!("  - Loading index for field: {}", col.name());
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
            operator: operator.clone(),
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
        // Debug the file structure
        println!("File structure debugging:");
        println!("  - Magic bytes: 8 bytes at position 0");
        println!("  - Header size: {} bytes", self.buffer.header_buf.len());

        // Get the current position (should be at the start of the file)
        let start_pos = self.reader.stream_position()?;
        println!("Current reader position before seeking: {}", start_pos);

        // Skip the rtree index bytes; we know the correct offset for that
        let rtree_offset = self.rtree_index_size();
        println!("R-tree index size: {}", rtree_offset);
        self.reader.seek(SeekFrom::Current(rtree_offset as i64))?;

        // Now we should be at the start of the attribute indices
        let attr_index_start_pos = self.reader.stream_position()?;
        println!("Attribute index start position: {}", attr_index_start_pos);

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

            println!("Processing attribute index for field: {}", field_name);
            println!("  - Column index: {}", column_idx);
            println!("  - Index length: {}", index_size);
            println!("  - Current offset: {}", current_offset);
            println!(
                "  - Absolute position: {}",
                attr_index_start_pos + current_offset
            );

            // Store the offset for this field
            index_offsets.insert(field_name, attr_index_start_pos + current_offset);

            // Skip over this index to position at the next one
            current_offset += index_size;
            self.reader.seek(SeekFrom::Current(index_size as i64))?;
        }

        // Reset reader position to the start of attribute indices
        self.reader.seek(SeekFrom::Start(attr_index_start_pos))?;
        println!(
            "Reset to attribute index start position: {}",
            self.reader.stream_position()?
        );

        println!("Index offsets: {:?}", index_offsets);

        // Debug: Read a small portion of the first index to check the format
        if !index_offsets.is_empty() {
            let first_field = index_offsets.keys().next().unwrap().clone();
            let first_offset = *index_offsets.get(&first_field).unwrap();
            // let column_type = columns
            //     .iter()
            //     .find(|col| col.name() == first_field)
            //     .ok_or(Error::AttributeIndexNotFound)?
            //     .type_();

            // Seek to the first index
            self.reader.seek(SeekFrom::Start(first_offset))?;
            println!(
                "Seeking to first index at position: {}",
                self.reader.stream_position()?
            );

            // Read the first 16 bytes
            let mut debug_buffer = vec![0u8; 16];
            self.reader.read_exact(&mut debug_buffer)?;
            println!(
                "First 16 bytes of index data for {}: {:?}",
                first_field, debug_buffer
            );

            // Try to interpret the first 4 bytes as a type ID
            if debug_buffer.len() >= 4 {
                let type_id = ByteSerializableType::from_bytes(&debug_buffer[0..4])?;
                println!("Type ID from first 4 bytes: {:?}", type_id);
            }

            // Reset position again
            self.reader.seek(SeekFrom::Start(attr_index_start_pos))?;
        }

        // Create a StreamableMultiIndex from the reader
        println!("Creating StreamableMultiIndex...");
        println!(
            "Reader position before creating StreamableMultiIndex: {}",
            self.reader.stream_position()?
        );

        // Try to create the StreamableMultiIndex with detailed error handling
        let streamable_index =
            match StreamableMultiIndex::from_reader(&mut self.reader, &index_offsets) {
                Ok(index) => {
                    println!("Successfully created StreamableMultiIndex");
                    index
                }
                Err(e) => {
                    println!("Error creating StreamableMultiIndex: {:?}", e);

                    // Instead of falling back to MultiIndex, let's try to understand the issue
                    println!(
                        "Current reader position after error: {}",
                        self.reader.stream_position()?
                    );

                    // Let's try to read the first few bytes of each index to see what's there
                    self.reader.seek(SeekFrom::Start(attr_index_start_pos))?;
                    println!(
                        "Reset to attribute index start position: {}",
                        self.reader.stream_position()?
                    );

                    for (field_name, offset) in &index_offsets {
                        self.reader
                            .seek(SeekFrom::Start(attr_index_start_pos + *offset))?;
                        println!(
                            "Reading index for field {} at position {}",
                            field_name,
                            self.reader.stream_position()?
                        );

                        let mut header_bytes = vec![0u8; 12]; // Read type ID (4 bytes) and entry count (8 bytes)
                        if let Ok(_) = self.reader.read_exact(&mut header_bytes) {
                            let type_id = u32::from_le_bytes([
                                header_bytes[0],
                                header_bytes[1],
                                header_bytes[2],
                                header_bytes[3],
                            ]);
                            let entry_count = u64::from_le_bytes([
                                header_bytes[4],
                                header_bytes[5],
                                header_bytes[6],
                                header_bytes[7],
                                header_bytes[8],
                                header_bytes[9],
                                header_bytes[10],
                                header_bytes[11],
                            ]);
                            println!("  - Type ID: {}", type_id);
                            println!("  - Entry count: {}", entry_count);
                        } else {
                            println!("  - Failed to read header bytes");
                        }
                    }

                    return Err(Error::IndexCreationError(format!(
                        "Failed to create streamable index: {}",
                        e
                    )));
                }
            };

        // Create a query from the AttrQuery
        let query_obj = build_query(&query);
        println!("Query conditions: {:?}", query_obj.conditions);

        // Execute the streaming query
        println!("Executing streaming query...");
        println!(
            "Reader position before streaming query: {}",
            self.reader.stream_position()?
        );

        let result = match streamable_index.stream_query(&mut self.reader, &query_obj) {
            Ok(res) => {
                println!(
                    "Successfully executed streaming query, found {} results",
                    res.len()
                );
                res
            }
            Err(e) => {
                println!("Error executing streaming query: {:?}", e);
                println!(
                    "Current reader position after error: {}",
                    self.reader.stream_position()?
                );
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
