use crate::deserializer::to_cj_feature;
use crate::{add_indices_to_multi_memory_index, build_query, fb::*, AttrQuery};

use crate::error::{Error, Result};
use crate::reader::city_buffer::FcbBuffer;
use crate::{
    check_magic_bytes, size_prefixed_root_as_city_feature, HEADER_MAX_BUFFER_SIZE,
    HEADER_SIZE_SIZE, MAGIC_BYTES_SIZE,
};
use byteorder::{ByteOrder, LittleEndian};
use bytes::{BufMut, Bytes, BytesMut};
use chrono::{DateTime, Utc};
use cjseq::CityJSONFeature;
use http_range_client::BufferedHttpRangeClient;
use http_range_client::{AsyncBufferedHttpRangeClient, AsyncHttpRangeClient};
use reqwest;
use static_btree::{FixedStringKey, Float, KeyType, Operator};

use packed_rtree::{http::HttpRange, http::HttpSearchResultItem, NodeItem, PackedRTree};
use static_btree::{
    http::HttpRange as AttrHttpRange, http::HttpSearchResultItem as AttrHttpSearchResultItem,
};
use static_btree::{HttpIndex, HttpMultiIndex};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::ops::Range;
use tracing::debug;
use tracing::trace;

#[cfg(test)]
mod mock_http_range_client;

// The largest request we'll speculatively make.
// If a single huge feature requires, we'll necessarily exceed this limit.
const DEFAULT_HTTP_FETCH_SIZE: usize = 1_048_576; // 1MB

/// FlatCityBuf dataset HTTP reader
pub struct HttpFcbReader<T: AsyncHttpRangeClient + Send + Sync> {
    client: AsyncBufferedHttpRangeClient<T>,
    // feature reading requires header access, therefore
    // header_buf is included in the FcbBuffer struct.
    fbs: FcbBuffer,
}

pub struct AsyncFeatureIter<T: AsyncHttpRangeClient + Send + Sync> {
    client: AsyncBufferedHttpRangeClient<T>,
    // feature reading requires header access, therefore
    // header_buf is included in the FcbBuffer struct.
    fbs: FcbBuffer,
    /// Which features to iterate
    selection: FeatureSelection,
    /// Number of selected features
    count: usize,
}

impl HttpFcbReader<reqwest::Client> {
    pub async fn open(url: &str) -> Result<HttpFcbReader<reqwest::Client>> {
        trace!("starting: opening http reader, reading header");
        let client = BufferedHttpRangeClient::new(url);
        Self::_open(client).await
    }
}

impl<T: AsyncHttpRangeClient + Send + Sync> HttpFcbReader<T> {
    pub async fn new(client: AsyncBufferedHttpRangeClient<T>) -> Result<HttpFcbReader<T>> {
        Self::_open(client).await
    }

    async fn _open(mut client: AsyncBufferedHttpRangeClient<T>) -> Result<HttpFcbReader<T>> {
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
        let mut read_bytes = 0;
        let bytes = client.get_range(read_bytes, MAGIC_BYTES_SIZE).await?; // to get magic bytes
        if !check_magic_bytes(bytes) {
            return Err(Error::MissingMagicBytes);
        }

        read_bytes += MAGIC_BYTES_SIZE;
        let mut bytes = BytesMut::from(client.get_range(read_bytes, HEADER_SIZE_SIZE).await?);
        read_bytes += HEADER_SIZE_SIZE;

        let header_size = LittleEndian::read_u32(&bytes) as usize;
        if header_size > HEADER_MAX_BUFFER_SIZE || header_size < 8 {
            // minimum size check avoids panic in FlatBuffers header decoding
            return Err(Error::IllegalHeaderSize(header_size));
        }

        bytes.put(client.get_range(read_bytes, header_size).await?);
        read_bytes += header_size;

        let header_buf = bytes.to_vec();

        // verify flatbuffer
        let _header = size_prefixed_root_as_header(&header_buf)?;

        trace!("completed: opening http reader");
        Ok(HttpFcbReader {
            client,
            fbs: FcbBuffer {
                header_buf,
                features_buf: Vec::new(),
            },
        })
    }

