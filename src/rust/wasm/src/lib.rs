#![cfg(target_arch = "wasm32")]
use console_log::init_with_level;
use fcb_core::deserializer::{to_cj_feature, to_cj_metadata};
use fcb_core::{size_prefixed_root_as_header, Header};
// #[cfg(target_arch = "wasm32")]
use gloo_client::WasmHttpClient;
#[cfg(target_arch = "wasm32")]
use log::{debug, info, trace};
use serde_wasm_bindgen::to_value;
use wasm_bindgen::prelude::*;

use byteorder::{ByteOrder, LittleEndian};
use bytes::{BufMut, Bytes, BytesMut};
use fcb_core::city_buffer::FcbBuffer;
use fcb_core::{
    check_magic_bytes, size_prefixed_root_as_city_feature, HEADER_MAX_BUFFER_SIZE,
    HEADER_SIZE_SIZE, MAGIC_BYTES_SIZE,
};

use std::fmt::Error;
use std::result::Result;

use http_range_client::{AsyncBufferedHttpRangeClient, AsyncHttpRangeClient};

use packed_rtree::{http::HttpRange, http::HttpSearchResultItem, NodeItem, PackedRTree};
use std::collections::VecDeque;
use std::ops::Range;

mod gloo_client;

// The largest request we'll speculatively make.
// If a single huge feature requires, we'll necessarily exceed this limit.
const DEFAULT_HTTP_FETCH_SIZE: usize = 1_048_576; // 1MB

/// FlatCityBuf dataset HTTP reader
#[wasm_bindgen]
pub struct HttpFcbReader {
    client: AsyncBufferedHttpRangeClient<WasmHttpClient>,
    // feature reading requires header access, therefore
    // header_buf is included in the FcbBuffer struct.
    fbs: FcbBuffer,
}

#[wasm_bindgen]
pub struct AsyncFeatureIter {
    client: AsyncBufferedHttpRangeClient<WasmHttpClient>,
    // feature reading requires header access, therefore
    // header_buf is included in the FcbBuffer struct.
    fbs: FcbBuffer,
    /// Which features to iterate
    selection: FeatureSelection,
    /// Number of selected features
    count: usize,
}

#[wasm_bindgen]
impl HttpFcbReader {
    #[wasm_bindgen(constructor)]
    pub async fn new(url: String) -> Result<HttpFcbReader, JsValue> {
        println!("open===: {:?}", url);
        console_error_panic_hook::set_once();
        init_with_level(log::Level::Debug).expect("Could not initialize logger");

        trace!("starting: opening http reader, reading header");
        let client = WasmHttpClient::new(&url);
        Self::_open(client).await
    }

