use cjseq::CityJSONFeature;

use crate::serializer::*;

use super::attribute::{cityfeature_to_index_entries, AttributeIndexEntry, AttributeSchema};

use crate::error::Result;
use crate::packed_rtree::NodeItem;

/// A writer that converts CityJSON features to FlatBuffers format
///
/// This struct handles the serialization of CityJSON features into a binary
/// FlatBuffers representation, which is more efficient for storage and transmission.
pub struct FeatureWriter {
    /// The FlatBuffers builder instance used for serialization
    fbb: flatbuffers::FlatBufferBuilder<'static>,
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

impl FeatureWriter {
    /// Creates a new `FeatureWriter` instance, which serialises features
    /// one at a time with [`Self::finish_to_feature`].
    pub fn new(
        attr_schema: AttributeSchema,
        semantic_attr_schema: Option<AttributeSchema>,
        attr_indices: Option<Vec<String>>,
    ) -> FeatureWriter {
        FeatureWriter {
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

    /// Serializes `feature` to a FlatBuffers binary format
    ///
    /// This method converts the CityJSON feature into a FlatBuffers representation,
    /// including all city objects and vertices. The resulting buffer is size-prefixed.
    ///
    /// # Returns
    ///
    /// A vector of bytes containing the serialized feature, or an error for a
    /// cjseq value with no FlatCityBuf encoding.
    pub fn finish_to_feature(&mut self, feature: &CityJSONFeature) -> Result<Vec<u8>> {
        self.reset_bbox();
        self.reset_attribute_feature_offsets();
        self.extract_indexable_attributes(feature);
        let (cf_buf, bbox) = to_fcb_city_feature(
            &mut self.fbb,
            feature.id.as_str(),
            feature,
            &self.attr_schema,
            self.semantic_attr_schema.as_ref(),
        )
        .inspect_err(|_| self.fbb.reset())?;
        self.bbox = bbox;
        self.fbb.finish_size_prefixed(cf_buf, None);
        let buf = self.fbb.finished_data().to_vec();
        self.fbb.reset();
        Ok(buf)
    }

    fn extract_indexable_attributes(&mut self, feature: &CityJSONFeature) {
        if let Some(attr_indices) = &self.attr_indices {
            let index_entries =
                cityfeature_to_index_entries(feature, &self.attr_schema, attr_indices);
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
