use cjseq::CityJSONFeature;

mod city_feature_writer;
mod geometry_encoderdecoder;

/// FCB Feature writer.
pub struct FeatureWriter<'a> {
    pub city_features: &'a [&'a CityJSONFeature],
    fbb: flatbuffers::FlatBufferBuilder<'a>,
    // pub(crate) bbox: NodeItem,
}

// #[derive(PartialEq, Debug)]
// enum GeomState {
//     Normal,
//     GeometryCollection,
//     ForceMulti,
// }

macro_rules! to_fb_vector {
    ( $self:ident, $items:ident ) => {
        if cfg!(target_endian = "big") {
            let mut iter = std::mem::take(&mut $self.$items).into_iter();
            $self.fbb.create_vector_from_iter(&mut iter)
        } else {
            let items = $self.fbb.create_vector(&$self.$items);
            $self.$items.truncate(0);
            items
        }
    };
}

impl<'a> FeatureWriter<'a> {
    pub fn new(city_features: &'a [&'a CityJSONFeature]) -> FeatureWriter<'a> {
        FeatureWriter {
            city_features,
            fbb: flatbuffers::FlatBufferBuilder::new(),
        }
    }

    // fn reset_bbox(&mut self) {
    //     if self.geom_state != GeomState::GeometryCollection {
    //         self.bbox = NodeItem::create(0);
    //     }
    // }
    pub fn finish_to_feature(&mut self) -> Vec<u8> {
        let mut fb_features = Vec::new();
        for cf in self.city_features {
            let city_objects_buf: Vec<_> = cf
                .city_objects
                .iter()
                .map(|(id, co)| self.create_city_object(id, co))
                .collect();
            let cf_buf = self.create_city_feature(cf.id.as_str(), &city_objects_buf, &cf.vertices);
            fb_features.push(cf_buf);
        }
        let f = self.fbb.create_vector(&fb_features);
        self.fbb.finish(f, None);
        self.fbb.finished_data().to_vec()
    }
}
