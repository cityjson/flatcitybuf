use crate::error::{CityJSONError, Result};
use crate::feature_writer::FeatureWriter;
use crate::header_generated::{
    Column, GeographicalExtent, Header, HeaderArgs, ReferenceSystem,
    ReferenceSystemArgs, Transform, Vector,
};
use crate::MAGIC_BYTES;
use cjseq::{CityJSON, CityJSONFeature, Metadata as CJMetadata, Transform as CjTransform};
use flatbuffers::FlatBufferBuilder;
use std::fs::File;
use std::io::{BufWriter, Read, Seek, Write};

// Note: Many parts of this code are derived from https://github.com/flatgeobuf/flatgeobuf/tree/master/src/rust

pub struct FcbWriter<'a> {
    tmpout: BufWriter<File>,
    fbb: FlatBufferBuilder<'a>,
    columns: Vec<flatbuffers::WIPOffset<Column<'a>>>,
    cj: CityJSON,
    options: FcbWriterOptions,
    feat_writer: FeatureWriter<'a>,
    // feat_offsets: Vec<FeatureOffset>,
    // feat_nodes: Vec<NodeItem>,
}

/// Options for FlatCityBuf writer
#[derive(Debug)]
pub struct FcbWriterOptions {
    /// Write index
    pub write_index: bool,
}

impl Default for FcbWriterOptions {
    fn default() -> Self {
        FcbWriterOptions { write_index: true }
    }
}

// Offsets in temporary file
// struct FeatureOffset {
//     offset: usize,
//     size: usize,
// }

impl<'a> FcbWriter<'a> {
    pub fn create(cj: CityJSON, city_features: &'a [&CityJSONFeature]) -> Result<Self> {
        let options = FcbWriterOptions { write_index: false };
        Self::create_with_options(options, cj, city_features)
    }

    pub fn create_with_options(
        options: FcbWriterOptions,
        cj: CityJSON,
        city_features: &'a [&CityJSONFeature],
    ) -> Result<Self> {
        let fbb = FlatBufferBuilder::new();

        // let index_node_size = if options.write_index {
        //     PackedRTree::DEFAULT_NODE_SIZE
        // } else {
        //     0
        // };
        // let index_node_size = 0; // TODO: implement index later

        // let feat_writer = FeatureWriter::with_dims(
        //     header_args.geometry_type,
        //     options.detect_type,
        //     options.promote_to_multi,
        //     // dims,
        // );

        let tmpout = BufWriter::new(tempfile::tempfile()?);
        let feat_writer: FeatureWriter<'a> = FeatureWriter::new(city_features);

        Ok(FcbWriter {
            tmpout,
            fbb,
            columns: Vec::new(),
            cj,
            options,
            feat_writer,
            // feat_writer,
            // feat_offsets: Vec::new(),
            // feat_nodes: Vec::new(),
        })
    }

    /// Add a new column.
    // pub fn add_column<F>(&mut self, name: &str, col_type: ColumnType, cfgfn: F)
    // where
    //     F: FnOnce(&mut FlatBufferBuilder<'a>, &mut ColumnArgs),
    // {
    //     let mut col = ColumnArgs {
    //         name: Some(self.fbb.create_string(name)),
    //         type_: col_type,
    //         ..Default::default()
    //     };
    //     cfgfn(&mut self.fbb, &mut col);
    //     self.columns.push(Column::create(&mut self.fbb, &col));
    // }

    fn geographical_extent(geographical_extent: &[f64; 6]) -> GeographicalExtent {
        let min = Vector::new(
            geographical_extent[0],
            geographical_extent[1],
            geographical_extent[2],
        );
        let max = Vector::new(
            geographical_extent[3],
            geographical_extent[4],
            geographical_extent[5],
        );
        GeographicalExtent::new(&min, &max)
    }

    fn transform(transform: &CjTransform) -> Transform {
        let scale = Vector::new(transform.scale[0], transform.scale[1], transform.scale[2]);
        let translate = Vector::new(
            transform.translate[0],
            transform.translate[1],
            transform.translate[2],
        );
        Transform::new(&scale, &translate)
    }