    async fn _open(
        mut client: AsyncBufferedHttpRangeClient<WasmHttpClient>,
    ) -> Result<HttpFcbReader, JsValue> {
        // Because we use a buffered HTTP reader, anything extra we fetch here can
        // be utilized to skip subsequent fetches.
        // Immediately following the header is the optional spatial index, we deliberately fetch
        // a small part of that to skip subsequent requests
        let prefetch_index_bytes: usize = {
            // The actual branching factor will be in the header, but since we don't have the header
            // yet we guess. The consequence of getting this wrong isn't catastrophic, it just means
            // we may be fetching slightly more than we need or that we make an extra request later.
            let assumed_branching_factor = PackedRTree::DEFAULT_NODE_SIZE as usize;

            // NOTE: each layer is exponentially larger
            let prefetched_layers: u32 = 3;

            (0..prefetched_layers)
                .map(|i| assumed_branching_factor.pow(i) * std::mem::size_of::<NodeItem>())
                .sum()
        };

        // In reality, the header is probably less than half this size, but better to overshoot and
        // fetch an extra kb rather than have to issue a second request.
        let assumed_header_size = 2024;
        let min_req_size = assumed_header_size + prefetch_index_bytes;
        client.set_min_req_size(min_req_size);
        debug!("fetching header. min_req_size: {min_req_size} (assumed_header_size: {assumed_header_size}, prefetched_index_bytes: {prefetch_index_bytes})");
        info!("fetching header. min_req_size: {min_req_size} (assumed_header_size: {assumed_header_size}, prefetched_index_bytes: {prefetch_index_bytes})");
        let mut read_bytes = 0;
        let bytes = client
            .get_range(read_bytes, MAGIC_BYTES_SIZE)
            .await
            .map_err(|e| JsValue::from_str(&e.to_string()))?; // to get magic bytes
        if !check_magic_bytes(bytes) {
            return Err(JsValue::from_str("MissingMagicBytes"));
        }
        debug!("checked magic bytes");

        read_bytes += MAGIC_BYTES_SIZE;
        let mut bytes = BytesMut::from(
            client
                .get_range(read_bytes, HEADER_SIZE_SIZE)
                .await
                .map_err(|e| JsValue::from_str(&e.to_string()))?,
        );
        read_bytes += HEADER_SIZE_SIZE;

        let header_size = LittleEndian::read_u32(&bytes) as usize;
        if !(8..=HEADER_MAX_BUFFER_SIZE).contains(&header_size) {
            // minimum size check avoids panic in FlatBuffers header decoding
            return Err(JsValue::from_str(&format!(
                "IllegalHeaderSize: {header_size}"
            )));
        }
        info!("header_size: {header_size}");

        bytes.put(
            client
                .get_range(read_bytes, header_size)
                .await
                .map_err(|e| JsValue::from_str(&e.to_string()))?,
        );
        read_bytes += header_size;

        let header_buf = bytes.to_vec();

        // verify flatbuffer
        let header = size_prefixed_root_as_header(&header_buf)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        info!("header:---------");
        info!("header: {:?}", to_cj_metadata(&header));
        trace!("completed: opening http reader");
        Ok(HttpFcbReader {
            client,
            fbs: FcbBuffer {
                header_buf,
                features_buf: Vec::new(),
            },
        })
    }

    #[wasm_bindgen]
    pub fn header(&self) -> Result<JsValue, JsValue> {
        let header = self.fbs.header();
        info!("header in the function: {:?}", to_cj_metadata(&header));
        let cj = to_cj_metadata(&header).map_err(|e| JsValue::from_str(&e.to_string()))?;
        let jsval = to_value(&cj).map_err(|e| JsValue::from_str(&e.to_string()))?;
        info!("jsval: {:?}", jsval);
        Ok(jsval)
    }

    fn header_len(&self) -> usize {
        MAGIC_BYTES_SIZE + self.fbs.header_buf.len()
    }
    /// Select all features.
    #[wasm_bindgen]
    pub async fn select_all(self) -> Result<AsyncFeatureIter, JsValue> {
        let header = self.fbs.header();
        let count = header.features_count();
        // TODO: support reading with unknown feature count
        let index_size = if header.index_node_size() > 0 {
            PackedRTree::index_size(count as usize, header.index_node_size())
        } else {
            0
        };
        // Skip index
        let feature_base = self.header_len() + index_size;
        Ok(AsyncFeatureIter {
            client: self.client,
            fbs: self.fbs,
            selection: FeatureSelection::SelectAll(SelectAll {
                features_left: count,
                pos: feature_base,
            }),
            count: count as usize,
        })
    }
    /// Select features within a bounding box.
    #[wasm_bindgen]
    pub async fn select_bbox(
        mut self,
        min_x: f64,
        min_y: f64,
        max_x: f64,
        max_y: f64,
    ) -> Result<AsyncFeatureIter, JsValue> {
        trace!("starting: select_bbox, traversing index");
        // Read R-Tree index and build filter for features within bbox
        let header = self.fbs.header();
        if header.index_node_size() == 0 || header.features_count() == 0 {
            return Err(JsValue::from_str("NoIndex"));
        }
        let count = header.features_count() as usize;
        let header_len = self.header_len();

        // request up to this many extra bytes if it means we can eliminate an extra request
        let combine_request_threshold = 256 * 1024;

        let list = PackedRTree::http_stream_search(
            &mut self.client,
            header_len,
            count,
            PackedRTree::DEFAULT_NODE_SIZE,
            min_x,
            min_y,
            max_x,
            max_y,
            combine_request_threshold,
        )
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
        debug_assert!(
            list.windows(2)
                .all(|w| w[0].range.start() < w[1].range.start()),
            "Since the tree is traversed breadth first, list should be sorted by construction."
        );

        let count = list.len();
        let feature_batches = FeatureBatch::make_batches(list, combine_request_threshold)
            .await
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let selection = FeatureSelection::SelectBbox(SelectBbox { feature_batches });
        trace!("completed: select_bbox");
        Ok(AsyncFeatureIter {
            client: self.client,
            fbs: self.fbs,
            selection,
            count,
        })
    }
}

