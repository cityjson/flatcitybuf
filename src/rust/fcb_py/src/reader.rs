use crate::error::{fcb_error_to_py_err, io_error_to_py_err, FcbError};
use crate::query::{AttrFilter, BBox};
use crate::types::{Feature, FileInfo};
use crate::utils::{cityfeature_to_python, is_url};
use fcb_core::{
    packed_rtree::Query as SpatialQuery,
    reader_trait::{NotSeekable, Seekable},
    FcbReader, FeatureIter,
};
use fcb_core::{AsyncFeatureIter, AttrQuery};
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
                let reader = HttpFcbReader::open(&path).await?; //TODO: This should be async
                ReaderInner::Http(reader)
            }
            #[cfg(not(feature = "http"))]
            {
                return Err(PyErr::new::<crate::error::FcbError, _>(
                    "HTTP support not compiled. Rebuild with 'http' feature.",
                ));
            }
        } else {
            // Local file
            let file = File::open(&path).map_err(io_error_to_py_err)?;
            let buf_reader = BufReader::new(file);
            let reader = FcbReader::open(buf_reader).map_err(fcb_error_to_py_err)?;
            ReaderInner::File(reader)
        };

        Ok(Self { inner, path })
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
            #[cfg(feature = "http")]
            ReaderInner::Http(_) => Err(PyErr::new::<crate::error::FcbError, _>(
                "Info not yet implemented for HTTP reader",
            )),
        }
    }

    /// Query features by bounding box
    pub fn query_bbox(
        &self,
        min_x: f64,
        min_y: f64,
        max_x: f64,
        max_y: f64,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> PyResult<Vec<Feature>> {
        let bbox = BBox::new(min_x, min_y, max_x, max_y);
        self.query_spatial(bbox, limit, offset)
    }

    /// Query features by spatial bounding box
    pub fn query_spatial(
        &self,
        bbox: BBox,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> PyResult<Vec<Feature>> {
        match &self.inner {
            ReaderInner::File(_reader) => {
                let query: SpatialQuery = bbox.into();

                // NOTE: We need to re-instantiate the reader to move it into the iterator
                // This is a limitation we'll need to work around.
                // This is because the Python object is shared, and we cannot move `self`.
                let file = File::open(&self.path).map_err(io_error_to_py_err)?;
                let buf_reader = BufReader::new(file);
                let reader = FcbReader::open(buf_reader).map_err(fcb_error_to_py_err)?;

                let mut feature_iter = reader
                    .select_query(query, limit, offset)
                    .map_err(fcb_error_to_py_err)?;

                Python::with_gil(|py| {
                    let mut features = Vec::new();
                    while let Ok(Some(iter)) = feature_iter.next() {
                        let fcb_feature = iter.cur_feature();
                        let py_feature = cityfeature_to_python(py, fcb_feature)?;
                        features.push(py_feature);
                    }
                    Ok(features)
                })
            }

            #[cfg(feature = "http")]
            #[tokio::main] //TODO: Fix async
            ReaderInner::Http(reader) => {
                let query: SpatialQuery = bbox.into();

                // TODO: Fix async
                let mut feature_iter = reader
                    .select_query_paged(query, limit, offset)
                    .await
                    .map_err(fcb_error_to_py_err)?;

                Python::with_gil(|py| {
                    let mut features = Vec::new();
                    while let Ok(Some(iter)) = feature_iter.next() {
                        let fcb_feature = iter.cur_feature();
                        let py_feature = cityfeature_to_python(py, fcb_feature)?;
                        features.push(py_feature);
                    }
                    Ok(features)
                })
            }
        }
    }

    /// Query features by attribute filter
    pub fn query_attr(&self, filters: Vec<AttrFilter>) -> PyResult<Vec<Feature>> {
        //TODO: This should be iterator of Python rather than Vec<Feature> as it is not efficient to load all features into memory. Todo do that, we need to implement `__iter__` method for `Reader` and `ReaderIterator` classes.
        match &self.inner {
            ReaderInner::File(reader) => {
                let query: AttrQuery = filters.into_iter().map(|f| f.into()).collect();
                let file = File::open(&self.path).map_err(io_error_to_py_err)?;
                let attr_index_info = reader.header().attribute_index();
                //TODO: implement type casting by considering the key type of the index. For example, we want to convert integer into float when the index is a float. We can refer to `parse_and_convert_value` in fcb_api/src/filter_parser.rs as a reference.

                let buf_reader = BufReader::new(file);
                let reader = FcbReader::open(buf_reader).map_err(fcb_error_to_py_err)?;

                let mut feature_iter = reader
                    .select_attr_query(query)
                    .map_err(fcb_error_to_py_err)?;

                Python::with_gil(|py| {
                    let mut features = Vec::new();
                    while let Ok(Some(iter)) = feature_iter.next() {
                        let fcb_feature = iter.cur_feature();
                        let py_feature = cityfeature_to_python(py, fcb_feature)?;
                        features.push(py_feature);
                    }
                    Ok(features)
                })
            }
            #[cfg(feature = "http")]
            #[tokio::main] //TODO: Fix async
            ReaderInner::Http(reader) => {
                let query: AttrQuery = filters.into_iter().map(|f| f.into()).collect();
                let file = File::open(&self.path).map_err(io_error_to_py_err)?;
                let attr_index_info = reader.header().attribute_index();
                //TODO: implement type casting by considering the key type of the index. For example, we want to convert integer into float when the index is a float. We can refer to `parse_and_convert_value` in fcb_api/src/filter_parser.rs as a reference.

                let mut feature_iter = reader
                    .select_attr_query(&query)
                    .await
                    .map_err(fcb_error_to_py_err)?;

                Python::with_gil(|py| {
                    let mut features = Vec::new();
                    while let Ok(Some(iter)) = feature_iter.next() {
                        let fcb_feature = iter.cur_feature();
                        let py_feature = cityfeature_to_python(py, fcb_feature)?;
                        features.push(py_feature);
                    }
                    Ok(features)
                })
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
            }
            #[cfg(feature = "http")]
            ReaderInner::Http(_) => Err(PyErr::new::<crate::error::FcbError, _>(
                "Iterator not yet implemented for HTTP reader",
            )),
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
    Http(AsyncFeatureIter<reqwest::Client>),
}

#[pymethods]
impl ReaderIterator {
    fn __iter__(slf: PyRef<Self>) -> PyRef<Self> {
        slf
    }

    fn __next__(&mut self) -> PyResult<Option<Feature>> {
        match &mut self.inner {
            IteratorInner::File(iter) => match iter.next() {
                Ok(Some(feature_iter)) => Python::with_gil(|py| {
                    let fcb_feature = feature_iter.cur_feature();
                    let py_feature = cityfeature_to_python(py, fcb_feature)?;
                    Ok(Some(py_feature))
                }),
                Err(e) => Err(fcb_error_to_py_err(e)),
                Ok(None) => Ok(None),
            },
            #[cfg(feature = "http")]
            #[tokio::main] //TODO: Fix async
            IteratorInner::Http(iter) => match iter.next().await {
                Ok(Some(feature)) => Python::with_gil(|py| {
                    let fcb_feature = feature.cur_feature();
                    let py_feature = cityfeature_to_python(py, fcb_feature)?;
                    Ok(Some(py_feature))
                }),
                Err(e) => Err(fcb_error_to_py_err(e)),
                Ok(None) => Ok(None),
            },
        }
    }
}
