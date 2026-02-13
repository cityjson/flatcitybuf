use crate::error::to_napi_error;
use crate::iter::FeatureIter;
use crate::query::{NodeAttrQuery, NodeSpatialQuery};

use fcb_core::deserializer::to_cj_metadata;
use fcb_core::http_reader::HttpFcbReader;
use napi::bindgen_prelude::*;
use napi_derive::napi;

/// FlatCityBuf HTTP reader for remote FCB files.
///
/// Opens a remote FCB file via HTTP range requests and provides
/// methods to query features by spatial bounds or attribute filters.
///
/// Note: Each query method re-opens the HTTP connection because the
/// underlying reader is consumed during selection. This matches the
/// Python async binding pattern.
#[napi]
pub struct FcbReader {
    url: String,
    /// Cached CityJSON metadata from the initial open
    cityjson_cache: serde_json::Value,
    /// Cached feature count from header
    feature_count: u64,
}

#[napi]
impl FcbReader {
    /// Open a remote FCB file by URL.
    ///
    /// Fetches the header and spatial index metadata via HTTP range requests.
    #[napi(factory)]
    pub async fn open(url: String) -> Result<FcbReader> {
        let reader = HttpFcbReader::open(&url).await.map_err(to_napi_error)?;
        let header = reader.header();
        let cj = to_cj_metadata(&header).map_err(to_napi_error)?;
        let cityjson_cache =
            serde_json::to_value(&cj).map_err(|e| Error::from_reason(e.to_string()))?;
        let feature_count = header.features_count();

        Ok(FcbReader {
            url,
            cityjson_cache,
            feature_count,
        })
    }

    /// Get CityJSON metadata (transform, CRS, metadata object).
    ///
    /// Returns the CityJSON-compatible metadata extracted from the FCB header.
    #[napi]
    pub fn cityjson(&self) -> serde_json::Value {
        self.cityjson_cache.clone()
    }

    /// Get the feature count from the header.
    #[napi(getter)]
    pub fn features_count(&self) -> u32 {
        self.feature_count as u32
    }

    /// Select all features. Returns an async iterator.
    #[napi]
    pub async fn select_all(&self) -> Result<FeatureIter> {
        let reader = HttpFcbReader::open(&self.url)
            .await
            .map_err(to_napi_error)?;
        let iter = reader.select_all().await.map_err(to_napi_error)?;
        Ok(FeatureIter::new(iter))
    }

    /// Select features by spatial query (bbox, point intersects, or point nearest).
    #[napi]
    pub async fn select_spatial(&self, query: &NodeSpatialQuery) -> Result<FeatureIter> {
        let inner_query = query.to_core_query()?;
        let reader = HttpFcbReader::open(&self.url)
            .await
            .map_err(to_napi_error)?;
        let iter = reader
            .select_query(inner_query)
            .await
            .map_err(to_napi_error)?;
        Ok(FeatureIter::new(iter))
    }

    /// Select features by spatial query with pagination.
    #[napi]
    pub async fn select_spatial_paged(
        &self,
        query: &NodeSpatialQuery,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<FeatureIter> {
        let inner_query = query.to_core_query()?;
        let reader = HttpFcbReader::open(&self.url)
            .await
            .map_err(to_napi_error)?;
        let iter = reader
            .select_query_paged(
                inner_query,
                limit.map(|l| l as usize),
                offset.map(|o| o as usize),
            )
            .await
            .map_err(to_napi_error)?;
        Ok(FeatureIter::new(iter))
    }

    /// Select features by attribute query.
    #[napi]
    pub async fn select_attr_query(&self, query: &NodeAttrQuery) -> Result<FeatureIter> {
        let core_query = query.to_core_query()?;
        let reader = HttpFcbReader::open(&self.url)
            .await
            .map_err(to_napi_error)?;
        let iter = reader
            .select_attr_query(&core_query)
            .await
            .map_err(to_napi_error)?;
        Ok(FeatureIter::new(iter))
    }

    /// Select features by attribute query with pagination.
    #[napi]
    pub async fn select_attr_query_paged(
        &self,
        query: &NodeAttrQuery,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<FeatureIter> {
        let core_query = query.to_core_query()?;
        let reader = HttpFcbReader::open(&self.url)
            .await
            .map_err(to_napi_error)?;
        let iter = reader
            .select_attr_query_paged(
                &core_query,
                limit.map(|l| l as usize),
                offset.map(|o| o as usize),
            )
            .await
            .map_err(to_napi_error)?;
        Ok(FeatureIter::new(iter))
    }
}
