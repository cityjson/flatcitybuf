#![cfg(target_arch = "wasm32")]
use fcb_core::deserializer::{to_cj_feature, to_cj_metadata};
use fcb_core::{size_prefixed_root_as_header, Header};
// #[cfg(target_arch = "wasm32")]
use gloo_client::WasmHttpClient;
use js_sys::Array;
use log::Level;
#[cfg(target_arch = "wasm32")]
use log::{debug, info, trace};
use serde_wasm_bindgen::to_value;
use wasm_bindgen::prelude::*;

use bst::{ByteSerializableValue, MultiIndex, Operator, OrderedFloat};

use byteorder::{ByteOrder, LittleEndian};
use bytes::{BufMut, Bytes, BytesMut};
use chrono::NaiveDateTime;
use fcb_core::city_buffer::FcbBuffer;
use fcb_core::{
    build_query, check_magic_bytes, fb::*, process_attr_index_entry,
    size_prefixed_root_as_city_feature, AttrQuery, HEADER_MAX_BUFFER_SIZE, HEADER_SIZE_SIZE,
    MAGIC_BYTES_SIZE,
};

use std::fmt::Error;
use std::result::Result;
use std::sync::atomic::{AtomicBool, Ordering};

use http_range_client::{AsyncBufferedHttpRangeClient, AsyncHttpRangeClient};

use packed_rtree::{http::HttpRange, http::HttpSearchResultItem, NodeItem, PackedRTree};
use std::collections::VecDeque;
use std::ops::Range;

mod gloo_client;

// The largest request we'll speculatively make.
// If a single huge feature requires, we'll necessarily exceed this limit.
const DEFAULT_HTTP_FETCH_SIZE: usize = 1_048_576; // 1MB

// Static variable to track if logger has been initialized
static LOGGER_INITIALIZED: AtomicBool = AtomicBool::new(false);

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

