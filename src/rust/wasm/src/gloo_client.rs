use async_trait::async_trait;
use bytes::Bytes;
use http_range_client::{AsyncBufferedHttpRangeClient, AsyncHttpRangeClient, HttpError, Result};

#[cfg(target_arch = "wasm32")]
use gloo_net::http::RequestBuilder as GlooRequest;

pub struct WasmHttpClient {}

#[cfg(target_arch = "wasm32")]
impl WasmHttpClient {
    pub fn new(url: &str) -> AsyncBufferedHttpRangeClient<WasmHttpClient> {
        AsyncBufferedHttpRangeClient::with(WasmHttpClient {}, url)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl WasmHttpClient {
    pub fn new(url: &str) -> AsyncBufferedHttpRangeClient<WasmHttpClient> {
        // This is a mock implementation for non-wasm targets
        // It will never be called in production, but enables compilation
        AsyncBufferedHttpRangeClient::with(WasmHttpClient {}, url)
    }
}

#[cfg(target_arch = "wasm32")]
#[async_trait(?Send)]
impl AsyncHttpRangeClient for WasmHttpClient {
    async fn get_range(&self, url: &str, range: &str) -> Result<Bytes> {
        let response = GlooRequest::new(url)
            .header("Range", range)
            .send()
            .await
            .map_err(|e| HttpError::HttpError(e.to_string()))?;

        if !response.ok() {
            return Err(HttpError::HttpStatus(response.status()));
        }
        response
            .binary()
            .await
            .map(Bytes::from)
            .map_err(|e| HttpError::HttpError(e.to_string()))
    }

    async fn head_response_header(&self, url: &str, header: &str) -> Result<Option<String>> {
        let response = GlooRequest::new(url)
            .send()
            .await
            .map_err(|e| HttpError::HttpError(format!("failed to send request: {}", e)))?;
        if let Some(val) = response.headers().get(header) {
            Ok(Some(val.to_string()))
        } else {
            Ok(None)
        }
    }
}

// Mock implementation for non-wasm targets
#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
impl AsyncHttpRangeClient for WasmHttpClient {
    async fn get_range(&self, _url: &str, _range: &str) -> Result<Bytes> {
        // Mock implementation that will never be called
        Err(HttpError::HttpError(
            "Not implemented for non-wasm targets".to_string(),
        ))
    }

    async fn head_response_header(&self, _url: &str, _header: &str) -> Result<Option<String>> {
        // Mock implementation that will never be called
        Err(HttpError::HttpError(
            "Not implemented for non-wasm targets".to_string(),
        ))
    }
}
