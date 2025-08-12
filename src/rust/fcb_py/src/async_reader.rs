use crate::error::{fcb_error_to_py_err, FcbError};
use crate::query::{AttrFilter, BBox};
use crate::type_conversion::python_value_to_keytype;
use crate::types::{CityJSON, FileInfo};
use crate::utils::{cityfeature_to_python, header_to_cityjson, is_url};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use pyo3_async_runtimes::tokio::future_into_py;
use std::sync::Arc;
use tokio::sync::Mutex;

#[cfg(feature = "http")]
use fcb_core::{
    http_reader::{AsyncFeatureIter, HttpFcbReader},
    packed_rtree::Query as SpatialQuery,
    AttrQuery,
};

/// Asynchronous reader for HTTP-based FlatCityBuf files
#[cfg(feature = "http")]
#[pyclass]
pub struct AsyncReader {
    url: String,
}

#[cfg(feature = "http")]
#[pymethods]
impl AsyncReader {
    /// Create a new async reader for an HTTP URL
    #[new]
    pub fn new(url: String) -> PyResult<Self> {
        if !is_url(&url) {
            return Err(PyErr::new::<FcbError, _>(
                "AsyncReader only supports HTTP/HTTPS URLs. Use Reader for local files.",
            ));
        }
        Ok(Self { url })
    }

    /// Open and initialize the reader (async)
    pub fn open<'p>(&self, py: Python<'p>) -> PyResult<Bound<'p, PyAny>> {
        let url = self.url.clone();
        future_into_py(py, async move {
            let reader = HttpFcbReader::open(&url)
                .await
                .map_err(fcb_error_to_py_err)?;

            Ok(Python::with_gil(|py| {
                Py::new(py, AsyncReaderOpened { reader, url }).unwrap()
            }))
        })
    }

    fn __repr__(&self) -> String {
        format!("AsyncReader('{}')", self.url)
    }
}

/// Opened async reader with active connection
#[cfg(feature = "http")]
#[pyclass]
pub struct AsyncReaderOpened {
    reader: HttpFcbReader<reqwest::Client>,
    url: String,
}

#[cfg(feature = "http")]
#[pymethods]
impl AsyncReaderOpened {
    /// Get file information and metadata
    pub fn info(&self) -> PyResult<FileInfo> {
        let header = self.reader.header();
        let feature_count = header.features_count();

        let columns = Python::with_gil(|py| -> PyResult<PyObject> {
            let py_list = PyList::empty(py);
            if let Some(cols) = header.columns() {
                for col in cols.iter() {
                    let col_dict = PyDict::new(py);
                    col_dict.set_item("name", col.name())?;
                    col_dict.set_item("index", col.index())?;
                    col_dict.set_item("type", col.type_().variant_name())?;
                    col_dict.set_item("nullable", col.nullable())?;
                    col_dict.set_item("unique", col.unique())?;
                    col_dict.set_item("primary_key", col.primary_key())?;
                    col_dict.set_item("metadata", col.metadata())?;
                    col_dict.set_item("precision", col.precision())?;
                    col_dict.set_item("scale", col.scale())?;
                    py_list.append(col_dict)?;
                }
            }
            Ok(py_list.to_object(py))
        })?;

        let bbox = header.geographical_extent().map(|bbox| {
            (
                bbox.min().x(),
                bbox.min().y(),
                bbox.max().x(),
                bbox.max().y(),
            )
        });

        let crs = header
            .reference_system()
            .map(|crs| format!("EPSG:{}", crs.code_string().unwrap_or_default()));

        Ok(FileInfo::new(feature_count, columns, crs, bbox))
    }

    /// Get CityJSON header information with metadata and transform
    pub fn cityjson_header(&self) -> PyResult<CityJSON> {
        Python::with_gil(|py| header_to_cityjson(py, &self.reader.header()))
    }

