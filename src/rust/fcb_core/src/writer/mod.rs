use crate::MAGIC_BYTES;
use anyhow::Result;
use attribute::AttributeSchema;
use cjseq::{CityJSON, CityJSONFeature, Transform as CjTransform};
use feature_writer::FeatureWriter;
use header_writer::{HeaderWriter, HeaderWriterOptions};
use packed_rtree::{calc_extent, hilbert_sort, NodeItem, PackedRTree};

use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};

pub mod attribute;
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
    /// Offset of the feature in the feature data section
    ///
    /// transform: CjTransform
    transform: CjTransform,
    feat_offsets: Vec<FeatureOffset>,
    feat_nodes: Vec<NodeItem>,
    attr_schema: AttributeSchema,
}

#[derive(Clone, PartialEq, Debug)]
struct FeatureOffset {
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
    ) -> Result<Self> {
        let attr_schema = attr_schema.unwrap_or_default();
        let transform = cj.transform.clone();
        let header_writer = HeaderWriter::new(cj, header_option, attr_schema.clone());
        Ok(Self {
            header_writer,
            transform,
            feat_writer: None,
            tmpout: BufWriter::new(tempfile::tempfile()?),
            attr_schema,
            feat_offsets: Vec::new(),
            feat_nodes: Vec::new(),
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
            let mut node = Self::actual_bbox(transform, &feat_writer.bbox);

            node.offset = self.feat_offsets.len() as u64;
            self.feat_nodes.push(node);

            let tempoffset = self
                .feat_offsets
                .last()
                .map(|it| it.offset + it.size)
                .unwrap_or(0);
            self.feat_offsets.push(FeatureOffset {
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
        NodeItem::new(
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
            self.feat_writer = Some(FeatureWriter::new(feature, self.attr_schema.clone()));
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
        out.write_all(&MAGIC_BYTES)?;
        let index_node_size = self.header_writer.header_options.index_node_size;
        let header_buf = self.header_writer.finish_to_header();
        out.write_all(&header_buf)?;

        if index_node_size > 0 && !self.feat_nodes.is_empty() {
            let extent = calc_extent(&self.feat_nodes);
            println!("extent: {:?}", extent);
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
            tree.stream_write(&mut out)?;
        }

        self.tmpout.rewind()?;
        let unsorted_feature_output = self.tmpout.into_inner().map_err(|e| e.into_error())?;
        let mut unsorted_feature_reader = BufReader::new(unsorted_feature_output);
        {
            let mut buf = Vec::with_capacity(2038);
            for node in &self.feat_nodes {
                let feat = &self.feat_offsets[node.offset as usize];
                unsorted_feature_reader.seek(SeekFrom::Start(feat.offset as u64))?;
                buf.resize(feat.size, 0);
                unsorted_feature_reader.read_exact(&mut buf)?;
                out.write_all(&buf)?;
            }
        }

        Ok(())
    }
}
