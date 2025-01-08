use cjseq::CityJSONFeature;

use crate::fcb_serde::fcb_serializer::*;

/// A writer that converts CityJSON features to FlatBuffers format
///
/// This struct handles the serialization of CityJSON features into a binary
/// FlatBuffers representation, which is more efficient for storage and transmission.
pub struct FeatureWriter<'a> {
    /// The CityJSON feature to be serialized
    city_feature: &'a CityJSONFeature,
    /// The FlatBuffers builder instance used for serialization
    fbb: flatbuffers::FlatBufferBuilder<'a>,
}

impl<'a> FeatureWriter<'a> {
    /// Creates a new `FeatureWriter` instance
    ///
    /// # Arguments
    ///
    /// * `city_feature` - A reference to the CityJSON feature to be serialized
    pub fn new(city_feature: &'a CityJSONFeature) -> FeatureWriter<'a> {
        FeatureWriter {
            city_feature,
            fbb: flatbuffers::FlatBufferBuilder::new(),
        }
    }

    /// Serializes the current feature to a FlatBuffers binary format
    ///
    /// This method converts the CityJSON feature into a FlatBuffers representation,
    /// including all city objects and vertices. The resulting buffer is size-prefixed.
    ///
    /// # Returns
    ///
    /// A vector of bytes containing the serialized feature
    pub fn finish_to_feature(&mut self) -> Vec<u8> {
        let city_objects_buf: Vec<_> = self
            .city_feature
            .city_objects
            .iter()
            .map(|(id, co)| to_fcb_city_object(&mut self.fbb, id, co))
            .collect();
        let cf_buf = to_fcb_city_feature(
            &mut self.fbb,
            self.city_feature.id.as_str(),
            &city_objects_buf,
            &self.city_feature.vertices,
        );
        self.fbb.finish_size_prefixed(cf_buf, None);
        let buf = self.fbb.finished_data().to_vec();
        self.fbb.reset();
        buf
    }

    /// Updates the writer with a new feature to be serialized
    ///
    /// # Arguments
    ///
    /// * `feature` - A reference to the new CityJSON feature
    pub fn add_feature(&mut self, feature: &'a CityJSONFeature) {
        self.city_feature = feature;
    }
}
