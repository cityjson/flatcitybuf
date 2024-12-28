use crate::error::Result;
use crate::MAGIC_BYTES;
use cjseq::{CityJSON, CityJSONFeature};
use feature_writer::FeatureWriter;
use header_writer::HeaderWriter;
use std::fs::File;
use std::io::{BufWriter, Read, Seek, Write};

pub mod feature_writer;
pub mod geometry_encoderdecoder;
pub mod header_writer;

pub struct FcbWriter<'a> {
    tmpout: BufWriter<File>,
    header_writer: HeaderWriter<'a>,
    feat_writer: FeatureWriter<'a>,
    // feat_offsets: Vec<FeatureOffset>,
    // feat_nodes: Vec<NodeItem>,
}

// Offsets in temporary file
// struct FeatureOffset {
//     offset: usize,
//     size: usize,
// }

impl<'a> FcbWriter<'a> {
    pub fn new(cj: CityJSON, features: &'a [&'a CityJSONFeature]) -> Result<Self> {
        let header_writer = HeaderWriter::new(cj, None);
        let feat_writer = FeatureWriter::new(features);
        Ok(Self {
            header_writer,
            feat_writer,
            tmpout: BufWriter::new(tempfile::tempfile()?),
        })
    }
    pub fn write_features(&mut self) -> Result<()> {
        let feat_buf = self.feat_writer.finish_to_feature();
        println!("feat_buf size: {} bytes", feat_buf.len());
        self.tmpout.write_all(&feat_buf)?;
        Ok(())
    }

    /// Write the FlatGeobuf dataset (Hilbert sorted)
    pub fn write(mut self, mut out: impl Write) -> Result<()> {
        out.write_all(&MAGIC_BYTES)?;

        // Write features. This should be done first, so that the header can be written with the correct number of features.
        self.write_features()?;

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