#[wasm_bindgen]
impl AsyncFeatureIter {
    fn _header(&self) -> Header {
        self.fbs.header()
    }
    #[wasm_bindgen]
    pub fn header(&self) -> Result<JsValue, JsValue> {
        let header = self.fbs.header();
        let cj = to_cj_metadata(&header).map_err(|e| JsValue::from_str(&e.to_string()))?;
        to_value(&cj).map_err(|e| JsValue::from_str(&e.to_string()))
    }
    /// Number of selected features (might be unknown)
    #[wasm_bindgen]
    pub fn features_count(&self) -> Option<usize> {
        if self.count > 0 {
            Some(self.count)
        } else {
            None
        }
    }
    /// Read next feature
    #[wasm_bindgen]
    pub async fn next(&mut self) -> Result<Option<JsValue>, JsValue> {
        let Some(buffer) = self
            .selection
            .next_feature_buffer(&mut self.client)
            .await
            .map_err(|e| JsValue::from_str(&e.to_string()))?
        else {
            return Ok(None);
        };

        // Not zero-copy
        self.fbs.features_buf = buffer.to_vec();
        // verify flatbuffer
        let feature = size_prefixed_root_as_city_feature(&self.fbs.features_buf)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let cj_feature = to_cj_feature(feature, self._header().columns())
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        Ok(Some(to_value(&cj_feature)?))
    }

    #[wasm_bindgen]
    pub fn cur_cj_feature(&self) -> Result<JsValue, JsValue> {
        let cj_feature = to_cj_feature(self.fbs.feature(), self._header().columns())
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(to_value(&cj_feature)?)
    }
}

enum FeatureSelection {
    SelectAll(SelectAll),
    SelectBbox(SelectBbox),
}

impl FeatureSelection {
    async fn next_feature_buffer<T: AsyncHttpRangeClient>(
        &mut self,
        client: &mut AsyncBufferedHttpRangeClient<T>,
    ) -> Result<Option<Bytes>, Error> {
        match self {
            FeatureSelection::SelectAll(select_all) => select_all.next_buffer(client).await,
            FeatureSelection::SelectBbox(select_bbox) => select_bbox.next_buffer(client).await,
        }
    }
}

struct SelectAll {
    /// Features left
    features_left: u64,

    /// How many bytes into the file we've read so far
    pos: usize,
}

impl SelectAll {
    async fn next_buffer<T: AsyncHttpRangeClient>(
        &mut self,
        client: &mut AsyncBufferedHttpRangeClient<T>,
    ) -> Result<Option<Bytes>, Error> {
        client.min_req_size(DEFAULT_HTTP_FETCH_SIZE);

        if self.features_left == 0 {
            return Ok(None);
        }
        self.features_left -= 1;

        let mut feature_buffer =
            BytesMut::from(client.get_range(self.pos, 4).await.map_err(|_| Error)?);
        self.pos += 4;
        let feature_size = LittleEndian::read_u32(&feature_buffer) as usize;
        feature_buffer.put(
            client
                .get_range(self.pos, feature_size)
                .await
                .map_err(|_| Error)?,
        );
        self.pos += feature_size;

        Ok(Some(feature_buffer.freeze()))
    }
}

struct SelectBbox {
    /// Selected features
    feature_batches: Vec<FeatureBatch>,
}

