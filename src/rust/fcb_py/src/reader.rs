use crate::error::{fcb_error_to_py_err, io_error_to_py_err, FcbError};
use crate::query::{AttrFilter, BBox};
use crate::type_conversion::python_value_to_keytype;
use crate::types::{CityJSON, Feature, FileInfo};
use crate::utils::{cityfeature_to_python, header_to_cityjson, is_url};
use fallible_streaming_iterator::FallibleStreamingIterator;
use fcb_core::{packed_rtree::Query as SpatialQuery, AttrQuery, FcbReader};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use std::fs::File;
use std::io::BufReader;

/// Synchronous reader for local FlatCityBuf files
#[pyclass]
pub struct Reader {
    path: String,
}

#[pymethods]
impl Reader {
    /// Create a new reader for a local file
    #[new]
    pub fn new(path: String) -> PyResult<Self> {
        if is_url(&path) {
            return Err(PyErr::new::<FcbError, _>(
                "URL paths are not supported by Reader. Use AsyncReader for HTTP URLs.",
            ));
        }

        // Test that file can be opened
        File::open(&path).map_err(io_error_to_py_err)?;

        Ok(Self { path })
    }

    /// Get file information and metadata
    pub fn info(&self) -> PyResult<FileInfo> {
        let file = File::open(&self.path).map_err(io_error_to_py_err)?;
        let buf_reader = BufReader::new(file);
        let reader = FcbReader::open(buf_reader).map_err(fcb_error_to_py_err)?;

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

    /// Get CityJSON header information with metadata and transform
    pub fn cityjson_header(&self) -> PyResult<CityJSON> {
        let file = File::open(&self.path).map_err(io_error_to_py_err)?;
        let buf_reader = BufReader::new(file);
        let reader = FcbReader::open(buf_reader).map_err(fcb_error_to_py_err)?;

        Python::with_gil(|py| header_to_cityjson(py, &reader.header()))
    }

    /// Query features by bounding box (returns an iterator)
    pub fn query_bbox(
        &self,
        min_x: f64,
        min_y: f64,
        max_x: f64,
        max_y: f64,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> PyResult<FeatureIterator> {
        let bbox = BBox::new(min_x, min_y, max_x, max_y);
        self.query_spatial(bbox, limit, offset)
    }

    /// Query features by spatial bounding box (returns an iterator)
    pub fn query_spatial(
        &self,
        bbox: BBox,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> PyResult<FeatureIterator> {
        let query: SpatialQuery = bbox.into();

        let file = File::open(&self.path).map_err(io_error_to_py_err)?;
        let buf_reader = BufReader::new(file);
        let reader = FcbReader::open(buf_reader).map_err(fcb_error_to_py_err)?;

        let feature_iter = reader
            .select_query(query, limit, offset)
            .map_err(fcb_error_to_py_err)?;

        let total_count = feature_iter
            .size_hint()
            .1
            .unwrap_or(feature_iter.size_hint().0) as u64;

        Ok(FeatureIterator {
            inner: Box::new(feature_iter),
            total_count,
        })
    }

    /// Query features by attribute filter (returns an iterator)
    pub fn query_attr(&self, filters: Vec<AttrFilter>) -> PyResult<FeatureIterator> {
        let file = File::open(&self.path).map_err(io_error_to_py_err)?;
        let buf_reader = BufReader::new(file);
        let reader = FcbReader::open(buf_reader).map_err(fcb_error_to_py_err)?;

        let header = reader.header();

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
        let feature_iter = reader
            .select_attr_query(attr_query)
            .map_err(fcb_error_to_py_err)?;

        let total_count = feature_iter
            .size_hint()
            .1
            .unwrap_or(feature_iter.size_hint().0) as u64;

        Ok(FeatureIterator {
            inner: Box::new(feature_iter),
            total_count,
        })
    }

    /// Get all features as an iterator
    pub fn __iter__(&self) -> PyResult<FeatureIterator> {
        let file = File::open(&self.path).map_err(io_error_to_py_err)?;
        let buf_reader = BufReader::new(file);
        let reader = FcbReader::open(buf_reader).map_err(fcb_error_to_py_err)?;
        let total_count = reader.header().features_count();

        let feature_iter = reader.select_all().map_err(fcb_error_to_py_err)?;

        Ok(FeatureIterator {
            inner: Box::new(feature_iter),
            total_count,
        })
    }

    fn __repr__(&self) -> String {
        format!("Reader('{}')", self.path)
    }
}

/// Iterator that wraps FeatureIter from fcb_core
#[pyclass]
pub struct FeatureIterator {
    // Use a boxed trait object to handle different FeatureIter types
    inner: Box<
        dyn FallibleStreamingIterator<
                Item = fcb_core::city_buffer::FcbBuffer,
                Error = fcb_core::error::Error,
            > + Send,
    >,
    total_count: u64,
}

#[pymethods]
impl FeatureIterator {
    /// Total number of features that will be returned
    pub fn total_count(&self) -> u64 {
        self.total_count
    }

    /// Get size hint (remaining features, exact count if known)
    pub fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }

    /// Number of features remaining
    pub fn count(&self) -> usize {
        self.inner.size_hint().1.unwrap_or(self.inner.size_hint().0)
    }

    fn __iter__(slf: PyRef<Self>) -> PyRef<Self> {
        slf
    }

    fn __next__(&mut self) -> PyResult<Option<Feature>> {
        // Use the FallibleStreamingIterator's advance method
        match self.inner.advance() {
            Ok(()) => {
                if let Some(buffer) = self.inner.get() {
                    Python::with_gil(|py| {
                        let py_feature = cityfeature_to_python(py, buffer)?;
                        Ok(Some(py_feature))
                    })
                } else {
                    Ok(None)
                }
            }
            Err(e) => Err(fcb_error_to_py_err(e)),
        }
    }

    fn __len__(&self) -> usize {
        self.count()
    }

    /// Collect all remaining features into a list
    pub fn collect(&mut self) -> PyResult<Vec<Feature>> {
        let mut features = Vec::new();
        while let Some(feature) = self.__next__()? {
            features.push(feature);
        }
        Ok(features)
    }

    fn __repr__(&self) -> String {
        format!("FeatureIterator(count={})", self.count())
    }
}
