use crate::error::Result;
use crate::packed_rtree::PackedRTree;
use crate::serializer::to_fcb_header;
use crate::static_btree::DEFAULT_BRANCHING_FACTOR;
use crate::ID_COLUMN_NAME;
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
    /// The index of the City Object id column, when the file gets an id
    /// index. It is not part of `attr_schema`, so no attribute is ever
    /// encoded under it.
    pub(super) id_column: Option<u16>,
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
    /// The branching factor of the index over City Object ids, or `None` to
    /// write no such index.
    ///
    /// The index holds the id of every City Object, a feature's children
    /// included, and is queried as the column
    /// [`ID_COLUMN_NAME`](crate::ID_COLUMN_NAME). The ids are not copied into
    /// the attributes: the header declares the column with
    /// `primary_key = true` and a reader takes its value from `CityObject.id`.
    /// A dataset whose attribute schema already has a column of that name
    /// keeps it, and gets no id index.
    pub id_index: Option<u16>,
}

impl Default for HeaderWriterOptions {
    fn default() -> Self {
        HeaderWriterOptions {
            write_index: true,
            index_node_size: PackedRTree::DEFAULT_NODE_SIZE,
            feature_count: 0,
            attribute_indices: None,
            geographical_extent: None,
            id_index: Some(DEFAULT_BRANCHING_FACTOR),
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
        // `index_node_size` is the caller's; only `write_index: false` may
        // override it, and then only to 0, which is how the header says "no
        // R-tree". Forcing DEFAULT_NODE_SIZE here made the field write-only
        // and every file's node size 16, so no reader could be tested
        // against a non-default one.
        if !options.write_index {
            options.index_node_size = 0;
        }
        // The next free column index. The dataset's own `id` attribute wins
        // over the id column: two columns of one name could not be told
        // apart in a query.
        let id_column = options
            .id_index
            .filter(|_| !attr_schema.contains_key(ID_COLUMN_NAME))
            .map(|_| {
                attr_schema
                    .values()
                    .map(|(index, _)| index + 1)
                    .max()
                    .unwrap_or(0)
            });
        HeaderWriter {
            fbb,
            cj,
            header_options: options,
            attr_schema,
            semantic_attr_schema,
            attribute_indices_info: None,
            id_column,
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
            self.id_column,
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