#[wasm_bindgen(start)]
impl HttpFcbReader {
    #[wasm_bindgen(constructor, start)]
    pub async fn new(url: String) -> Result<HttpFcbReader, JsValue> {
        println!("open===: {:?}", url);

        // Only initialize the logger once
        if !LOGGER_INITIALIZED.load(Ordering::SeqCst) {
            if let Ok(_) = console_log::init_with_level(Level::Trace) {
                LOGGER_INITIALIZED.store(true, Ordering::SeqCst);
                log::info!("Logger initialized successfully.");
            }
        }

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

    #[wasm_bindgen]
    pub fn free(self) {
        println!("freeing reader");
    }

    fn header_len(&self) -> usize {
        MAGIC_BYTES_SIZE + self.fbs.header_buf.len()
    }

    fn rtree_index_size(&self) -> u64 {
        let header = self.fbs.header();
        let feat_count = header.features_count() as usize;
        if header.index_node_size() > 0 && feat_count > 0 {
            PackedRTree::index_size(feat_count, header.index_node_size()) as u64
        } else {
            0
        }
    }

    fn attr_index_size(&self) -> u64 {
        let header = self.fbs.header();
        header
            .attribute_index()
            .map(|attr_index| {
                attr_index
                    .iter()
                    .try_fold(0u32, |acc, ai| {
                        let len = ai.length();
                        if len > u32::MAX - acc {
                            Err(JsValue::from_str("attribute index size overflow"))
                        } else {
                            Ok(acc + len)
                        }
                    }) // sum of all attribute index lengths
                    .unwrap_or(0) as u64
            })
            .unwrap_or(0)
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
        let attr_index_size = self.attr_index_size() as usize;

        let list = PackedRTree::http_stream_search(
            &mut self.client,
            header_len,
            attr_index_size,
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

    #[wasm_bindgen]
    pub async fn select_attr_query(
        mut self,
        query: &WasmAttrQuery,
    ) -> Result<AsyncFeatureIter, JsValue> {
        trace!("starting: select_attr_query via http reader");
        log::info!("select_attr_query: {:?}", query);
        let header = self.fbs.header();
        let header_len = self.header_len();
        // Assume the header provides rtree and attribute index sizes.
        let rtree_index_size = self.rtree_index_size() as usize;
        let attr_index_size = self.attr_index_size() as usize;
        let attr_index_offset = header_len + rtree_index_size;
        let feature_begin = header_len + rtree_index_size + attr_index_size;

        // Fetch the attribute index block via HTTP range request.
        let mut attr_index_bytes = self
            .client
            .get_range(attr_index_offset, attr_index_size)
            .await
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        let attr_index_entries = header
            .attribute_index()
            .ok_or_else(|| JsValue::from_str("attribute index not found"))?;
        let columns: Vec<Column> = header
            .columns()
            .ok_or_else(|| JsValue::from_str("no columns found in header"))?
            .iter()
            .collect();

        let mut multi_index = MultiIndex::new();

        for attr_info in attr_index_entries.iter() {
            process_attr_index_entry(
                &mut attr_index_bytes,
                &mut multi_index,
                &columns,
                &query.inner,
                attr_info,
            )
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        }

        let query = build_query(&query.inner);

        let result = bst::stream_query(&multi_index, query, feature_begin)
            .await
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        let count = result.len();
        let combine_request_threshold = 256 * 1024;

        let http_ranges: Vec<HttpRange> = result.into_iter().map(|item| item.range).collect();

        trace!(
            "completed: select_attr_query via http reader, matched features: {}",
            count
        );
        Ok(AsyncFeatureIter {
            client: self.client,
            fbs: self.fbs,
            selection: FeatureSelection::SelectAttr(SelectAttr {
                ranges: http_ranges,
                range_pos: 0,
            }),
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

    #[wasm_bindgen]
    pub fn free(self) {
        println!("freeing iterator");
    }
}

enum FeatureSelection {
    SelectAll(SelectAll),
    SelectBbox(SelectBbox),
    SelectAttr(SelectAttr),
}

impl FeatureSelection {
    async fn next_feature_buffer<T: AsyncHttpRangeClient>(
        &mut self,
        client: &mut AsyncBufferedHttpRangeClient<T>,
    ) -> Result<Option<Bytes>, Error> {
        match self {
            FeatureSelection::SelectAll(select_all) => select_all.next_buffer(client).await,
            FeatureSelection::SelectBbox(select_bbox) => select_bbox.next_buffer(client).await,
            FeatureSelection::SelectAttr(select_attr) => select_attr.next_buffer(client).await,
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

struct SelectAttr {
    // TODO: change this implementation so it can batch features
    ranges: Vec<HttpRange>,
    range_pos: usize,
}

impl SelectAttr {
    async fn next_buffer<T: AsyncHttpRangeClient>(
        &mut self,
        client: &mut AsyncBufferedHttpRangeClient<T>,
    ) -> Result<Option<Bytes>, Error> {
        let Some(range) = self.ranges.get(self.range_pos) else {
            return Ok(None);
        };
        let mut feature_buffer = BytesMut::from(
            client
                .get_range(range.start(), 4)
                .await
                .map_err(|_| Error)?,
        );
        let feature_size = LittleEndian::read_u32(&feature_buffer) as usize;
        feature_buffer.put(
            client
                .get_range(range.start() + 4, feature_size)
                .await
                .map_err(|_| Error)?,
        );
        self.range_pos += 1;
        Ok(Some(feature_buffer.freeze()))
    }
}

/// A wasm‑friendly wrapper over `AttrQuery`, which is defined as:
/// `pub type AttrQuery = Vec<(String, Operator, ByteSerializableValue)>;`
#[wasm_bindgen]
#[derive(Debug)]
pub struct WasmAttrQuery {
    inner: AttrQuery,
}

#[wasm_bindgen]
impl WasmAttrQuery {
    /// Creates a new WasmAttrQuery from a JS array of query tuples.
    ///
    /// Each query tuple must be an array of three elements:
    /// [field: string, operator: string, value: number | boolean | string | Date]
    ///
    /// For example, in JavaScript you could pass:
    /// `[ ["b3_h_dak_50p", "Gt", 2.0],
    ///   ["identificatie", "Eq", "NL.IMBAG.Pand.0503100000012869"],
    ///   ["created", "Ge", new Date("2020-01-01T00:00:00Z")] ]`
    #[wasm_bindgen(constructor)]
    pub fn new(js_value: &JsValue) -> Result<WasmAttrQuery, JsValue> {
        // Expect the JS value to be an array of query tuples.
        let arr = Array::from(js_value);
        let mut inner: AttrQuery = Vec::new();

        for tuple in arr.iter() {
            // Each tuple is expected to be an array with at least 3 elements.
            let tuple_arr = Array::from(&tuple);
            if tuple_arr.length() < 3 {
                return Err(JsValue::from_str("Each query tuple must have 3 elements"));
            }

            // First element: field name (string)
            let field = tuple_arr
                .get(0)
                .as_string()
                .ok_or_else(|| JsValue::from_str("Field must be a string"))?;

            // Second element: operator as string, converting to the Operator enum.
            let op_str = tuple_arr
                .get(1)
                .as_string()
                .ok_or_else(|| JsValue::from_str("Operator must be a string"))?;
            let operator = match op_str.as_str() {
                "Eq" => Operator::Eq,
                "Gt" => Operator::Gt,
                "Ge" => Operator::Ge,
                "Lt" => Operator::Lt,
                "Le" => Operator::Le,
                "Ne" => Operator::Ne,
                _ => return Err(JsValue::from_str("Invalid operator value")),
            };

            // Third element: the value
            let value_js = tuple_arr.get(2);
            let bs_value = if let Some(b) = value_js.as_bool() {
                // If boolean then use Bool
                ByteSerializableValue::Bool(b)
            } else if value_js.is_instance_of::<js_sys::Date>() {
                // If a JS Date, convert to milliseconds then to a NaiveDateTime.
                let date: js_sys::Date = value_js.unchecked_into();
                let millis = date.get_time();
                let secs = (millis / 1000.0) as i64;
                let nanos = ((millis % 1000.0) * 1_000_000.0) as u32;
                match NaiveDateTime::from_timestamp_opt(secs, nanos) {
                    Some(ndt) => ByteSerializableValue::NaiveDateTime(ndt),
                    None => return Err(JsValue::from_str("Invalid Date value")),
                }
            } else if let Some(n) = value_js.as_f64() {
                // All JS numbers are f64.
                ByteSerializableValue::F64(OrderedFloat(n))
            } else if let Some(s) = value_js.as_string() {
                ByteSerializableValue::String(s)
            } else {
                return Err(JsValue::from_str("Unsupported value type in query tuple"));
            };

            inner.push((field, operator, bs_value));
        }

        Ok(WasmAttrQuery { inner })
    }

    /// Returns the inner AttrQuery as a JsValue (an array of query tuples)
    /// useful for debugging.
    #[wasm_bindgen(getter)]
    pub fn inner(&self) -> JsValue {
        let arr = Array::new();
        for (field, op, val) in self.inner.iter() {
            let tuple = Array::new();
            tuple.push(&JsValue::from_str(field));
            let op_str = match op {
                Operator::Eq => "Eq",
                Operator::Gt => "Gt",
                Operator::Ge => "Ge",
                Operator::Lt => "Lt",
                Operator::Le => "Le",
                Operator::Ne => "Ne",
            };
            tuple.push(&JsValue::from_str(op_str));
            let val_js = match val {
                ByteSerializableValue::I64(n) => JsValue::from_f64(*n as f64),
                ByteSerializableValue::I32(n) => JsValue::from_f64(*n as f64),
                ByteSerializableValue::I16(n) => JsValue::from_f64(*n as f64),
                ByteSerializableValue::I8(n) => JsValue::from_f64(*n as f64),
                ByteSerializableValue::U64(n) => JsValue::from_f64(*n as f64),
                ByteSerializableValue::U32(n) => JsValue::from_f64(*n as f64),
                ByteSerializableValue::U16(n) => JsValue::from_f64(*n as f64),
                ByteSerializableValue::U8(n) => JsValue::from_f64(*n as f64),
                ByteSerializableValue::F64(f) => JsValue::from_f64(f.into_inner()),
                ByteSerializableValue::F32(f) => JsValue::from_f64(f.into_inner() as f64),
                ByteSerializableValue::Bool(b) => JsValue::from_bool(*b),
                ByteSerializableValue::String(s) => JsValue::from_str(s),
                ByteSerializableValue::NaiveDateTime(ndt) => JsValue::from_str(&ndt.to_string()),
                ByteSerializableValue::NaiveDate(nd) => JsValue::from_str(&nd.to_string()),
                ByteSerializableValue::DateTime(dt) => JsValue::from_str(&dt.to_rfc3339()),
            };
            tuple.push(&val_js);
            arr.push(&tuple);
        }
        arr.into()
    }
}