impl SelectBbox {
    async fn next_buffer<T: AsyncHttpRangeClient>(
        &mut self,
        client: &mut AsyncBufferedHttpRangeClient<T>,
    ) -> Result<Option<Bytes>, Error> {
        let mut next_buffer = None;
        while next_buffer.is_none() {
            let Some(feature_batch) = self.feature_batches.last_mut() else {
                break;
            };
            let Some(buffer) = feature_batch.next_buffer(client).await? else {
                // done with this batch
                self.feature_batches
                    .pop()
                    .expect("already asserted feature_batches was non-empty");
                continue;
            };
            next_buffer = Some(buffer)
        }

        Ok(next_buffer)
    }
}

struct FeatureBatch {
    /// The byte location of each feature within the file
    feature_ranges: VecDeque<HttpRange>,
}

impl FeatureBatch {
    async fn make_batches(
        feature_ranges: Vec<HttpSearchResultItem>,
        combine_request_threshold: usize,
    ) -> Result<Vec<Self>, Error> {
        let mut batched_ranges = vec![];

        for search_result_item in feature_ranges.into_iter() {
            let Some(latest_batch) = batched_ranges.last_mut() else {
                let mut new_batch = VecDeque::new();
                new_batch.push_back(search_result_item.range);
                batched_ranges.push(new_batch);
                continue;
            };

            let previous_item = latest_batch.back().expect("we never push an empty batch");

            let HttpRange::Range(Range { end: prev_end, .. }) = previous_item else {
                debug_assert!(false, "This shouldn't happen. Only the very last feature is expected to have an unknown length");
                let mut new_batch = VecDeque::new();
                new_batch.push_back(search_result_item.range);
                batched_ranges.push(new_batch);
                continue;
            };

            let wasted_bytes = search_result_item.range.start() - prev_end;
            if wasted_bytes < combine_request_threshold {
                if wasted_bytes == 0 {
                    trace!("adjacent feature");
                } else {
                    trace!("wasting {wasted_bytes} to avoid an extra request");
                }
                latest_batch.push_back(search_result_item.range)
            } else {
                trace!("creating a new request for batch rather than wasting {wasted_bytes} bytes");
                let mut new_batch = VecDeque::new();
                new_batch.push_back(search_result_item.range);
                batched_ranges.push(new_batch);
            }
        }

        let mut batches: Vec<_> = batched_ranges.into_iter().map(FeatureBatch::new).collect();
        batches.reverse();
        Ok(batches)
    }

    fn new(feature_ranges: VecDeque<HttpRange>) -> Self {
        Self { feature_ranges }
    }

    /// When fetching new data, how many bytes should we fetch at once.
    /// It was computed based on the specific feature ranges of the batch
    /// to optimize number of requests vs. wasted bytes vs. resident memory
    fn request_size(&self) -> usize {
        let Some(first) = self.feature_ranges.front() else {
            return 0;
        };
        let Some(last) = self.feature_ranges.back() else {
            return 0;
        };

        // `last.length()` should only be None if this batch includes the final feature
        // in the dataset. Since we can't infer its actual length, we'll fetch only
        // the first 4 bytes of that feature buffer, which will tell us the actual length
        // of the feature buffer for the subsequent request.
        let last_feature_length = last.length().unwrap_or(4);

        let covering_range = first.start()..last.start() + last_feature_length;

        covering_range
            .len()
            // Since it's all held in memory, don't fetch more than DEFAULT_HTTP_FETCH_SIZE at a time
            // unless necessary.
            .min(DEFAULT_HTTP_FETCH_SIZE)
    }

    async fn next_buffer<T: AsyncHttpRangeClient>(
        &mut self,
        client: &mut AsyncBufferedHttpRangeClient<T>,
    ) -> Result<Option<Bytes>, Error> {
        let request_size = self.request_size();
        client.set_min_req_size(request_size);
        let Some(feature_range) = self.feature_ranges.pop_front() else {
            return Ok(None);
        };

        let mut pos = feature_range.start();
        let mut feature_buffer = BytesMut::from(client.get_range(pos, 4).await.map_err(|_| Error)?);
        pos += 4;
        let feature_size = LittleEndian::read_u32(&feature_buffer) as usize;
        feature_buffer.put(
            client
                .get_range(pos, feature_size)
                .await
                .map_err(|_| Error)?,
        );

        Ok(Some(feature_buffer.freeze()))
    }
}
