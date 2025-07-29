use crate::packed_rtree::{calc_extent, hilbert_sort, NodeItem, PackedRTree};
use crate::MAGIC_BYTES;
use attr_index::build_attribute_index_for_attr;
use attribute::AttributeSchema;
use cjseq::{CityJSON, CityJSONFeature, Transform as CjTransform};
use feature_writer::{AttributeFeatureOffset, FeatureWriter};
use header_writer::{HeaderWriter, HeaderWriterOptions};
use serializer::AttributeIndexInfo;

use crate::error::Result;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
mod attr_index;
pub mod attribute;
pub mod error;
pub mod feature_writer;
pub mod geom_encoder;
pub mod header_writer;
pub mod serializer;
/// Main writer for FlatCityBuf (FCB) format
///
/// FcbWriter handles the serialization of CityJSON data into the FCB binary format.
/// It manages both header and feature writing, using a temporary file for feature storage
/// before final assembly.
pub struct FcbWriter<'a> {
    /// Temporary buffer for storing features before final assembly
    tmpout: BufWriter<File>,
    /// Writer for the FCB header section
    header_writer: HeaderWriter<'a>,
    /// Optional writer for features
    feat_writer: Option<FeatureWriter<'a>>,

    transform: CjTransform,
    /// Offset of the feature in the feature data section
    feat_offsets: Vec<FeatureOffset>,
    feat_nodes: Vec<NodeItem>,
    attr_schema: AttributeSchema,
    semantic_attr_schema: Option<AttributeSchema>,
    // temporary storage for attribute index entries
    attribute_index_entries: HashMap<usize, AttributeFeatureOffset>,
}

#[derive(Clone, PartialEq, Debug)]
struct FeatureOffset {
    temp_feature_id: usize,
    offset: usize,
    size: usize,
}

impl<'a> FcbWriter<'a> {
    /// Creates a new FCB writer instance
    ///
    /// # Arguments
    ///
    /// * `cj` - The CityJSON data to be written
    /// * `header_option` - Optional configuration for header writing
    /// * `first_feature` - Optional first feature to begin writing
    ///
    /// # Returns
    ///
    /// A Result containing the new FcbWriter instance
    pub fn new(
        cj: CityJSON,
        header_option: Option<HeaderWriterOptions>,
        attr_schema: Option<AttributeSchema>,
        semantic_attr_schema: Option<AttributeSchema>,
    ) -> Result<Self> {
        let attr_schema = attr_schema.unwrap_or_default();

        let transform = cj.transform.clone();
        let header_writer = HeaderWriter::new(
            cj,
            header_option,
            attr_schema.clone(),
            semantic_attr_schema.clone(),
        );
        Ok(Self {
            header_writer,
            transform,
            feat_writer: None,
            tmpout: BufWriter::new(tempfile::tempfile()?),
            attr_schema,
            semantic_attr_schema,
            feat_offsets: Vec::new(),
            feat_nodes: Vec::new(),
            attribute_index_entries: HashMap::new(),
        })
    }

    /// Writes the current feature to the temporary buffer
    ///
    /// # Returns
    ///
    /// A Result indicating success or failure of the write operation
    fn write_feature(&mut self) -> Result<()> {
        let transform = &self.transform;

        if let Some(feat_writer) = &mut self.feat_writer {
            let feat_buf = feat_writer.finish_to_feature();

            let mut attr_feature_offset = feat_writer.attribute_feature_offsets.clone();

            let mut node = Self::actual_bbox(transform, &feat_writer.bbox);
            node.offset = self.feat_offsets.len() as u64;
            self.feat_nodes.push(node);

            let tempoffset = self
                .feat_offsets
                .last()
                .map(|it| it.offset + it.size)
                .unwrap_or(0);

            attr_feature_offset.offset = tempoffset;
            self.attribute_index_entries
                .insert(self.feat_offsets.len(), attr_feature_offset);

            self.feat_offsets.push(FeatureOffset {
                temp_feature_id: self.feat_offsets.len(),
                offset: tempoffset,
                size: feat_buf.len(),
            });

            self.tmpout.write_all(&feat_buf)?;
        }
        Ok(())
    }

    fn actual_bbox(transform: &CjTransform, bbox: &NodeItem) -> NodeItem {
        let scale_x = transform.scale[0];
        let scale_y = transform.scale[1];
        let translate_x = transform.translate[0];
        let translate_y = transform.translate[1];
        NodeItem::bounds(
            bbox.min_x * scale_x + translate_x,
            bbox.min_y * scale_y + translate_y,
            bbox.max_x * scale_x + translate_x,
            bbox.max_y * scale_y + translate_y,
        )
    }

