use crate::MAGIC_BYTES;
use anyhow::Result;
use attribute::AttributeSchema;
use cjseq::{CityJSON, CityJSONFeature};
use feature_writer::FeatureWriter;
use header_writer::{HeaderWriter, HeaderWriterOptions};
use std::fs::File;
use std::io::{BufWriter, Read, Seek, Write};

pub mod attribute;
pub mod feature_writer;
pub mod geometry_encoderdecoder;
pub mod header_writer;

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

    attr_schema: AttributeSchema,
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
        first_feature: Option<&'a CityJSONFeature>,
        attr_schema: Option<&AttributeSchema>,
    ) -> Result<Self> {
        let owned_schema = AttributeSchema::new();
        let attr_schema = attr_schema.unwrap_or(&owned_schema);

        let header_writer = HeaderWriter::new(cj, header_option, attr_schema.clone()); // if attr_schema is None, instantiate an empty one
        let feat_writer = first_feature.map(|feat| FeatureWriter::new(feat, attr_schema.clone()));
        Ok(Self {
            header_writer,
            feat_writer,
            tmpout: BufWriter::new(tempfile::tempfile()?),
            attr_schema: attr_schema.clone(),
        })
    }

    /// Writes the current feature to the temporary buffer
    ///
    /// # Returns
    ///
    /// A Result indicating success or failure of the write operation
    pub fn write_feature(&mut self) -> Result<()> {
        if let Some(feat_writer) = &mut self.feat_writer {
            let feat_buf = feat_writer.finish_to_feature();
            self.tmpout.write_all(&feat_buf)?;
        }
        Ok(())
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

        let header_buf = self.header_writer.finish_to_header();
        println!("header buf size: {} bytes", header_buf.len());
        out.write_all(&header_buf)?;

        self.tmpout.rewind()?;
        let mut unsorted_feature_output = self.tmpout.into_inner().map_err(|e| e.into_error())?;
        let mut feature_buf: Vec<u8> = Vec::new();
        unsorted_feature_output.read_to_end(&mut feature_buf)?;
        println!(
            "unsorted_feature_output buf size: {} bytes",
            feature_buf.len()
        );
        out.write_all(&feature_buf)?;

        Ok(())
    }
}
