use std::collections::HashMap;
use std::marker::PhantomData;

use crate::static_btree::error::{Error, Result};
use crate::static_btree::key::{Key, KeyType, Max, Min};
use crate::static_btree::query::types::{Operator, QueryCondition};
use crate::static_btree::stree::http::HttpSearchResultItem;
use crate::static_btree::stree::Stree;
use async_trait::async_trait;
use http_range_client::{AsyncBufferedHttpRangeClient, AsyncHttpRangeClient};

/// HTTP-based index for remote access
#[derive(Debug, Clone)]
pub struct HttpIndex<K: Key> {
    /// total number of items in the tree
    num_items: usize,
    /// branching factor of the B+tree
    branching_factor: u16,
    /// byte offset where the index begins
    index_begin: usize,
    /// byte offset where the feature data begins
    feature_begin: usize,
    /// threshold for combining HTTP requests to reduce roundtrips
    combine_request_threshold: usize,
    _marker: PhantomData<K>,
}

impl<K: Key> HttpIndex<K> {
    /// Create a new HTTP index descriptor with all necessary metadata
    pub fn new(
        num_items: usize,
        branching_factor: u16,
        index_begin: usize,
        feature_begin: usize,
        combine_request_threshold: usize,
    ) -> Self {
        Self {
            num_items,
            branching_factor,
            index_begin,
            feature_begin,
            combine_request_threshold,
            _marker: PhantomData,
        }
    }

    /// Find exact matches for a key via HTTP
    pub async fn find_exact<T: AsyncHttpRangeClient>(
        &self,
        client: &mut AsyncBufferedHttpRangeClient<T>,
        key: K,
    ) -> Result<Vec<HttpSearchResultItem>> {
        let items: Vec<HttpSearchResultItem> = Stree::http_stream_find_exact(
            client,
            self.index_begin,
            self.feature_begin,
            self.num_items,
            self.branching_factor,
            key.clone(),
            self.combine_request_threshold,
        )
        .await?;

        Ok(items)
    }

    /// Find all items in [start..end] via HTTP. At least one bound is required.
    pub async fn find_range<T: AsyncHttpRangeClient>(
        &self,
        client: &mut AsyncBufferedHttpRangeClient<T>,
        start: Option<K>,
        end: Option<K>,
    ) -> Result<Vec<HttpSearchResultItem>> {
        let (lower, upper) = match (start, end) {
            (Some(lo), Some(hi)) => (lo, hi),
            (Some(lo), None) => (lo, K::max_value()),
            (None, Some(hi)) => (K::min_value(), hi),
            (None, None) => {
                return Err(Error::QueryError(
                    "find_range requires at least one bound".to_string(),
                ));
            }
        };

        let items: Vec<HttpSearchResultItem> = Stree::http_stream_find_range(
            client,
            self.index_begin,
            self.feature_begin,
            self.num_items,
            self.branching_factor,
            lower.clone(),
            upper.clone(),
            self.combine_request_threshold,
        )
        .await?;

        Ok(items)
    }
}

/// Trait for HTTP indices with heterogeneous key support
#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
pub trait TypedHttpSearchIndex<T: AsyncHttpRangeClient + Send + Sync>:
    Send + Sync + std::fmt::Debug
{
    /// Execute a typed query condition over HTTP with a specific HTTP client
    async fn execute_query_condition(
        &self,
        client: &mut AsyncBufferedHttpRangeClient<T>,
        condition: &QueryCondition,
    ) -> Result<Vec<HttpSearchResultItem>>;
}

/// Wasm-specific version that doesn't require Send + Sync
#[cfg(target_arch = "wasm32")]
#[async_trait(?Send)]
pub trait TypedHttpSearchIndex<T: AsyncHttpRangeClient>: std::fmt::Debug {
    /// Execute a typed query condition over HTTP with a specific HTTP client
    async fn execute_query_condition(
        &self,
        client: &mut AsyncBufferedHttpRangeClient<T>,
        condition: &QueryCondition,
    ) -> Result<Vec<HttpSearchResultItem>>;
}

