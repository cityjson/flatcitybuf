use std::ops::Range;


use crate::{sorted_index::ValueOffset, ByteSerializable};

#[cfg(feature = "http")]
#[derive(Debug, Clone)]
pub enum HttpRange {
    Range(Range<usize>),
    RangeFrom(std::ops::RangeFrom<usize>),
}

#[cfg(feature = "http")]
impl HttpRange {
    pub fn start(&self) -> usize {
        match self {
            HttpRange::Range(range) => range.start,
            HttpRange::RangeFrom(range) => range.start,
        }
    }

    pub fn end(&self) -> Option<usize> {
        match self {
            HttpRange::Range(range) => Some(range.end),
            HttpRange::RangeFrom(_) => None,
        }
    }
}

#[cfg(feature = "http")]
#[derive(Debug, Clone)]
pub struct HttpSearchResultItem {
    /// Byte range in the feature data section
    pub range: HttpRange,
}

pub trait TypedHttpStreamableIndex<T: Ord + ByteSerializable + Send + Sync + 'static>:
    Send + Sync
{
    /// Returns the size of the index in bytes.
    fn index_size(&self) -> u64;

    /// Returns the offsets for an exact match given a key.
    /// For use with HTTP range requests.
    #[cfg(feature = "http")]
    async fn http_stream_query_exact<C: http_range_client::AsyncHttpRangeClient>(
        &self,
        client: &mut http_range_client::AsyncBufferedHttpRangeClient<C>,
        index_offset: usize,
        key: &T,
    ) -> std::io::Result<Vec<ValueOffset>>;

    /// Returns the offsets for a range query given optional lower and upper keys.
    /// For use with HTTP range requests.
    #[cfg(feature = "http")]
    async fn http_stream_query_range<C: http_range_client::AsyncHttpRangeClient>(
        &self,
        client: &mut http_range_client::AsyncBufferedHttpRangeClient<C>,
        index_offset: usize,
        lower: Option<&T>,
        upper: Option<&T>,
    ) -> std::io::Result<Vec<ValueOffset>>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpIndexMeta<T: Ord + ByteSerializable + Send + Sync + 'static> {
    /// Number of entries in the index.
    pub entry_count: u64,
    /// Total size of the index in bytes.
    pub size: u64,
    /// Phantom data to represent the type parameter.
    pub _phantom: std::marker::PhantomData<T>,
}

impl<T: Ord + ByteSerializable + Send + Sync + 'static + std::fmt::Debug>
    TypedHttpStreamableIndex<T> for HttpIndexMeta<T>
{
    fn index_size(&self) -> u64 {
        self.size
    }
    #[cfg(feature = "http")]
    async fn http_stream_query_exact<C: http_range_client::AsyncHttpRangeClient>(
        &self,
        client: &mut http_range_client::AsyncBufferedHttpRangeClient<C>,
        index_offset: usize,
        key: &T,
    ) -> std::io::Result<Vec<ValueOffset>> {
        // HTTP implementation would go here, similar to the existing one but type-aware
        unimplemented!("Type-aware HTTP streaming not yet implemented")
    }

    #[cfg(feature = "http")]
    async fn http_stream_query_range<C: http_range_client::AsyncHttpRangeClient>(
        &self,
        client: &mut http_range_client::AsyncBufferedHttpRangeClient<C>,
        index_offset: usize,
        lower: Option<&T>,
        upper: Option<&T>,
    ) -> std::io::Result<Vec<ValueOffset>> {
        // HTTP implementation would go here, similar to the existing one but type-aware
        unimplemented!("Type-aware HTTP streaming not yet implemented")
    }
}

// /// A multi-index that can be streamed from a reader.
// #[derive(Default)]
// pub struct HttpStreamableMultiIndex {
//     /// A mapping from field names to their corresponding index metadata.
//     pub indices: HashMap<String, HttpIndexMeta<T>>,
//     /// A mapping from field names to their offsets in the file.
//     pub index_offsets: HashMap<String, u64>,
// }

// impl HttpStreamableMultiIndex {
//     /// Create a new, empty streamable multi-index.
//     pub fn new() -> Self {
//         Self {
//             indices: HashMap::new(),
//             index_offsets: HashMap::new(),
//         }
//     }

//     /// Add an index for a field.
//     pub fn add_index(&mut self, field_name: String, index: TypeErasedIndexMeta) {
//         self.indices.insert(field_name, index);
//     }

//     #[cfg(feature = "http")]
//     pub async fn http_stream_query<T: AsyncHttpRangeClient>(
//         &self,
//         client: &mut AsyncBufferedHttpRangeClient<T>,
//         query: &Query,
//         index_offset: usize,
//         feature_begin: usize,
//     ) -> std::io::Result<Vec<HttpSearchResultItem>> {
//         // TODO: Implement HTTP streaming query

//         unimplemented!("HTTP streaming query not yet implemented for TypeErasedIndexMeta");
//     }

//     #[cfg(feature = "http")]
//     pub async fn http_stream_query_batched<T: AsyncHttpRangeClient>(
//         &self,
//         client: &mut AsyncBufferedHttpRangeClient<T>,
//         query: &Query,
//         index_offset: usize,
//         feature_begin: usize,
//         batch_threshold: usize,
//     ) -> std::io::Result<Vec<HttpSearchResultItem>> {
//         // TODO: Implement batched HTTP streaming query
//         unimplemented!("Batched HTTP streaming query not yet implemented for TypeErasedIndexMeta");
//     }
// }
