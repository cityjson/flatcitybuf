use crate::error::Result;
use crate::packed_rtree::PackedRTree;
use crate::serializer::to_fcb_header;
use cjseq::CityJSON;
use flatbuffers::FlatBufferBuilder;

use super::{attribute::AttributeSchema, serializer::AttributeIndexInfo};

/// Writer for converting CityJSON header information to FlatBuffers format
pub struct HeaderWriter<'a> {
    /// FlatBuffers builder instance
    pub fbb: FlatBufferBuilder<'a>,
    /// Source CityJSON data
    pub cj: CityJSON,

    /// Configuration options for header writing
    pub header_options: HeaderWriterOptions,
    /// Attribute schema
    pub attr_schema: AttributeSchema,

    /// Semantic attribute schema
    pub semantic_attr_schema: Option<AttributeSchema>,
    /// Attribute indices
    pub(super) attribute_indices_info: Option<Vec<AttributeIndexInfo>>,
}

/// Configuration options for header writing process
#[derive(Debug, Clone)]
pub struct HeaderWriterOptions {
    /// Whether to write index information
    pub write_index: bool,
    pub feature_count: u64,
    /// Size of the index node
    pub index_node_size: u16,
    /// Attribute indices
    pub attribute_indices: Option<Vec<(String, Option<u16>)>>, // (field name, branching factor)
    /// Geographical extent
    pub geographical_extent: Option<[f64; 6]>,
}

impl Default for HeaderWriterOptions {
    fn default() -> Self {
        HeaderWriterOptions {
            write_index: true,
            index_node_size: PackedRTree::DEFAULT_NODE_SIZE,
            feature_count: 0,
            attribute_indices: None,
            geographical_extent: None,
        }
    }
}

impl<'a> HeaderWriter<'a> {
    /// Creates a new HeaderWriter with optional configuration
    ///
    /// # Arguments
    ///
    /// * `cj` - The CityJSON data to write
    /// * `header_options` - Optional configuration for the header writing process
    pub(super) fn new(
        cj: CityJSON,
        header_options: Option<HeaderWriterOptions>,
        attr_schema: AttributeSchema,
        semantic_attr_schema: Option<AttributeSchema>,
    ) -> HeaderWriter<'a> {
        Self::new_with_options(
            header_options.unwrap_or_default(),
            cj,
            attr_schema,
            semantic_attr_schema,
        )
    }

    /// Creates a new HeaderWriter with specific configuration
    ///
    /// # Arguments
    ///
    /// * `options` - Configuration for the header writing process
    /// * `cj` - The CityJSON data to write
    fn new_with_options(
        mut options: HeaderWriterOptions,
        cj: CityJSON,
        attr_schema: AttributeSchema,
        semantic_attr_schema: Option<AttributeSchema>,
    ) -> HeaderWriter<'a> {
        let fbb = FlatBufferBuilder::new();
        let index_node_size = if options.write_index {
            PackedRTree::DEFAULT_NODE_SIZE
        } else {
            0
        };
        options.index_node_size = index_node_size;
        HeaderWriter {
            fbb,
            cj,
            header_options: options,
            attr_schema,
            semantic_attr_schema,
            attribute_indices_info: None,
        }
    }

    /// Finalizes the header and returns it as a byte vector
    ///
    /// # Returns
    ///
    /// A size-prefixed FlatBuffer containing the serialized header
    pub(super) fn finish_to_header(mut self) -> Result<Vec<u8>> {
        let header = to_fcb_header(
            &mut self.fbb,
            &self.cj,
            self.header_options,
            &self.attr_schema,
            self.semantic_attr_schema.as_ref(),
            self.attribute_indices_info
                .as_ref()
                .filter(|info| !info.is_empty())
                .map(|info| info.as_slice()),
        )?;
        self.fbb.finish_size_prefixed(header, None);
        Ok(self.fbb.finished_data().to_vec())
    }
}
