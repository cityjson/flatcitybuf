use crate::feature_generated::*;
use crate::header_generated::*;

pub struct FcbBuffer {
    pub(crate) header_buf: Vec<u8>,
    pub(crate) features_buf: Vec<u8>,
}

impl FcbBuffer {
    pub(crate) fn header(&self) -> Header {
        unsafe { size_prefixed_root_as_header_unchecked(&self.header_buf) }
    }

    pub(crate) fn features(&self) -> CityFeature {
        unsafe { size_prefixed_root_as_city_feature_unchecked(&self.features_buf) }
    }
}
