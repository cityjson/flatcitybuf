use std::io::{self, Read, Seek, SeekFrom};

use anyhow::{anyhow, Ok, Result};
pub use bst::*;

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
) -> Result<()> {
    let length = attr_info.length();
    let mut buffer = vec![0; length as usize];
    reader.read_exact(&mut buffer)?;

    if let Some(col) = columns.iter().find(|col| col.index() == attr_info.index()) {
        if query.iter().any(|(name, _, _)| col.name() == name) {
            match col.type_() {
                ColumnType::Int => {
                    let index = SortedIndex::<i32>::deserialize(&mut buffer.as_slice())?;
                    multi_index.add_index(col.name().to_string(), Box::new(index));
                }
                ColumnType::Long => {
                    let index = SortedIndex::<i64>::deserialize(&mut buffer.as_slice())?;
                    multi_index.add_index(col.name().to_string(), Box::new(index));
                }
                ColumnType::Float => {
                    let index =
                        SortedIndex::<OrderedFloat<f32>>::deserialize(&mut buffer.as_slice())?;
                    multi_index.add_index(col.name().to_string(), Box::new(index));
                }
                ColumnType::Double => {
                    let index =
                        SortedIndex::<OrderedFloat<f64>>::deserialize(&mut buffer.as_slice())?;
                    multi_index.add_index(col.name().to_string(), Box::new(index));
                }
                ColumnType::String => {
                    let index = SortedIndex::<String>::deserialize(&mut buffer.as_slice())?;
                    multi_index.add_index(col.name().to_string(), Box::new(index));
                }
                ColumnType::Bool => {
                    let index = SortedIndex::<bool>::deserialize(&mut buffer.as_slice())?;
                    multi_index.add_index(col.name().to_string(), Box::new(index));
                }
                ColumnType::DateTime => {
                    let index = SortedIndex::<DateTime<Utc>>::deserialize(&mut buffer.as_slice())?;
                    multi_index.add_index(col.name().to_string(), Box::new(index));
                }
                _ => return Err(anyhow!("unsupported column type")),
            }
        }
    }
    Ok(())
}

fn byte_serializable_to_bytes(value: &ByteSerializableValue) -> Vec<u8> {
    match value {
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

pub fn build_query(query: &AttrQuery) -> Query {
    Query {
        conditions: query
            .iter()
            .map(|(name, operator, value)| QueryCondition {
                field: name.to_string(),
                operator: *operator,
                key: byte_serializable_to_bytes(value),
            })
            .collect(),
    }
}

impl<R: Read + Seek> FcbReader<R> {
    pub fn select_attr_query(mut self, query: AttrQuery) -> Result<FeatureIter<R, Seekable>> {
        // query: vec<(field_name, operator, value)>
        let header = self.buffer.header();
        let attr_index_entries = header
            .attribute_index()
            .ok_or_else(|| anyhow!("attribute index not found"))?;
        let columns = header
            .columns()
            .ok_or_else(|| anyhow!("no columns found in header"))?;
        let columns: Vec<Column> = columns.iter().collect();

        // skip the rtree index bytes; we know the correct offset for that
        let rtree_offset = self.rtree_index_size();
        self.reader.seek(SeekFrom::Current(rtree_offset as i64))?;

        let mut multi_index = MultiIndex::new();

        // Process each attribute index entry.
        for attr_info in attr_index_entries.iter() {
            process_attr_index_entry(
                &mut self.reader,
                &mut multi_index,
                &columns,
                &query,
                attr_info,
            )?;
        }

        let query = build_query(&query);

        let mut result = multi_index.query(query);
        // sort result so it can read features in order
        result.sort();
        let header_size = self.buffer.header_buf.len();
        let feature_offset = FeatureOffset {
            magic_bytes: 8,
            header: header_size as u64,
            rtree_index: self.rtree_index_size(),
            attributes: self.attr_index_size(),
        };
        let total_feat_count = result.len() as u64;
        Ok(FeatureIter::<R, Seekable>::new(
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

        let mut multi_index = MultiIndex::new();

        for attr_info in attr_index_entries.iter() {
            process_attr_index_entry(
                &mut self.reader,
                &mut multi_index,
                &columns,
                &query,
                attr_info,
            )?;
        }

        let query = build_query(&query);

        let mut result = multi_index.query(query);
        result.sort();
        let header_size = self.buffer.header_buf.len();
        let feature_offset = FeatureOffset {
            magic_bytes: 8,
            header: header_size as u64,
            rtree_index: self.rtree_index_size(),
            attributes: self.attr_index_size(),
        };
        let total_feat_count = result.len() as u64;
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
