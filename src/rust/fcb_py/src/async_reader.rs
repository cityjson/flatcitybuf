use crate::error::{fcb_error_to_py_err, FcbError};
use crate::query::{AttrFilter, BBox};
use crate::type_conversion::python_value_to_keytype;
use crate::types::FileInfo;
use crate::utils::is_url;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use pyo3_asyncio::tokio::future_into_py;

#[cfg(feature = "http")]
use fcb_core::{http_reader::HttpFcbReader, packed_rtree::Query as SpatialQuery, AttrQuery};

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

    /// Query features by bounding box (async)
    pub fn query_bbox<'p>(
        &mut self,
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
        &mut self,
        py: Python<'p>,
        bbox: BBox,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> PyResult<&'p PyAny> {
        let query: SpatialQuery = bbox.into();

        // We'll need to work around the clone issue by using a different approach
        // For now, let's return an error indicating this needs more work
        future_into_py(py, async move {
            let mut feature_iter = self
                .reader
                .select_query_paged(query, limit, offset)
                .await
                .map_err(fcb_error_to_py_err)?;

            let mut features = Vec::new();
            while let Ok(Some(iter)) = feature_iter.next() {
                let fcb_feature = iter.cur_feature();
                let py_feature = cityfeature_to_python(py, fcb_feature)?;
                features.push(py_feature);
            }
            Ok(features)
        })
    }

    /// Query features by attribute filter (async)
    pub fn query_attr<'p>(
        &mut self,
        py: Python<'p>,
        filters: Vec<AttrFilter>,
    ) -> PyResult<&'p PyAny> {
        let header = self.reader.header();

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

        let _attr_query: AttrQuery = query_conditions;

        future_into_py(py, async move {
            Err::<Vec<()>, _>(PyErr::new::<FcbError, _>(
                "Attribute queries not yet implemented for async reader - clone issues need to be resolved"
            ))
        })
    }

    fn __repr__(&self) -> String {
        format!("AsyncReaderOpened('{}')", self.url)
    }
}

// Stub implementations when HTTP feature is not enabled
#[cfg(not(feature = "http"))]
#[pyclass]
pub struct AsyncReader;

#[cfg(not(feature = "http"))]
#[pymethods]
impl AsyncReader {
    #[new]
    pub fn new(_url: String) -> PyResult<Self> {
        Err(PyErr::new::<FcbError, _>(
            "HTTP support not compiled. Rebuild with 'http' feature.",
        ))
    }
}

#[cfg(not(feature = "http"))]
#[pyclass]
pub struct AsyncReaderOpened;

// Remove the async iterator for now due to clone complexity
#[cfg(not(feature = "http"))]
#[pyclass]
pub struct AsyncReaderIterator;
