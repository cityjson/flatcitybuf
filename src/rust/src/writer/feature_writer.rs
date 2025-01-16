use cjseq::CityJSONFeature;

use crate::serializer::*;

use super::attribute::AttributeSchema;

use crate::packedrtree::NodeItem;

/// A writer that converts CityJSON features to FlatBuffers format
///
/// This struct handles the serialization of CityJSON features into a binary
/// FlatBuffers representation, which is more efficient for storage and transmission.
pub struct FeatureWriter<'a> {
    /// The CityJSON feature to be serialized
    city_feature: &'a CityJSONFeature,
    /// The FlatBuffers builder instance used for serialization
    fbb: flatbuffers::FlatBufferBuilder<'a>,
    /// The attribute schema to be used for serialization
    attr_schema: AttributeSchema,
    pub(crate) bbox: NodeItem,
}

impl<'a> FeatureWriter<'a> {
    /// Creates a new `FeatureWriter` instance
    ///
    /// # Arguments
    ///
    /// * `city_feature` - A reference to the CityJSON feature to be serialized
    pub fn new(
        city_feature: &'a CityJSONFeature,
        attr_schema: AttributeSchema,
    ) -> FeatureWriter<'a> {
        FeatureWriter {
            city_feature,
            fbb: flatbuffers::FlatBufferBuilder::new(),
            attr_schema,
            bbox: NodeItem::create(0),
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
        let (cf_buf, bbox) = to_fcb_city_feature(
            &mut self.fbb,
            self.city_feature.id.as_str(),
            self.city_feature,
            &self.attr_schema,
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

    fn reset_bbox(&mut self) {
        self.bbox = NodeItem::create(0);
    }
}
