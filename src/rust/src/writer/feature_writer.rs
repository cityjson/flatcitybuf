
use cjseq::CityJSONFeature;

use crate::fcb_serde::fcb_serializer::*;

pub struct FeatureWriter<'a> {
    city_features: &'a [&'a CityJSONFeature],
    fbb: flatbuffers::FlatBufferBuilder<'a>,
}

impl<'a> FeatureWriter<'a> {
    pub fn new(city_features: &'a [&'a CityJSONFeature]) -> FeatureWriter<'a> {
        FeatureWriter {
            city_features,
            fbb: flatbuffers::FlatBufferBuilder::new(),
        }
    }

    pub fn finish_to_feature(&mut self) -> Vec<u8> {
        let mut fb_features = Vec::new();
        for cf in self.city_features {
            let city_objects_buf: Vec<_> = cf
                .city_objects
                .iter()
                .map(|(id, co)| to_fcb_city_object(&mut self.fbb, id, co))
                .collect();
            let cf_buf = to_fcb_city_feature(
                &mut self.fbb,
                cf.id.as_str(),
                &city_objects_buf,
                &cf.vertices,
            );
            fb_features.push(cf_buf);
        }
        let f = self.fbb.create_vector(&fb_features);
        self.fbb.finish(f, None);
        self.fbb.finished_data().to_vec()
    }
}