    pub fn header(&self) -> Header {
        self.fbs.header()
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
                            Err(Error::AttributeIndexSizeOverflow)
                        } else {
                            Ok(acc + len)
                        }
                    }) // sum of all attribute index lengths
                    .unwrap_or(0) as u64
            })
            .unwrap_or(0) as u64
    }

    fn index_size(&self) -> u64 {
        self.rtree_index_size() + self.attr_index_size()
    }

    /// Select all features.
    pub async fn select_all(self) -> Result<AsyncFeatureIter<T>> {
        let header = self.fbs.header();
        let count = header.features_count();
        let index_size = self.index_size() as usize;
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
    pub async fn select_bbox(
        mut self,
        min_x: f64,
        min_y: f64,
        max_x: f64,
        max_y: f64,
    ) -> Result<AsyncFeatureIter<T>> {
        trace!("starting: select_bbox, traversing index");
        // Read R-Tree index and build filter for features within bbox
        let header = self.fbs.header();
        if header.index_node_size() == 0 || header.features_count() == 0 {
            return Err(Error::NoIndex);
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
        .await?;
        debug_assert!(
            list.windows(2)
                .all(|w| w[0].range.start() < w[1].range.start()),
            "Since the tree is traversed breadth first, list should be sorted by construction."
        );

        let count = list.len();
        let feature_batches = FeatureBatch::make_batches(list, combine_request_threshold).await?;
        let selection = FeatureSelection::SelectBbox(SelectBbox { feature_batches });
        trace!("completed: select_bbox");
        Ok(AsyncFeatureIter {
            client: self.client,
            fbs: self.fbs,
            selection,
            count,
        })
    }

    /// This method uses the attribute index section to find matching feature offsets.
    /// It then groups (batches) the remote feature ranges in order to reduce IO overhead.
    pub async fn select_attr_query(mut self, query: &AttrQuery) -> Result<AsyncFeatureIter<T>> {
        trace!("starting: select_attr_query via http reader");
        let header = self.fbs.header();
        let header_len = self.header_len();
        // Assume the header provides rtree and attribute index sizes.

        // file structure:
        // magic_bytes + header + rtree_index + attr_index1 + attr_index2 + ... + features
        let rtree_index_size = self.rtree_index_size() as usize;
        let attr_index_size = self.attr_index_size() as usize;
        let attr_index_begin = header_len + rtree_index_size;
        let feature_begin = header_len + rtree_index_size + attr_index_size;

        let attr_index_entries = header
            .attribute_index()
            .ok_or_else(|| Error::AttributeIndexNotFound)?;
        let mut attr_index_entries = attr_index_entries.iter().collect::<Vec<_>>();
        let columns: Vec<Column> = header
            .columns()
            .ok_or_else(|| Error::NoColumnsInHeader)?
            .iter()
            .collect();
        attr_index_entries.sort_by_key(|attr_info| attr_info.index());

        // Build the query
        let query = build_query(&query);

        // Create a StreamableMultiIndex from HTTP range requests
        let mut http_multi_index = HttpMultiIndex::new();

        let mut current_index_begin = attr_index_begin;
        for attr_info in attr_index_entries.iter() {
            Self::add_indices_to_multi_http_index(
                &mut http_multi_index,
                &columns,
                attr_info,
                current_index_begin,
                feature_begin,
            )?;
            current_index_begin += attr_info.length() as usize;
        }

        let result = http_multi_index
            .query(&mut self.client, &query.conditions)
            .await?;

        let count = result.len();

        let http_ranges: Vec<HttpRange> = result
            .into_iter()
            .map(|item| match item.range {
                AttrHttpRange::Range(range) => HttpRange::Range(range.start..range.end),
                AttrHttpRange::RangeFrom(range) => HttpRange::RangeFrom(range.start..),
            })
            .collect();

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

    pub fn add_indices_to_multi_http_index<C: AsyncHttpRangeClient + Send + Sync>(
        multi_index: &mut HttpMultiIndex<C>,
        columns: &[Column],
        attr_info: &AttributeIndex,
        index_begin: usize,
        feature_begin: usize,
    ) -> Result<()> {
        if let Some(col) = columns.iter().find(|col| col.index() == attr_info.index()) {
            // TODO: now it assuming to add all indices to the multi_index. However, we should only add the indices that are used in the query. To do that, we need to change the implementation of StreamMultiIndex. Current StreamMultiIndex's `add_index` method assumes that all indices are added to the multi_index. We'll change it to take Range<usize> as an argument.
            let index_begin = index_begin as u64;
            match col.type_() {
                ColumnType::Int => {
                    let index = HttpIndex::<i32>::new(
                        attr_info.num_unique_items() as usize,
                        attr_info.branching_factor(),
                        index_begin as usize,
                        feature_begin as usize,
                        1024 * 1024, // combine_request_threshold
                    );
                    multi_index.add_index(col.name().to_string(), index);
                }
                ColumnType::Float => {
                    let index = HttpIndex::<Float<f32>>::new(
                        attr_info.num_unique_items() as usize,
                        attr_info.branching_factor(),
                        index_begin as usize,
                        feature_begin as usize,
                        1024 * 1024, // combine_request_threshold
                    );
                    multi_index.add_index(col.name().to_string(), index);
                }
                ColumnType::Double => {
                    let index = HttpIndex::<Float<f64>>::new(
                        attr_info.num_unique_items() as usize,
                        attr_info.branching_factor(),
                        index_begin as usize,
                        feature_begin as usize,
                        1024 * 1024, // combine_request_threshold
                    );
                    multi_index.add_index(col.name().to_string(), index);
                }
                ColumnType::String => {
                    let index = HttpIndex::<FixedStringKey<50>>::new(
                        attr_info.num_unique_items() as usize,
                        attr_info.branching_factor(),
                        index_begin as usize,
                        feature_begin as usize,
                        1024 * 1024, // combine_request_threshold
                    );
                    multi_index.add_index(col.name().to_string(), index);
                }

                ColumnType::Bool => {
                    let index = HttpIndex::<bool>::new(
                        attr_info.num_unique_items() as usize,
                        attr_info.branching_factor(),
                        index_begin as usize,
                        feature_begin as usize,
                        1024 * 1024, // combine_request_threshold
                    );
                    multi_index.add_index(col.name().to_string(), index);
                }
                ColumnType::DateTime => {
                    let index = HttpIndex::<DateTime<Utc>>::new(
                        attr_info.num_unique_items() as usize,
                        attr_info.branching_factor(),
                        index_begin as usize,
                        feature_begin as usize,
                        1024 * 1024, // combine_request_threshold
                    );
                    multi_index.add_index(col.name().to_string(), index);
                }
                _ => return Err(Error::UnsupportedColumnType(col.name().to_string())),
            }
        }
        Ok(())
    }

    // Add WASM-specific version without Send + Sync bounds
    #[cfg(target_arch = "wasm32")]
    pub fn add_indices_to_multi_wasm_http_index<C: AsyncHttpRangeClient>(
        multi_index: &mut HttpMultiIndex<C>,
        columns: &[Column],
        attr_info: &AttributeIndex,
        index_begin: usize,
        feature_begin: usize,
    ) -> Result<()> {
        if let Some(col) = columns.iter().find(|col| col.index() == attr_info.index()) {
            // TODO: now it assuming to add all indices to the multi_index. However, we should only add the indices that are used in the query. To do that, we need to change the implementation of StreamMultiIndex. Current StreamMultiIndex's `add_index` method assumes that all indices are added to the multi_index. We'll change it to take Range<usize> as an argument.
            let index_begin = index_begin as u64;
            match col.type_() {
                ColumnType::Int => {
                    let index = HttpIndex::<i32>::new(
                        attr_info.num_unique_items() as usize,
                        attr_info.branching_factor(),
                        index_begin as usize,
                        feature_begin as usize,
                        1024 * 1024, // combine_request_threshold
                    );
                    multi_index.add_index(col.name().to_string(), index);
                }
                ColumnType::Float => {
                    let index = HttpIndex::<Float<f32>>::new(
                        attr_info.num_unique_items() as usize,
                        attr_info.branching_factor(),
                        index_begin as usize,
                        feature_begin as usize,
                        1024 * 1024, // combine_request_threshold
                    );
                    multi_index.add_index(col.name().to_string(), index);
                }
                ColumnType::Double => {
                    let index = HttpIndex::<Float<f64>>::new(
                        attr_info.num_unique_items() as usize,
                        attr_info.branching_factor(),
                        index_begin as usize,
                        feature_begin as usize,
                        1024 * 1024, // combine_request_threshold
                    );
                    multi_index.add_index(col.name().to_string(), index);
                }
                ColumnType::String => {
                    let index = HttpIndex::<FixedStringKey<50>>::new(
                        attr_info.num_unique_items() as usize,
                        attr_info.branching_factor(),
                        index_begin as usize,
                        feature_begin as usize,
                        1024 * 1024, // combine_request_threshold
                    );
                    multi_index.add_index(col.name().to_string(), index);
                }

                ColumnType::Bool => {
                    let index = HttpIndex::<bool>::new(
                        attr_info.num_unique_items() as usize,
                        attr_info.branching_factor(),
                        index_begin as usize,
                        feature_begin as usize,
                        1024 * 1024, // combine_request_threshold
                    );
                    multi_index.add_index(col.name().to_string(), index);
                }
                ColumnType::DateTime => {
                    let index = HttpIndex::<DateTime<Utc>>::new(
                        attr_info.num_unique_items() as usize,
                        attr_info.branching_factor(),
                        index_begin as usize,
                        feature_begin as usize,
                        1024 * 1024, // combine_request_threshold
                    );
                    multi_index.add_index(col.name().to_string(), index);
                }
                _ => return Err(Error::UnsupportedColumnType(col.name().to_string())),
            }
        }
        Ok(())
    }
}

impl<T: AsyncHttpRangeClient + Send + Sync> AsyncFeatureIter<T> {
    pub fn header(&self) -> Header {
        self.fbs.header()
    }
    /// Number of selected features (might be unknown)
    pub fn features_count(&self) -> Option<usize> {
        if self.count > 0 {
            Some(self.count)
        } else {
            None
        }
    }
    /// Read next feature
    pub async fn next(&mut self) -> Result<Option<&FcbBuffer>> {
        let Some(buffer) = self.selection.next_feature_buffer(&mut self.client).await? else {
            return Ok(None);
        };

        // Not zero-copy
        self.fbs.features_buf = buffer.to_vec();
        // verify flatbuffer
        let _feature = size_prefixed_root_as_city_feature(&self.fbs.features_buf)?;
        Ok(Some(&self.fbs))
    }
    /// Return current feature
    pub fn cur_feature(&self) -> &FcbBuffer {
        &self.fbs
    }

    pub fn cur_cj_feature(&self) -> Result<CityJSONFeature> {
        let cj_feature = to_cj_feature(self.cur_feature().feature(), self.header().columns())?;
        Ok(cj_feature)
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
    ) -> Result<Option<Bytes>> {
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
    ) -> Result<Option<Bytes>> {
        client.min_req_size(DEFAULT_HTTP_FETCH_SIZE);

        if self.features_left == 0 {
            return Ok(None);
        }
        self.features_left -= 1;

        let mut feature_buffer = BytesMut::from(client.get_range(self.pos, 4).await?);
        self.pos += 4;
        let feature_size = LittleEndian::read_u32(&feature_buffer) as usize;
        feature_buffer.put(client.get_range(self.pos, feature_size).await?);
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
    ) -> Result<Option<Bytes>> {
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
    ) -> Result<Vec<Self>> {
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
    ) -> Result<Option<Bytes>> {
        let request_size = self.request_size();
        client.set_min_req_size(request_size);
        let Some(feature_range) = self.feature_ranges.pop_front() else {
            return Ok(None);
        };

        let mut pos = feature_range.start();
        let mut feature_buffer = BytesMut::from(client.get_range(pos, 4).await?);
        pos += 4;
        let feature_size = LittleEndian::read_u32(&feature_buffer) as usize;
        feature_buffer.put(client.get_range(pos, feature_size).await?);

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
    ) -> Result<Option<Bytes>> {
        let Some(range) = self.ranges.get(self.range_pos) else {
            return Ok(None);
        };
        let mut feature_buffer = BytesMut::from(client.get_range(range.start(), 4).await?);
        let feature_size = LittleEndian::read_u32(&feature_buffer) as usize;
        feature_buffer.put(client.get_range(range.start() + 4, feature_size).await?);
        self.range_pos += 1;
        Ok(Some(feature_buffer.freeze()))
    }
}
#[cfg(test)]
mod tests {
    use std::{path::PathBuf, str::FromStr};

    use cjseq::CityJSONFeature;
    use static_btree::{FixedStringKey, Float, KeyType, Operator};

    use crate::error::Result;
    use crate::HttpFcbReader;

    #[tokio::test]
    async fn fcb_http_reader_test() -> Result<()> {
        #[derive(Debug)]
        struct QueryTestCase {
            test_name: &'static str,
            query: Vec<(String, Operator, KeyType)>,
            expected_count: usize,
            validator: fn(&CityJSONFeature) -> bool,
        }

        let test_cases = vec![
                    // Test case: Expect one matching feature with b3_h_dak_50p > 2.0 and matching identificatie.
                    QueryTestCase {
                        test_name: "test_attr_index_multiple_queries: b3_h_dak_50p > 2.0 and identificatie == NL.IMBAG.Pand.0503100000012869",
                        query: vec![
                            (
                                "b3_h_dak_50p".to_string(),
                                Operator::Gt,
                                KeyType::Float64(Float(2.0)),
                            ),
                            (
                                "identificatie".to_string(),
                                Operator::Eq,
                                KeyType::StringKey50(FixedStringKey::from_str(
                                    "NL.IMBAG.Pand.0503100000012869",
                                )),
                            ),
                        ],
                        expected_count: 1,
                        validator: |feature: &CityJSONFeature| {
                            let mut valid_b3 = false;
                            let mut valid_ident = false;
                            for co in feature.city_objects.values() {
                                if let Some(attrs) = &co.attributes {
                                    if let Some(val) = attrs.get("b3_h_dak_50p") {
                                        if val.as_f64().unwrap() > 2.0 {
                                            valid_b3 = true;
                                        }
                                    }
                                    if let Some(ident) = attrs.get("identificatie") {
                                        if ident.as_str().unwrap() == "NL.IMBAG.Pand.0503100000012869" {
                                            valid_ident = true;
                                        }
                                    }
                                }
                            }
                            valid_b3 && valid_ident
                        },
                    },
                    // Test case: Expect zero features where tijdstipregistratie is before 2008-01-01.
                    QueryTestCase {
                        test_name: "test_attr_index_multiple_queries: tijdstipregistratie < 2008-01-01",
                        query: vec![(
                            "tijdstipregistratie".to_string(),
                            Operator::Lt,
                            KeyType::DateTime(chrono::DateTime::<chrono::Utc>::from_str(
                                "2008-01-01T00:00:00Z",
                            )
                            .unwrap()),
                        )],
                        expected_count: 0,
                        validator: |feature: &CityJSONFeature| {
                            let mut valid_tijdstip = true;
                            let query_tijdstip = chrono::NaiveDate::from_ymd(2008, 1, 1).and_hms(0, 0, 0);
                            for co in feature.city_objects.values() {
                                if let Some(attrs) = &co.attributes {
                                    if let Some(val) = attrs.get("tijdstipregistratie") {
                                        let val_tijdstip = chrono::NaiveDateTime::parse_from_str(
                                            val.as_str().unwrap(),
                                            "%Y-%m-%dT%H:%M:%S",
                                        )
                                        .unwrap();
                                        if val_tijdstip < query_tijdstip {
                                            valid_tijdstip = false;
                                        }
                                    }
                                }
                            }
                            valid_tijdstip
                        },
                    },
                    // Test case: Expect zero features where tijdstipregistratie is after 2008-01-01.
                    QueryTestCase {
                        test_name: "test_attr_index_multiple_queries: tijdstipregistratie > 2008-01-01",
                        query: vec![(
                            "tijdstipregistratie".to_string(),
                            Operator::Gt,
                            KeyType::DateTime(chrono::DateTime::<chrono::Utc>::from_utc(
                                chrono::NaiveDate::from_ymd(2008, 1, 1).and_hms(0, 0, 0),
                                chrono::Utc,
                            )),
                        )],
                        expected_count: 3,
                        validator: |feature: &CityJSONFeature| {
                            let mut valid_tijdstip = false;
                            let query_tijdstip = chrono::NaiveDate::from_ymd(2008, 1, 1).and_hms(0, 0, 0);
                            for co in feature.city_objects.values() {
                                if let Some(attrs) = &co.attributes {
                                    if let Some(val) = attrs.get("tijdstipregistratie") {
                                        let val_tijdstip =
                                            chrono::DateTime::parse_from_rfc3339(val.as_str().unwrap())
                                                .map_err(|e| eprintln!("Failed to parse datetime: {}", e))
                                                .map(|dt| dt.naive_utc())
                                                .unwrap_or_else(|_| {
                                                    chrono::NaiveDateTime::from_timestamp_opt(0, 0).unwrap()
                                                });
                                        if val_tijdstip > query_tijdstip {
                                            valid_tijdstip = true;
                                        }
                                    }
                                }
                            }
                            valid_tijdstip
                        },
                    },
                ];

        for test_case in test_cases {
            let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let input_file_path = manifest_dir.join("tests/data/small.fcb");

            let (fcb, stats) = HttpFcbReader::mock_from_file(&input_file_path.to_str().unwrap())
                .await
                .unwrap();

            // {
            //     // The read guard needs to be in a scoped block, else we won't release the lock and the test will hang when
            //     // the actual FGB client code tries to update the stats.
            //     let stats = stats.read().unwrap();
            //     assert_eq!(stats.request_count, 1);
            //     // This number might change a little if the test data or logic changes, but they should be in the same ballpark.
            //     assert_eq!(stats.bytes_requested, 12944);
            // }

            let query = test_case.query;
            let mut iter = fcb.select_attr_query(&query).await.unwrap();

            let mut features = Vec::new();
            while let Some(feat_buf) = iter.next().await.unwrap() {
                let feature = feat_buf.cj_feature()?;
                features.push(feature);
            }
            assert_eq!(features.len(), test_case.expected_count);

            for feature in features {
                assert!(
                    (test_case.validator)(&feature),
                    "Failed to validate feature in test case: {}",
                    test_case.test_name
                );
            }
        }

        // {
        //     let stats = stats.read().unwrap();

        //     assert_eq!(stats.request_count, 5);
        //     assert_eq!(stats.bytes_requested, 2131152);
        // }
        Ok(())
    }
}
