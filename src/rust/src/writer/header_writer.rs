use crate::fcb_serializer::to_fcb_header;
use cjseq::CityJSON;
use flatbuffers::FlatBufferBuilder;

use super::attribute::AttributeSchema;

/// Writer for converting CityJSON header information to FlatBuffers format
pub struct HeaderWriter<'a> {
    /// FlatBuffers builder instance
    fbb: FlatBufferBuilder<'a>,
    /// Source CityJSON data
    cj: CityJSON,
    /// Configuration options for header writing
    header_options: HeaderWriterOptions,
    /// Attribute schema
    attr_schema: AttributeSchema,
}

/// Configuration options for header writing process
pub struct HeaderWriterOptions {
    /// Whether to write index information
    pub write_index: bool,
    /// Additional metadata for the header
    pub header_metadata: HeaderMetadata,
}

/// Additional metadata to be included in the header
pub struct HeaderMetadata {
    /// Total count of features in the CityJSON data
    pub features_count: u64,
}

impl Default for HeaderWriterOptions {
    fn default() -> Self {
        HeaderWriterOptions {
            write_index: true,
            header_metadata: HeaderMetadata { features_count: 0 },
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
        options: HeaderWriterOptions,
        cj: CityJSON,
        attr_schema: AttributeSchema,
    ) -> HeaderWriter<'a> {
        let fbb = FlatBufferBuilder::new();

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
            self.header_options.header_metadata,
            &self.attr_schema,
        );
        self.fbb.finish_size_prefixed(header, None);
        self.fbb.finished_data().to_vec()
    }
}