    fn reference_system(
        fbb: &mut FlatBufferBuilder<'a>,
        metadata: &CJMetadata,
    ) -> Option<flatbuffers::WIPOffset<ReferenceSystem<'a>>> {
        metadata.reference_system.as_ref().map(|ref_sys| {
            let authority = Some(fbb.create_string(&ref_sys.authority));

            let version = ref_sys.version.parse::<i32>().unwrap_or_else(|e| {
                println!("Failed to parse version: {}", e);
                0
            });
            let code = ref_sys.code.parse::<i32>().unwrap_or_else(|e| {
                println!("Failed to parse code: {}", e);
                0
            });

            let code_string = None; // TODO: implement code_string

            ReferenceSystem::create(
                fbb,
                &ReferenceSystemArgs {
                    authority,
                    version,
                    code,
                    code_string,
                },
            )
        })
    }

    pub fn write_features(&mut self) -> Result<()> {
        let feat_buf = self.feat_writer.finish_to_feature();
        self.tmpout.write_all(&feat_buf)?;
        Ok(())
    }

    /// Write the FlatGeobuf dataset (Hilbert sorted)
    pub fn write(mut self, mut out: impl Write) -> Result<()> {
        out.write_all(&MAGIC_BYTES)?;

        let metadata = self
            .cj
            .metadata
            .as_ref()
            .ok_or(CityJSONError::MissingField("metadata"))
            .unwrap();
        let reference_system = Self::reference_system(&mut self.fbb, metadata);
        let transform = Self::transform(&self.cj.transform);
        let geographical_extent = metadata
            .geographical_extent
            .as_ref()
            .map(Self::geographical_extent);
        let features_count = self.feat_writer.city_features.len();
        let header_args = HeaderArgs {
            version: Some(self.fbb.create_string(&self.cj.version)),
            transform: Some(&transform),
            columns: None,
            features_count: features_count as u64,
            geographical_extent: geographical_extent.as_ref(),
            reference_system,
            identifier: metadata
                .identifier
                .as_ref()
                .map(|i| self.fbb.create_string(i)),
            reference_date: metadata
                .reference_date
                .as_ref()
                .map(|r| self.fbb.create_string(r)),
            title: metadata.title.as_ref().map(|t| self.fbb.create_string(t)),
            poc_contact_name: metadata
                .point_of_contact
                .as_ref()
                .map(|poc| self.fbb.create_string(&poc.contact_name)),
            poc_contact_type: metadata.point_of_contact.as_ref().and_then(|poc| {
                poc.contact_type
                    .as_ref()
                    .map(|ct| self.fbb.create_string(ct))
            }),
            poc_role: metadata
                .point_of_contact
                .as_ref()
                .and_then(|poc| poc.role.as_ref().map(|r| self.fbb.create_string(r))),
            poc_phone: metadata
                .point_of_contact
                .as_ref()
                .and_then(|poc| poc.phone.as_ref().map(|p| self.fbb.create_string(p))),
            poc_email: metadata
                .point_of_contact
                .as_ref()
                .map(|poc| self.fbb.create_string(&poc.email_address)),
            poc_website: metadata
                .point_of_contact
                .as_ref()
                .and_then(|poc| poc.website.as_ref().map(|w| self.fbb.create_string(w))),
            poc_address_thoroughfare_number: metadata.point_of_contact.as_ref().and_then(|poc| {
                poc.address
                    .as_ref()
                    .map(|a| self.fbb.create_string(&a.thoroughfare_number.to_string()))
            }),
            poc_address_thoroughfare_name: metadata.point_of_contact.as_ref().map(|poc| {
                self.fbb.create_string(
                    &poc.address
                        .as_ref()
                        .map(|a| a.thoroughfare_name.clone())
                        .unwrap_or_default(),
                )
            }),
            poc_address_locality: metadata.point_of_contact.as_ref().map(|poc| {
                self.fbb.create_string(
                    &poc.address
                        .as_ref()
                        .map(|a| a.locality.clone())
                        .unwrap_or_default(),
                )
            }),
            poc_address_postcode: metadata.point_of_contact.as_ref().map(|poc| {
                self.fbb.create_string(
                    &poc.address
                        .as_ref()
                        .map(|a| a.postal_code.clone())
                        .unwrap_or_default(),
                )
            }),
            poc_address_country: metadata.point_of_contact.as_ref().map(|poc| {
                self.fbb.create_string(
                    &poc.address
                        .as_ref()
                        .map(|a| a.country.clone())
                        .unwrap_or_default(),
                )
            }),
            attributes: None,
        };

        let header = Header::create(&mut self.fbb, &header_args);
        self.fbb.finish_size_prefixed(header, None);
        let buf = self.fbb.finished_data();
        out.write_all(buf)?;

        self.tmpout.rewind()?;
        let mut unsorted_feature_output = self.tmpout.into_inner().map_err(|e| e.into_error())?;
        let mut buf = Vec::new();
        unsorted_feature_output.read_to_end(&mut buf)?;
        out.write_all(&buf)?;

        Ok(())
    }
}
