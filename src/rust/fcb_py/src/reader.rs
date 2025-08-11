use crate::error::{fcb_error_to_py_err, io_error_to_py_err, FcbError};
use crate::query::{AttrFilter, BBox};
use crate::type_conversion::python_value_to_keytype;
use crate::types::{Feature, FileInfo};
use crate::utils::{cityfeature_to_python, is_url};
use fcb_core::{packed_rtree::Query as SpatialQuery, AttrQuery, FcbReader};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use std::fs::File;
use std::io::BufReader;

// We need to use the actual marker types from fcb_core, not local ones

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
        let query: SpatialQuery = bbox.into();

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

    /// Query features by attribute filter
    pub fn query_attr(&self, filters: Vec<AttrFilter>) -> PyResult<Vec<Feature>> {
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
        let mut feature_iter = reader
            .select_attr_query(attr_query)
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

    /// Get all features as an iterator
    pub fn __iter__(&self) -> PyResult<ReaderIterator> {
        let file = File::open(&self.path).map_err(io_error_to_py_err)?;
        let buf_reader = BufReader::new(file);
        let reader = FcbReader::open(buf_reader).map_err(fcb_error_to_py_err)?;
        let total_count = reader.header().features_count();

        Ok(ReaderIterator {
            path: self.path.clone(),
            current_index: 0,
            total_count,
        })
    }

    fn __repr__(&self) -> String {
        format!("Reader('{}')", self.path)
    }
}

/// Iterator for features from synchronous reader
#[pyclass]
pub struct ReaderIterator {
    // Store path to re-open file for each operation (simple but not efficient)
    path: String,
    current_index: usize,
    total_count: u64,
}

#[pymethods]
impl ReaderIterator {
    fn __iter__(slf: PyRef<Self>) -> PyRef<Self> {
        slf
    }

    fn __next__(&mut self) -> PyResult<Option<Feature>> {
        if self.current_index >= self.total_count as usize {
            return Ok(None);
        }

        // Open file and create iterator each time (inefficient but simple)
        let file = File::open(&self.path).map_err(io_error_to_py_err)?;
        let buf_reader = BufReader::new(file);
        let reader = FcbReader::open(buf_reader).map_err(fcb_error_to_py_err)?;
        let mut feature_iter = reader.select_all().map_err(fcb_error_to_py_err)?;

        // Skip to current position
        for _ in 0..self.current_index {
            if feature_iter.next().is_err() {
                return Ok(None);
            }
        }

        // Get the current feature
        match feature_iter.next() {
            Ok(Some(feature_data)) => {
                self.current_index += 1;
                Python::with_gil(|py| {
                    let fcb_feature = feature_data.cur_feature();
                    let py_feature = cityfeature_to_python(py, fcb_feature)?;
                    Ok(Some(py_feature))
                })
            }
            Ok(None) => Ok(None),
            Err(e) => Err(fcb_error_to_py_err(e)),
        }
    }
}