    /// Adds a new feature to be written
    ///
    /// # Arguments
    ///
    /// * `feature` - The CityJSON feature to add
    ///
    /// # Returns
    ///
    /// A Result indicating success or failure of the operation
    pub fn add_feature(&mut self, feature: &'a CityJSONFeature) -> Result<()> {
        if self.feat_writer.is_none() {
            self.feat_writer = Some(FeatureWriter::new(
                feature,
                self.attr_schema.clone(),
                self.semantic_attr_schema.clone(),
                self.header_writer
                    .header_options
                    .attribute_indices
                    .as_ref()
                    .map(|a| a.iter().map(|(name, _)| name.clone()).collect()),
            ));
        }

        if let Some(feat_writer) = &mut self.feat_writer {
            feat_writer.add_feature(feature);
            self.write_feature()?;
        }

        Ok(())
    }

    /// Writes the complete FCB dataset to the output
    ///
    /// This method assembles the final FCB file by writing:
    /// 1. Magic bytes
    /// 2. Header
    /// 3. Feature data
    ///
    /// # Arguments
    ///
    /// * `out` - The output destination implementing Write
    ///
    /// # Returns
    ///
    /// A Result indicating success or failure of the write operation
    pub fn write(mut self, mut out: impl Write) -> Result<()> {
        let mut attr_indices = self.header_writer.header_options.attribute_indices.clone();

        // sort attribute indices by schema index (ascending)
        if let Some(ref mut indices) = attr_indices {
            indices.sort_by_key(|(name, _)| {
                self.attr_schema
                    .get(name)
                    .map(|(idx, _)| *idx)
                    .unwrap_or(u16::MAX)
            });
        }

        out.write_all(&MAGIC_BYTES)?;
        let index_node_size = self.header_writer.header_options.index_node_size;

        let mut rtree_buf = Vec::new();
        if index_node_size > 0 && !self.feat_nodes.is_empty() {
            let extent = calc_extent(&self.feat_nodes);
            hilbert_sort(&mut self.feat_nodes, &extent);
            let mut offset = 0;
            let index_nodes = self
                .feat_nodes
                .iter()
                .map(|temp_node| {
                    let feat = &self.feat_offsets[temp_node.offset as usize];
                    let mut node = temp_node.clone();
                    node.offset = offset;
                    offset += feat.size as u64;
                    node
                })
                .collect::<Vec<_>>();
            let tree = PackedRTree::build(&index_nodes, &extent, index_node_size)?;
            tree.stream_write(&mut rtree_buf)?;
        }

        self.tmpout.rewind()?;
        let unsorted_feature_output = self.tmpout.into_inner().map_err(|e| e.into_error())?;
        let mut unsorted_feature_reader = BufReader::new(unsorted_feature_output);

        let mut sorted_feature_buf = Vec::with_capacity(2048);

        for node in &self.feat_nodes {
            let feat = &self.feat_offsets[node.offset as usize];
            unsorted_feature_reader.seek(SeekFrom::Start(feat.offset as u64))?;

            if let Some(attr_index_entry) =
                self.attribute_index_entries.get_mut(&feat.temp_feature_id)
            {
                attr_index_entry.offset = sorted_feature_buf.len();
                attr_index_entry.size = feat.size;
            }

            let cur_len = sorted_feature_buf.len();
            sorted_feature_buf.resize(cur_len + feat.size, 0);
            unsorted_feature_reader.read_exact(&mut sorted_feature_buf[cur_len..])?;
        }

        // build attribute index buffers in sorted order
        let mut attr_index_buf: Vec<u8> = Vec::new();
        let mut attr_index_info: Vec<AttributeIndexInfo> = Vec::new();
        if let Some(sorted_indices) = &attr_indices {
            for (name, bf_opt) in sorted_indices {
                let bf = bf_opt.unwrap_or(crate::static_btree::DEFAULT_BRANCHING_FACTOR);
                if let Ok((buf, info)) = build_attribute_index_for_attr(
                    name,
                    &self.attr_schema,
                    &self.attribute_index_entries,
                    bf,
                ) {
                    attr_index_info.push(info);
                    attr_index_buf.extend(&buf);
                }
            }
        }

        // write header with attribute indices metadata
        self.header_writer.attribute_indices_info = Some(attr_index_info);
        let header_buf = self.header_writer.finish_to_header()?;
        out.write_all(&header_buf)?;

        // write spatial index (if any), attribute index bytes, then feature data
        out.write_all(&rtree_buf)?;
        out.write_all(&attr_index_buf)?;
        out.write_all(&sorted_feature_buf)?;

        Ok(())
    }
}