    /// Query features by bounding box (returns an async iterator)
    #[pyo3(signature = (min_x, min_y, max_x, max_y, limit=None, offset=None))]
    pub fn query_bbox(
        &self,
        min_x: f64,
        min_y: f64,
        max_x: f64,
        max_y: f64,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> PyResult<AsyncFeatureIterator> {
        let bbox = BBox::new(min_x, min_y, max_x, max_y);
        self.query_spatial(bbox, limit, offset)
    }

    /// Query features by spatial bounding box (returns an async iterator)
    #[pyo3(signature = (bbox, limit=None, offset=None))]
    pub fn query_spatial(
        &self,
        bbox: BBox,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> PyResult<AsyncFeatureIterator> {
        let query: SpatialQuery = bbox.into();
        let url = self.url.clone();

        Ok(AsyncFeatureIterator::new(
            url,
            AsyncIteratorType::Spatial {
                query,
                limit,
                offset,
            },
        ))
    }

    /// Query features by attribute filter (returns an async iterator)
    pub fn query_attr(&self, filters: Vec<AttrFilter>) -> PyResult<AsyncFeatureIterator> {
        let header = self.reader.header();
        let url = self.url.clone();

        // Convert Python attribute filters to fcb_core query
        let mut query_conditions = Vec::new();
        for filter in filters {
            Python::with_gil(|py| {
                let key_value = python_value_to_keytype(py, &filter.value, &filter.field, &header)?;
                query_conditions.push((
                    filter.field.clone(),
                    filter.operator.clone().into(),
                    key_value,
                ));
                Ok::<(), PyErr>(())
            })?;
        }

        let attr_query: AttrQuery = query_conditions;

        Ok(AsyncFeatureIterator::new(
            url,
            AsyncIteratorType::Attribute { attr_query },
        ))
    }

    /// Get all features as an iterator (returns an async iterator)
    pub fn select_all(&self) -> PyResult<AsyncFeatureIterator> {
        let url = self.url.clone();

        Ok(AsyncFeatureIterator::new(url, AsyncIteratorType::All))
    }

    /// Python iterator protocol - returns self
    pub fn __iter__(slf: PyRef<Self>) -> PyRef<Self> {
        slf
    }

    fn __repr__(&self) -> String {
        format!("AsyncReaderOpened('{}')", self.url)
    }
}

/// Enumeration of different async iterator types
#[cfg(feature = "http")]
#[derive(Clone)]
pub enum AsyncIteratorType {
    All,
    Spatial {
        query: SpatialQuery,
        limit: Option<usize>,
        offset: Option<usize>,
    },
    Attribute {
        attr_query: AttrQuery,
    },
}

/// State holder for the async feature iterator
#[cfg(feature = "http")]
struct AsyncIteratorState {
    iter: Option<AsyncFeatureIter<reqwest::Client>>,
}

/// Async iterator that maintains HTTP client and iterator state
#[cfg(feature = "http")]
#[pyclass]
pub struct AsyncFeatureIterator {
    url: String,
    iterator_type: AsyncIteratorType,
    state: Arc<Mutex<AsyncIteratorState>>,
}

#[cfg(feature = "http")]
impl AsyncFeatureIterator {
    pub fn new(url: String, iterator_type: AsyncIteratorType) -> Self {
        Self {
            url,
            iterator_type,
            state: Arc::new(Mutex::new(AsyncIteratorState { iter: None })),
        }
    }
}

#[cfg(feature = "http")]
#[pymethods]
impl AsyncFeatureIterator {
    /// Get next feature (async)
    pub fn next<'p>(&mut self, py: Python<'p>) -> PyResult<Bound<'p, PyAny>> {
        let url = self.url.clone();
        let iterator_type = self.iterator_type.clone();
        let state = Arc::clone(&self.state);

        future_into_py(py, async move {
            let mut state_guard = state.lock().await;

            // Initialize the iterator if not already done
            if state_guard.iter.is_none() {
                let reader = HttpFcbReader::open(&url)
                    .await
                    .map_err(fcb_error_to_py_err)?;

                let async_iter = match iterator_type {
                    AsyncIteratorType::All => {
                        reader.select_all().await.map_err(fcb_error_to_py_err)?
                    }
                    AsyncIteratorType::Spatial {
                        query,
                        limit,
                        offset,
                    } => reader
                        .select_query_paged(query, limit, offset)
                        .await
                        .map_err(fcb_error_to_py_err)?,
                    AsyncIteratorType::Attribute { attr_query } => reader
                        .select_attr_query(&attr_query)
                        .await
                        .map_err(fcb_error_to_py_err)?,
                };

                state_guard.iter = Some(async_iter);
            }

            // Get the next feature from the existing iterator
            if let Some(ref mut iter) = state_guard.iter {
                match iter.next().await {
                    Ok(Some(buffer)) => Python::with_gil(|py| {
                        let feature = cityfeature_to_python(py, buffer)?;
                        Ok(Some(feature))
                    }),
                    Ok(None) => Ok(None),
                    Err(e) => Err(fcb_error_to_py_err(e)),
                }
            } else {
                Ok(None)
            }
        })
    }

    /// Collect all remaining features into a list (async)
    pub fn collect<'p>(&mut self, py: Python<'p>) -> PyResult<Bound<'p, PyAny>> {
        let url = self.url.clone();
        let iterator_type = self.iterator_type.clone();
        let state = Arc::clone(&self.state);

        future_into_py(py, async move {
            let mut state_guard = state.lock().await;

            // Initialize the iterator if not already done
            if state_guard.iter.is_none() {
                let reader = HttpFcbReader::open(&url)
                    .await
                    .map_err(fcb_error_to_py_err)?;

                let async_iter = match iterator_type {
                    AsyncIteratorType::All => {
                        reader.select_all().await.map_err(fcb_error_to_py_err)?
                    }
                    AsyncIteratorType::Spatial {
                        query,
                        limit,
                        offset,
                    } => reader
                        .select_query_paged(query, limit, offset)
                        .await
                        .map_err(fcb_error_to_py_err)?,
                    AsyncIteratorType::Attribute { attr_query } => reader
                        .select_attr_query(&attr_query)
                        .await
                        .map_err(fcb_error_to_py_err)?,
                };

                state_guard.iter = Some(async_iter);
            }

            let mut features = Vec::new();
            if let Some(ref mut iter) = state_guard.iter {
                while let Ok(Some(buffer)) = iter.next().await {
                    Python::with_gil(|py| {
                        let feature = cityfeature_to_python(py, buffer)?;
                        features.push(feature);
                        Ok::<(), PyErr>(())
                    })?;
                }
            }
            Ok(features)
        })
    }

    /// Get estimated feature count (may not be exact)
    pub fn features_count<'p>(&self, py: Python<'p>) -> PyResult<Bound<'p, PyAny>> {
        let state = Arc::clone(&self.state);

        future_into_py(py, async move {
            let state_guard = state.lock().await;
            if let Some(ref iter) = state_guard.iter {
                Ok(iter.features_count())
            } else {
                Ok(None)
            }
        })
    }

    /// Python async iterator protocol
    fn __aiter__(slf: PyRef<Self>) -> PyRef<Self> {
        slf
    }

    /// Python async iterator next
    pub fn __anext__<'p>(&mut self, py: Python<'p>) -> PyResult<Option<Bound<'p, PyAny>>> {
        // Return the awaitable for the next feature
        Ok(Some(self.next(py)?))
    }

    fn __repr__(&self) -> String {
        format!("AsyncFeatureIterator(url='{}')", self.url)
    }
}
