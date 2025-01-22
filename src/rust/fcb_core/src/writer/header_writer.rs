use crate::serializer::to_fcb_header;
use cjseq::CityJSON;
use flatbuffers::FlatBufferBuilder;
use packed_rtree::PackedRTree;

use super::attribute::AttributeSchema;

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
}

/// Configuration options for header writing process
pub struct HeaderWriterOptions {
    /// Whether to write index information
    pub write_index: bool,
    pub feature_count: u64,
    /// Size of the index node
    pub index_node_size: u16,
}

impl Default for HeaderWriterOptions {
    fn default() -> Self {
        HeaderWriterOptions {
            write_index: true,
            index_node_size: PackedRTree::DEFAULT_NODE_SIZE,
            feature_count: 0,
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
    pub fn new(
        cj: CityJSON,
        header_options: Option<HeaderWriterOptions>,
        attr_schema: AttributeSchema,
    ) -> HeaderWriter<'a> {
        Self::new_with_options(header_options.unwrap_or_default(), cj, attr_schema)
    }

    /// Creates a new HeaderWriter with specific configuration
    ///
    /// # Arguments
    ///
    /// * `options` - Configuration for the header writing process
    /// * `cj` - The CityJSON data to write
    pub fn new_with_options(
        mut options: HeaderWriterOptions,
        cj: CityJSON,
        attr_schema: AttributeSchema,
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
        }
    }

    /// Finalizes the header and returns it as a byte vector
    ///
    /// # Returns
    ///
    /// A size-prefixed FlatBuffer containing the serialized header
    pub fn finish_to_header(mut self) -> Vec<u8> {
        let header = to_fcb_header(
            &mut self.fbb,
            &self.cj,
            self.header_options,
            &self.attr_schema,
        );
        self.fbb.finish_size_prefixed(header, None);
        self.fbb.finished_data().to_vec()
    }
}
