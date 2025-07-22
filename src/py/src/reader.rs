use crate::error::{fcb_error_to_py_err, io_error_to_py_err};
use crate::query::{AttrFilter, BBox};
use crate::types::{Feature, FileInfo};
use crate::utils::{cityfeature_to_python, is_url};
use fcb_core::{
    FcbReader, FeatureIter, Seekable, NotSeekable,
    packed_rtree::Query as SpatialQuery,
};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use std::fs::File;
use std::io::BufReader;

#[cfg(feature = "http")]
use fcb_core::http_reader::HttpFcbReader;

/// Main reader for FlatCityBuf files (both local and HTTP)
#[pyclass]
pub struct Reader {
    inner: ReaderInner,
    path: String,
}

enum ReaderInner {
    File(FcbReader<BufReader<File>>),
    #[cfg(feature = "http")]
    Http(HttpFcbReader<reqwest::Client>),
}

#[pymethods]
impl Reader {
    /// Create a new reader for a local file or HTTP URL
    #[new]
    pub fn new(path: String) -> PyResult<Self> {
        let inner = if is_url(&path) {
            #[cfg(feature = "http")]
            {
                // For HTTP, we need to use async, but PyO3 doesn't handle async well in constructors
                // We'll defer the actual connection until first use
                return Err(PyErr::new::<crate::error::FcbError, _>(
                    "HTTP URLs not supported in sync Reader. Use AsyncReader instead."
                ));
            }
            #[cfg(not(feature = "http"))]
            {
                return Err(PyErr::new::<crate::error::FcbError, _>(
                    "HTTP support not compiled. Rebuild with 'http' feature."
                ));
            }
        } else {
            // Local file
            let file = File::open(&path).map_err(io_error_to_py_err)?;
            let buf_reader = BufReader::new(file);
            let reader = FcbReader::open(buf_reader).map_err(fcb_error_to_py_err)?;
            ReaderInner::File(reader)
        };

        Ok(Self {
            inner,
            path,
        })
    }

    /// Get file information and metadata
    pub fn info(&self) -> PyResult<FileInfo> {
        match &self.inner {
            ReaderInner::File(reader) => {
                let header = reader.header();
                let feature_count = header.features_count();
                
                let columns = Python::with_gil(|py| -> PyResult<PyObject> {
                    let py_list = PyList::empty(py);
                    if let Some(cols) = header.columns() {
                        for col in cols.iter() {
                            let col_dict = PyDict::new(py);
                            col_dict.set_item("name", col.name().unwrap_or("unknown"))?;
                            col_dict.set_item("index", col.index())?;
                            // TODO: Add more column metadata
                            py_list.append(col_dict)?;
                        }
                    }
                    Ok(py_list.to_object(py))
                })?;

                // TODO: Extract CRS and bbox from header
                Ok(FileInfo::new(
                    feature_count,
                    columns,
                    None, // CRS
                    None, // bbox
                ))
            },
            #[cfg(feature = "http")]
            ReaderInner::Http(_) => {
                Err(PyErr::new::<crate::error::FcbError, _>(
                    "Info not yet implemented for HTTP reader"
                ))
            }
        }
    }

    /// Query features by bounding box
    pub fn query_bbox(&self, min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> PyResult<Vec<Feature>> {
        let bbox = BBox::new(min_x, min_y, max_x, max_y);
        self.query_spatial(bbox)
    }

    /// Query features by spatial bounding box
    pub fn query_spatial(&self, bbox: BBox) -> PyResult<Vec<Feature>> {
        match &self.inner {
            ReaderInner::File(reader) => {
                let query: SpatialQuery = bbox.into();
                
                // We need to clone the reader to move it into the iterator
                // This is a limitation we'll need to work around
                let file = File::open(&self.path).map_err(io_error_to_py_err)?;
                let buf_reader = BufReader::new(file);
                let reader = FcbReader::open(buf_reader).map_err(fcb_error_to_py_err)?;
                
                let feature_iter = reader.select_query(query).map_err(fcb_error_to_py_err)?;
                
                Python::with_gil(|py| {
                    let mut features = Vec::new();
                    for result in feature_iter {
                        let iter = result.map_err(fcb_error_to_py_err)?;
                        let fcb_feature = iter.cur_feature();
                        let py_feature = cityfeature_to_python(py, fcb_feature)?;
                        features.push(py_feature);
                    }
                    Ok(features)
                })
            },
            #[cfg(feature = "http")]
            ReaderInner::Http(_) => {
                Err(PyErr::new::<crate::error::FcbError, _>(
                    "Spatial query not yet implemented for HTTP reader"
                ))
            }
        }
    }

    /// Query features by attribute filter
    pub fn query_attr(&self, field: String, operator: &str, value: PyObject) -> PyResult<Vec<Feature>> {
        let op = match operator {
            "==" | "=" => crate::query::Operator::Eq,
            "!=" => crate::query::Operator::Ne,
            ">" => crate::query::Operator::Gt,
            ">=" => crate::query::Operator::Ge,
            "<" => crate::query::Operator::Lt,
            "<=" => crate::query::Operator::Le,
            _ => return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                format!("Unsupported operator: {}", operator)
            )),
        };

