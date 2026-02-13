use crate::error::to_napi_error;

use fcb_core::http_reader::AsyncFeatureIter;
use napi::bindgen_prelude::*;
use napi_derive::napi;
use tokio::sync::Mutex;

/// Async iterator over FCB features.
///
/// Each call to `next()` fetches the next feature from the remote file
/// and returns it as a CityJSON feature object.
#[napi]
pub struct FeatureIter {
    inner: Mutex<AsyncFeatureIter<reqwest::Client>>,
    count: Option<usize>,
}

impl FeatureIter {
    pub fn new(inner: AsyncFeatureIter<reqwest::Client>) -> Self {
        let count = inner.features_count();
        Self {
            inner: Mutex::new(inner),
            count,
        }
    }
}

#[napi]
impl FeatureIter {
    /// Get the number of selected features, if known.
    #[napi]
    pub fn features_count(&self) -> Option<u32> {
        self.count.map(|c| c as u32)
    }

    /// Read the next feature. Returns null when iteration is complete.
    #[napi]
    pub async fn next(&self) -> Result<Option<serde_json::Value>> {
        let mut iter = self.inner.lock().await;
        let Some(_buffer) = iter.next().await.map_err(to_napi_error)? else {
            return Ok(None);
        };

        let cj_feature = iter.cur_cj_feature().map_err(to_napi_error)?;
        let value =
            serde_json::to_value(&cj_feature).map_err(|e| Error::from_reason(e.to_string()))?;
        Ok(Some(value))
    }

    /// Collect all remaining features into an array.
    #[napi]
    pub async fn collect(&self) -> Result<Vec<serde_json::Value>> {
        let mut iter = self.inner.lock().await;
        let mut features = Vec::new();
        loop {
            let Some(_buffer) = iter.next().await.map_err(to_napi_error)? else {
                break;
            };
            let cj_feature = iter.cur_cj_feature().map_err(to_napi_error)?;
            let value = serde_json::to_value(&cj_feature)
                .map_err(|e| Error::from_reason(e.to_string()))?;
            features.push(value);
        }
        Ok(features)
    }
}
