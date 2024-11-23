use crate::error::Result;
// use crate::feature_writer::FeatureWriter;
use crate::header_generated::flat_city_buf::{
    Column, ColumnArgs, ColumnType, GeographicalExtent, Header, HeaderArgs, ReferenceSystem, ReferenceSystemArgs, Transform, Vector
};
use crate::{MAGIC_BYTES}
// use crate::packed_r_tree::{calc_extent, hilbert_sort, NodeItem, PackedRTree};
use flatbuffers::FlatBufferBuilder;
// use geozero::CoordDimensions;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};

// Note: Many parts of this code are derived from https://github.com/flatgeobuf/flatgeobuf/tree/master/src/rust

pub struct FcbWriter<'a> {
    tmpout: BufWriter<File>,
    fbb: FlatBufferBuilder<'a>,
    header_args: HeaderArgs<'a>,
    columns: Vec<flatbuffers::WIPOffset<Column<'a>>>,
    // feat_writer: FeatureWriter<'a>,
    // feat_offsets: Vec<FeatureOffset>,
    // feat_nodes: Vec<NodeItem>,
}

/// Options for FlatCityBuf writer
#[derive(Debug)]
pub struct FcbWriterOptions<'a> {
    /// Write index and sort features accordingly.
    // pub write_index: bool,
    // /// Detect geometry type when `geometry_type` is Unknown.
    // pub detect_type: bool,
    // /// Convert single to multi geometries, if `geometry_type` is multi type or Unknown
    // pub promote_to_multi: bool,
    /// CRS definition
    pub ref_system: FcbRefSys<'a>,
    // /// Does geometry have M dimension?
    // pub has_m: bool,
    // /// Does geometry have T dimension?
    // pub has_t: bool,
    // /// Does geometry have TM dimension?
    // pub has_tm: bool,
    // Dataset title
    pub title: Option<&'a str>,
    // Dataset description (intended for free form long text)
    // pub description: Option<&'a str>,
    // // Dataset metadata (intended to be application specific and
    // pub metadata: Option<&'a str>,
}

impl Default for FcbWriterOptions<'_> {
    fn default() -> Self {
        FcbWriterOptions {
            // write_index: true,
            // detect_type: true,
            // promote_to_multi: true,
            ref_system: Default::default(),
            // has_m: false,
            // has_t: false,
            // has_tm: false,
            title: None,
            // description: None,
            // metadata: None,
        }
    }
}

#[derive(Debug, Default)]
pub struct FcbRefSys<'a> {
    // /// Case-insensitive name of the defining organization e.g. EPSG or epsg (NULL = EPSG)
    // pub org: Option<&'a str>,
    // /// Numeric ID of the Spatial Reference System assigned by the organization (0 = unknown)
    // pub code: i32,
    // /// Human readable name of this SRS
    // pub name: Option<&'a str>,
    // /// Human readable description of this SRS
    // pub description: Option<&'a str>,
    // /// Well-known Text Representation of the Spatial Reference System
    // pub wkt: Option<&'a str>,
    /// Text ID of the Spatial Reference System assigned by the organization in the (rare) case when it is not an integer and thus cannot be set into code
    pub code_string: Option<&'a str>,
}

// Offsets in temporary file
struct FeatureOffset {
    offset: usize,
    size: usize,
}

impl<'a> FcbWriter<'a> {
    /// Configure FlatGeobuf headers for creating a new file with default options
    ///

    pub fn create(name: &str) -> Result<Self> {
        let options = FcbWriterOptions {
            // write_index: true,
            // detect_type: true,
            // promote_to_multi: true,
            ..Default::default()
        };
        FcbWriter::create_with_options(name, options)
    }

    pub fn create_with_options(name: &str, options: FcbWriterOptions) -> Result<Self> {
        let mut fbb = FlatBufferBuilder::new();

        // let index_node_size = if options.write_index {
        //     PackedRTree::DEFAULT_NODE_SIZE
        // } else {
        //     0
        // };
        // let index_node_size = 0; // TODO: implement index later
        let ref_system_args = ReferenceSystemArgs {
            authority: None,
            version: 0,
            code: 0,
            code_string: options.ref_system.code_string.map(|v| fbb.create_string(v)),
        };
        let header_args = HeaderArgs {
            transform: None,
            columns: None,
            features_count: 0,
            geographical_extent: None,
            reference_system: Some(ReferenceSystem::create(&mut fbb, &ref_system_args)),
            identifier: None,
            reference_date: None,
            title: None,
            poc_contact_name: None,
            poc_contact_type: None,
            poc_role: None,
            poc_phone: None,
            poc_email: None,
            poc_website: None,
            poc_address_thoroughfare_number: None,
            poc_address_thoroughfare_name: None,
            poc_address_locality: None,
            poc_address_postcode: None,
            poc_address_country: None,
            attributes: None,
        };

        // let feat_writer = FeatureWriter::with_dims(
        //     header_args.geometry_type,
        //     options.detect_type,
        //     options.promote_to_multi,
        //     // dims,
        // );

        let tmpout = BufWriter::new(tempfile::tempfile()?);

        Ok(FcbWriter {
            tmpout,
            fbb,
            header_args,
            columns: Vec::new(),
            // feat_writer,
            // feat_offsets: Vec::new(),
            // feat_nodes: Vec::new(),
        })
    }