/// Implement the TypedHttpSearchIndex trait for each supported key type
macro_rules! impl_typed_http_search_index {
    ($key_type:ty, $enum_variant:path) => {
        #[cfg(not(target_arch = "wasm32"))]
        #[async_trait]
        impl<T: AsyncHttpRangeClient + Send + Sync> TypedHttpSearchIndex<T>
            for HttpIndex<$key_type>
        {
            async fn execute_query_condition(
                &self,
                client: &mut AsyncBufferedHttpRangeClient<T>,
                condition: &QueryCondition,
            ) -> Result<Vec<HttpSearchResultItem>> {
                // Extract the key value from the enum variant
                let key: $key_type = match &condition.key {
                    $enum_variant(val) => val.clone(),
                    _ => {
                        return Err(Error::QueryError(format!(
                            "key type mismatch: expected {}, got {:?}",
                            stringify!($key_type),
                            condition.key
                        )))
                    }
                };

                // Dispatch to exact or range methods
                let results = match condition.operator {
                    Operator::Eq => self.find_exact(client, key.clone()).await?,
                    Operator::Ne => {
                        let all = self
                            .find_range(
                                client,
                                Some(<$key_type>::min_value()),
                                Some(<$key_type>::max_value()),
                            )
                            .await?;
                        let eq = self.find_exact(client, key.clone()).await?;
                        all.into_iter().filter(|x| !eq.contains(x)).collect()
                    }
                    Operator::Gt => {
                        let mut v = self.find_range(client, Some(key.clone()), None).await?;
                        let eq = self.find_exact(client, key.clone()).await?;
                        v.retain(|x| !eq.contains(x));
                        v
                    }
                    Operator::Lt => {
                        let mut v = self.find_range(client, None, Some(key.clone())).await?;
                        let eq = self.find_exact(client, key.clone()).await?;
                        v.retain(|x| !eq.contains(x));
                        v
                    }
                    Operator::Ge => self.find_range(client, Some(key.clone()), None).await?,
                    Operator::Le => self.find_range(client, None, Some(key.clone())).await?,
                };
                Ok(results)
            }
        }

        #[cfg(target_arch = "wasm32")]
        #[async_trait(?Send)]
        impl<T: AsyncHttpRangeClient> TypedHttpSearchIndex<T> for HttpIndex<$key_type> {
            async fn execute_query_condition(
                &self,
                client: &mut AsyncBufferedHttpRangeClient<T>,
                condition: &QueryCondition,
            ) -> Result<Vec<HttpSearchResultItem>> {
                // Extract the key value from the enum variant
                let key: $key_type = match &condition.key {
                    $enum_variant(val) => val.clone(),
                    _ => {
                        return Err(Error::QueryError(format!(
                            "key type mismatch: expected {}, got {:?}",
                            stringify!($key_type),
                            condition.key
                        )))
                    }
                };

                // Dispatch to exact or range methods
                let results = match condition.operator {
                    Operator::Eq => self.find_exact(client, key.clone()).await?,
                    Operator::Ne => {
                        let all = self
                            .find_range(
                                client,
                                Some(<$key_type>::min_value()),
                                Some(<$key_type>::max_value()),
                            )
                            .await?;
                        let eq = self.find_exact(client, key.clone()).await?;
                        all.into_iter().filter(|x| !eq.contains(x)).collect()
                    }
                    Operator::Gt => {
                        let mut v = self.find_range(client, Some(key.clone()), None).await?;
                        let eq = self.find_exact(client, key.clone()).await?;
                        v.retain(|x| !eq.contains(x));
                        v
                    }
                    Operator::Lt => {
                        let mut v = self.find_range(client, None, Some(key.clone())).await?;
                        let eq = self.find_exact(client, key.clone()).await?;
                        v.retain(|x| !eq.contains(x));
                        v
                    }
                    Operator::Ge => self.find_range(client, Some(key.clone()), None).await?,
                    Operator::Le => self.find_range(client, None, Some(key.clone())).await?,
                };
                Ok(results)
            }
        }
    };
}

impl_typed_http_search_index!(i8, KeyType::Int8);
impl_typed_http_search_index!(u8, KeyType::UInt8);
impl_typed_http_search_index!(i16, KeyType::Int16);
impl_typed_http_search_index!(u16, KeyType::UInt16);
impl_typed_http_search_index!(i32, KeyType::Int32);
impl_typed_http_search_index!(i64, KeyType::Int64);
impl_typed_http_search_index!(u32, KeyType::UInt32);
impl_typed_http_search_index!(u64, KeyType::UInt64);
impl_typed_http_search_index!(ordered_float::OrderedFloat<f32>, KeyType::Float32);
impl_typed_http_search_index!(ordered_float::OrderedFloat<f64>, KeyType::Float64);
impl_typed_http_search_index!(bool, KeyType::Bool);
impl_typed_http_search_index!(chrono::DateTime<chrono::Utc>, KeyType::DateTime);
impl_typed_http_search_index!(
    crate::static_btree::key::FixedStringKey<20>,
    KeyType::StringKey20
);
impl_typed_http_search_index!(
    crate::static_btree::key::FixedStringKey<50>,
    KeyType::StringKey50
);
impl_typed_http_search_index!(
    crate::static_btree::key::FixedStringKey<100>,
    KeyType::StringKey100
);