        let filter = AttrFilter::new(field, op, value);
        self.query_attribute(&filter)
    }

    /// Query features by attribute filter object
    pub fn query_attribute(&self, filter: &AttrFilter) -> PyResult<Vec<Feature>> {
        match &self.inner {
            ReaderInner::File(_reader) => {
                // TODO: Implement attribute queries
                Err(PyErr::new::<crate::error::FcbError, _>(
                    "Attribute queries not yet implemented"
                ))
            },
            #[cfg(feature = "http")]
            ReaderInner::Http(_) => {
                Err(PyErr::new::<crate::error::FcbError, _>(
                    "Attribute queries not yet implemented for HTTP reader"
                ))
            }
        }
    }

    /// Get all features as an iterator
    pub fn __iter__(&self) -> PyResult<ReaderIterator> {
        match &self.inner {
            ReaderInner::File(_reader) => {
                // Create new reader for iteration
                let file = File::open(&self.path).map_err(io_error_to_py_err)?;
                let buf_reader = BufReader::new(file);
                let reader = FcbReader::open(buf_reader).map_err(fcb_error_to_py_err)?;
                let feature_iter = reader.select_all().map_err(fcb_error_to_py_err)?;
                
                Ok(ReaderIterator {
                    inner: IteratorInner::File(feature_iter),
                })
            },
            #[cfg(feature = "http")]
            ReaderInner::Http(_) => {
                Err(PyErr::new::<crate::error::FcbError, _>(
                    "Iterator not yet implemented for HTTP reader"
                ))
            }
        }
    }

    fn __repr__(&self) -> String {
        format!("Reader('{}')", self.path)
    }
}

/// Iterator for features
#[pyclass]
pub struct ReaderIterator {
    inner: IteratorInner,
}

enum IteratorInner {
    File(FeatureIter<BufReader<File>, Seekable>),
    #[cfg(feature = "http")]
    Http(Box<dyn Iterator<Item = Result<Feature, FcbError>> + Send>),
}

#[pymethods]
impl ReaderIterator {
    fn __iter__(slf: PyRef<Self>) -> PyRef<Self> {
        slf
    }

    fn __next__(&mut self) -> PyResult<Option<Feature>> {
        match &mut self.inner {
            IteratorInner::File(iter) => {
                match iter.next() {
                    Some(Ok(feature_iter)) => {
                        Python::with_gil(|py| {
                            let fcb_feature = feature_iter.cur_feature();
                            let py_feature = cityfeature_to_python(py, fcb_feature)?;
                            Ok(Some(py_feature))
                        })
                    },
                    Some(Err(e)) => Err(fcb_error_to_py_err(e)),
                    None => Ok(None),
                }
            },
            #[cfg(feature = "http")]
            IteratorInner::Http(iter) => {
                match iter.next() {
                    Some(Ok(feature)) => Ok(Some(feature)),
                    Some(Err(e)) => Err(e.into()),
                    None => Ok(None),
                }
            }
        }
    }
}

/// Async reader for HTTP URLs
#[cfg(feature = "http")]
#[pyclass]
pub struct AsyncReader {
    url: String,
}

#[cfg(feature = "http")]
#[pymethods]
impl AsyncReader {
    #[new]
    pub fn new(url: String) -> Self {
        Self { url }
    }

    /// Get file info (placeholder for now)
    pub fn info(&self) -> PyResult<FileInfo> {
        // For now, return a placeholder
        // TODO: Implement async HTTP info retrieval
        Python::with_gil(|py| {
            let columns = PyList::empty(py);
            Ok(FileInfo::new(
                0,
                columns.to_object(py),
                None,
                None,
            ))
        })
    }

    fn __repr__(&self) -> String {
        format!("AsyncReader('{}')", self.url)
    }
}