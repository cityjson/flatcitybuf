#![cfg(target_arch = "wasm32")]

use async_trait::async_trait;
use bytes::Bytes;
use gloo_net::http::RequestBuilder as GlooRequest;
use http_range_client::{AsyncBufferedHttpRangeClient, AsyncHttpRangeClient, HttpError, Result};

pub struct WasmHttpClient {}

impl WasmHttpClient {
    pub fn new(url: &str) -> AsyncBufferedHttpRangeClient<WasmHttpClient> {
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
            // let v = val
            //     .to_str()
            //     .map_err(|e| HttpError::HttpError(e.to_string()))?;
            // Ok(Some(v.to_string()))
            Ok(Some(val.to_string()))
        } else {
            Ok(None)
        }
    }
}