/// Container for multiple HTTP indices keyed by field name
#[derive(Debug)]
#[cfg(not(target_arch = "wasm32"))]
pub struct HttpMultiIndex<T: AsyncHttpRangeClient + Send + Sync> {
    indices: HashMap<String, Box<dyn TypedHttpSearchIndex<T>>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl<T: AsyncHttpRangeClient + Send + Sync> HttpMultiIndex<T> {
    /// Create a new empty HTTP multi-index
    pub fn new() -> Self {
        Self {
            indices: HashMap::new(),
        }
    }

    /// Add an index for any supported key type
    pub fn add_index<K: Key + 'static>(&mut self, field: String, index: HttpIndex<K>)
    where
        HttpIndex<K>: TypedHttpSearchIndex<T> + 'static,
    {
        self.indices.insert(field, Box::new(index));
    }

    /// Execute a multi-condition query by AND-ing all conditions
    pub async fn query(
        &self,
        client: &mut AsyncBufferedHttpRangeClient<T>,
        conditions: &[QueryCondition],
    ) -> Result<Vec<HttpSearchResultItem>> {
        if conditions.is_empty() {
            return Err(Error::QueryError("query cannot be empty".to_string()));
        }
        let mut result_sets = Vec::with_capacity(conditions.len());
        for cond in conditions {
            let idx = self.indices.get(&cond.field).ok_or_else(|| {
                Error::QueryError(format!("no index found for field '{}'", cond.field))
            })?;
            let items = idx.execute_query_condition(client, cond).await?;
            result_sets.push(items);
            if result_sets.is_empty() {
                // no results found for this condition, return early so we don't waste time intersecting empty sets
                return Ok(vec![]);
            }
        }
        // intersect all sets
        let mut iter = result_sets.into_iter();
        let mut intersection = iter.next().unwrap_or_default();
        for set in iter {
            intersection.retain(|x| set.contains(x));
        }
        Ok(intersection)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<T: AsyncHttpRangeClient + Send + Sync> Default for HttpMultiIndex<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Container for multiple HTTP indices keyed by field name (WASM version)
#[derive(Debug)]
#[cfg(target_arch = "wasm32")]
pub struct HttpMultiIndex<T: AsyncHttpRangeClient> {
    indices: HashMap<String, Box<dyn TypedHttpSearchIndex<T>>>,
}

#[cfg(target_arch = "wasm32")]
impl<T: AsyncHttpRangeClient> HttpMultiIndex<T> {
    /// Create a new empty HTTP multi-index
    pub fn new() -> Self {
        Self {
            indices: HashMap::new(),
        }
    }

    /// Add an index for any supported key type
    pub fn add_index<K: Key + 'static>(&mut self, field: String, index: HttpIndex<K>)
    where
        HttpIndex<K>: TypedHttpSearchIndex<T> + 'static,
    {
        self.indices.insert(field, Box::new(index));
    }
    /// Execute a multi-condition query by AND-ing all conditions
    pub async fn query(
        &self,
        client: &mut AsyncBufferedHttpRangeClient<T>,
        conditions: &[QueryCondition],
    ) -> Result<Vec<HttpSearchResultItem>> {
        if conditions.is_empty() {
            return Err(Error::QueryError("query cannot be empty".to_string()));
        }
        let mut result_sets = Vec::with_capacity(conditions.len());

        for cond in conditions {
            // print the field name of condition and indices names

            let idx = self.indices.get(&cond.field).ok_or_else(|| {
                Error::QueryError(format!("no index found for field '{}'", cond.field))
            })?;
            let items = idx.execute_query_condition(client, cond).await?;
            result_sets.push(items);
            if result_sets.is_empty() {
                // no results found for this condition, return early so we don't waste time intersecting empty sets
                return Ok(vec![]);
            }
        }
        // intersect all sets
        let mut iter = result_sets.into_iter();
        let mut intersection = iter.next().unwrap_or_default();
        for set in iter {
            intersection.retain(|x| set.contains(x));
        }
        Ok(intersection)
    }
}

#[cfg(target_arch = "wasm32")]
impl<T: AsyncHttpRangeClient> Default for HttpMultiIndex<T> {
    fn default() -> Self {
        Self::new()
    }
}
