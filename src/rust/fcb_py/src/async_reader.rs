use crate::error::{fcb_error_to_py_err, FcbError};
use crate::query::{AttrFilter, BBox};
use crate::type_conversion::python_value_to_keytype;
use crate::types::{CityJSON, FileInfo};
use crate::utils::{header_to_cityjson, is_url};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use pyo3_asyncio::tokio::future_into_py;

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
    pub fn open<'p>(&self, py: Python<'p>) -> PyResult<&'p PyAny> {
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
        Python::with_gil(|py| {
            header_to_cityjson(py, &self.reader.header())
        })
    }

    /// Query features by bounding box (async)
    pub fn query_bbox<'p>(
        &self,
        py: Python<'p>,
        min_x: f64,
        min_y: f64,
        max_x: f64,
        max_y: f64,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> PyResult<&'p PyAny> {
        let bbox = BBox::new(min_x, min_y, max_x, max_y);
        self.query_spatial(py, bbox, limit, offset)
    }

    /// Query features by spatial bounding box (async)
    pub fn query_spatial<'p>(
        &self,
        py: Python<'p>,
        bbox: BBox,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> PyResult<&'p PyAny> {
        let query: SpatialQuery = bbox.into();
        let url = self.url.clone();

        future_into_py(py, async move {
            let reader = HttpFcbReader::open(&url)
                .await
                .map_err(fcb_error_to_py_err)?;

            let async_iter = reader
                .select_query_paged(query, limit, offset)
                .await
                .map_err(fcb_error_to_py_err)?;

            let total_count = async_iter.features_count().unwrap_or(0) as u64;

            Ok(Python::with_gil(|py| {
                Py::new(
                    py,
                    AsyncFeatureIterator {
                        inner: async_iter,
                        total_count,
                    },
                )
                .unwrap()
            }))
        })
    }

    /// Query features by attribute filter (async)
    pub fn query_attr<'p>(&self, py: Python<'p>, filters: Vec<AttrFilter>) -> PyResult<&'p PyAny> {
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

        future_into_py(py, async move {
            let reader = HttpFcbReader::open(&url)
                .await
                .map_err(fcb_error_to_py_err)?;

            let async_iter = reader
                .select_attr_query(&attr_query)
                .await
                .map_err(fcb_error_to_py_err)?;

            let total_count = async_iter.features_count().unwrap_or(0) as u64;

            Ok(Python::with_gil(|py| {
                Py::new(
                    py,
                    AsyncFeatureIterator {
                        inner: async_iter,
                        total_count,
                    },
                )
                .unwrap()
            }))
        })
    }

    /// Get all features as an iterator (async)
    pub fn select_all<'p>(&self, py: Python<'p>) -> PyResult<&'p PyAny> {
        let url = self.url.clone();

        future_into_py(py, async move {
            let reader = HttpFcbReader::open(&url)
                .await
                .map_err(fcb_error_to_py_err)?;

            let async_iter = reader.select_all().await.map_err(fcb_error_to_py_err)?;

            let total_count = async_iter.features_count().unwrap_or(0) as u64;

            Ok(Python::with_gil(|py| {
                Py::new(
                    py,
                    AsyncFeatureIterator {
                        inner: async_iter,
                        total_count,
                    },
                )
                .unwrap()
            }))
        })
    }

    fn __repr__(&self) -> String {
        format!("AsyncReaderOpened('{}')", self.url)
    }
}

/// Async iterator that wraps AsyncFeatureIter from fcb_core
#[cfg(feature = "http")]
#[pyclass]
pub struct AsyncFeatureIterator {
    // Use a boxed AsyncFeatureIter to handle the generic type parameter
    inner: AsyncFeatureIter<reqwest::Client>,
    total_count: u64,
}

#[cfg(feature = "http")]
#[pymethods]
impl AsyncFeatureIterator {
    /// Total number of features that will be returned
    pub fn total_count(&self) -> u64 {
        self.total_count
    }

    /// Number of features remaining
    pub fn features_count(&self) -> Option<usize> {
        self.inner.features_count()
    }

    /// Get next feature (async) - uses a different approach to avoid Send issues
    // TODO: fix this
    pub fn next<'p>(_slf: PyRefMut<Self>, py: Python<'p>) -> PyResult<&'p PyAny> {
        // For now, return None to indicate end of iteration
        // The PyRefMut cannot be moved across await boundaries due to Send constraints
        future_into_py(py, async move { Ok(None::<crate::types::Feature>) })
    }

    /// Collect all remaining features into a list (async)
    // TODO: fix this
    pub fn collect<'p>(_slf: PyRefMut<Self>, py: Python<'p>) -> PyResult<&'p PyAny> {
        // Return empty list for now due to Send constraints with PyRefMut
        future_into_py(py, async move { Ok(Vec::<crate::types::Feature>::new()) })
    }

    fn __repr__(&self) -> String {
        format!("AsyncFeatureIterator(total_count={})", self.total_count)
    }
}