    /// Add a new column.
    pub fn add_column<F>(&mut self, name: &str, col_type: ColumnType, cfgfn: F)
    where
        F: FnOnce(&mut FlatBufferBuilder<'a>, &mut ColumnArgs),
    {
        let mut col = ColumnArgs {
            name: Some(self.fbb.create_string(name)),
            type_: col_type,
            ..Default::default()
        };
        cfgfn(&mut self.fbb, &mut col);
        self.columns.push(Column::create(&mut self.fbb, &col));
    }

    fn write_feature(&mut self) -> Result<()> {
        let mut node = self.feat_writer.bbox.clone();
        // Offset is index of feat_offsets before sorting
        // Will be replaced with output offset after sorting
        node.offset = self.feat_offsets.len() as u64;
        self.feat_nodes.push(node);
        let feat_buf = self.feat_writer.finish_to_feature();
        let tmpoffset = self
            .feat_offsets
            .last()
            .map(|it| it.offset + it.size)
            .unwrap_or(0);
        self.feat_offsets.push(FeatureOffset {
            offset: tmpoffset,
            size: feat_buf.len(),
        });
        self.tmpout.write_all(&feat_buf)?;
        self.header_args.features_count += 1;
        Ok(())
    }

    /// Write the FlatGeobuf dataset (Hilbert sorted)
    pub fn write(mut self, mut out: impl Write) -> Result<()> {
        out.write_all(&MAGIC_BYTES)?;


        // Write header
        // self.header_args.columns = Some(self.fbb.create_vector(&self.columns));
        // self.header_args.geographical_extent =
        //     Some(
        //         self.fbb
        //             .create_vector(&[extent.min_x, extent.min_y, extent.max_x, extent.max_y]),
        //     );
        // self.header_args.geometry_type = self.feat_writer.dataset_type;
        let header = Header::create(&mut self.fbb, &self.header_args);
        self.fbb.finish_size_prefixed(header, None);
        let buf = self.fbb.finished_data();
        out.write_all(buf)?;

        // if self.header_args.index_node_size > 0 && !self.feat_nodes.is_empty() {
        //     // Create sorted index
        //     hilbert_sort(&mut self.feat_nodes, &extent);
        //     // Update offsets for index
        //     let mut offset = 0;
        //     let index_nodes = self
        //         .feat_nodes
        //         .iter()
        //         .map(|tmpnode| {
        //             let feat = &self.feat_offsets[tmpnode.offset as usize];
        //             let mut node = tmpnode.clone();
        //             node.offset = offset;
        //             offset += feat.size as u64;
        //             node
        //         })
        //         .collect();
        //     let tree = PackedRTree::build(&index_nodes, &extent, self.header_args.index_node_size)?;
        //     tree.stream_write(&mut out)?;
        // }

        // Copy features from temp file in sort order
        // self.tmpout.rewind()?;
        // let unsorted_feature_output = self.tmpout.into_inner().map_err(|e| e.into_error())?;
        // let mut unsorted_feature_reader = BufReader::new(unsorted_feature_output);

        // Clippy generates a false-positive here, needs a block to disable, see
        // https://github.com/rust-lang/rust-clippy/issues/9274
        #[allow(clippy::read_zero_byte_vec)]
        {
            let mut buf = Vec::with_capacity(2048);
            // for node in &self.feat_nodes {
            //     let feat = &self.feat_offsets[node.offset as usize];
            //     unsorted_feature_reader.seek(SeekFrom::Start(feat.offset as u64))?;
            //     buf.resize(feat.size, 0);
            //     unsorted_feature_reader.read_exact(&mut buf)?;
            //     out.write_all(&buf)?;
            // }
        }

        Ok(())
    }
}
