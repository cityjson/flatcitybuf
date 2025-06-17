use cjseq::CityJSONFeature;

use crate::serializer::*;

use super::attribute::{cityfeature_to_index_entries, AttributeIndexEntry, AttributeSchema};

use crate::packed_rtree::NodeItem;

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

    semantic_attr_schema: Option<AttributeSchema>,
    pub(super) bbox: NodeItem,

    attr_indices: Option<Vec<String>>,

    pub(super) attribute_feature_offsets: AttributeFeatureOffset,
}

#[derive(Clone, PartialEq, Debug)]
pub(super) struct AttributeFeatureOffset {
    pub(super) offset: usize,
    pub(super) size: usize,
    pub(super) index_entries: Vec<AttributeIndexEntry>,
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
        semantic_attr_schema: Option<AttributeSchema>,
        attr_indices: Option<Vec<String>>,
    ) -> FeatureWriter<'a> {
        FeatureWriter {
            city_feature,
            fbb: flatbuffers::FlatBufferBuilder::new(),
            attr_schema,
            semantic_attr_schema,
            bbox: NodeItem::create(0),
            attr_indices,
            attribute_feature_offsets: AttributeFeatureOffset {
                offset: 0,
                size: 0,
                index_entries: Vec::new(),
            },
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
        self.reset_bbox();
        self.reset_attribute_feature_offsets();
        self.extract_indexable_attributes();
        let (cf_buf, bbox) = to_fcb_city_feature(
            &mut self.fbb,
            self.city_feature.id.as_str(),
            self.city_feature,
            &self.attr_schema,
            self.semantic_attr_schema.as_ref(),
        );
        self.bbox = bbox;
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

    fn extract_indexable_attributes(&mut self) {
        if let Some(attr_indices) = &self.attr_indices {
            let index_entries =
                cityfeature_to_index_entries(self.city_feature, &self.attr_schema, attr_indices);
            self.attribute_feature_offsets.index_entries = index_entries;
        }
    }

    fn reset_bbox(&mut self) {
        self.bbox = NodeItem::create(0);
    }

    fn reset_attribute_feature_offsets(&mut self) {
        self.attribute_feature_offsets.index_entries.clear();
        self.attribute_feature_offsets.offset = 0;
        self.attribute_feature_offsets.size = 0;
    }
}
