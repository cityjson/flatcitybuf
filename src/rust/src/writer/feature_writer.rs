use cjseq::CityJSONFeature;

use crate::fcb_serde::fcb_serializer::*;

pub struct FeatureWriter<'a> {
    city_feature: &'a CityJSONFeature,
    fbb: flatbuffers::FlatBufferBuilder<'a>,
}

impl<'a> FeatureWriter<'a> {
    pub fn new(city_feature: &'a CityJSONFeature) -> FeatureWriter<'a> {
        FeatureWriter {
            city_feature,
            fbb: flatbuffers::FlatBufferBuilder::new(),
        }
    }

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

    pub fn add_feature(&mut self, feature: &'a CityJSONFeature) {
        self.city_feature = feature;
    }
}
