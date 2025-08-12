// use chrono::{DateTime, Utc}; // Unused for now to avoid dependency issues
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use serde_json::Value;
// use std::collections::HashMap; // Unused for now

/// Python representation of a 3D vertex
#[pyclass]
#[derive(Clone, Debug)]
pub struct Vertex {
    #[pyo3(get)]
    pub x: f64,
    #[pyo3(get)]
    pub y: f64,
    #[pyo3(get)]
    pub z: f64,
}

#[pymethods]
impl Vertex {
    #[new]
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    fn __repr__(&self) -> String {
        format!("Vertex({}, {}, {})", self.x, self.y, self.z)
    }

    fn __str__(&self) -> String {
        self.__repr__()
    }

    fn to_tuple(&self) -> (f64, f64, f64) {
        (self.x, self.y, self.z)
    }
}

/// Python representation of geometry data
#[pyclass]
#[derive(Clone, Debug)]
pub struct Geometry {
    #[pyo3(get)]
    pub geometry_type: String,
    #[pyo3(get)]
    pub vertices: Vec<Vertex>,
    #[pyo3(get)]
    pub boundaries: PyObject,
    #[pyo3(get)]
    pub semantics: Option<PyObject>,
}

#[pymethods]
impl Geometry {
    #[new]
    pub fn new(
        geometry_type: String,
        vertices: Vec<Vertex>,
        boundaries: PyObject,
        semantics: Option<PyObject>,
    ) -> Self {
        Self {
            geometry_type,
            vertices,
            boundaries,
            semantics,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "Geometry(type='{}', vertices={}, boundaries={})",
            self.geometry_type,
            self.vertices.len(),
            self.boundaries
        )
    }

    fn __str__(&self) -> String {
        self.__repr__()
    }
}

/// Python representation of a CityJSON Feature
/// Matches the structure of CityJSONFeature with CityObjects
#[pyclass]
#[derive(Clone, Debug)]
pub struct Feature {
    #[pyo3(get)]
    pub id: String,
    #[pyo3(get)]
    pub r#type: String,
    #[pyo3(get)]
    pub city_objects: PyObject, // Dict[str, CityObject]
    #[pyo3(get)]
    pub vertices: Vec<Vertex>,
}

/// Python representation of a CityJSON CityObject
#[pyclass]
#[derive(Clone, Debug)]
pub struct CityObject {
    #[pyo3(get)]
    pub r#type: String,
    #[pyo3(get)]
    pub geometry: Vec<Geometry>,
    #[pyo3(get)]
    pub attributes: PyObject,
    #[pyo3(get)]
    pub children: Option<Vec<String>>,
    #[pyo3(get)]
    pub parents: Option<Vec<String>>,
}

#[pymethods]
impl Feature {
    #[new]
    pub fn new(id: String, r#type: String, city_objects: PyObject, vertices: Vec<Vertex>) -> Self {
        Self {
            id,
            r#type,
            city_objects,
            vertices,
        }
    }

    fn __repr__(&self) -> String {
        Python::with_gil(|py| {
            let n_objects = if let Ok(dict) = self.city_objects.downcast::<PyDict>(py) {
                dict.len()
            } else {
                0
            };
            format!(
                "Feature(id='{}', type='{}', city_objects={}, vertices={})",
                self.id,
                self.r#type,
                n_objects,
                self.vertices.len()
            )
        })
    }
}

#[pymethods]
impl CityObject {
    #[new]
    pub fn new(
        r#type: String,
        geometry: Vec<Geometry>,
        attributes: PyObject,
        children: Option<Vec<String>>,
        parents: Option<Vec<String>>,
    ) -> Self {
        Self {
            r#type,
            geometry,
            attributes,
            children,
            parents,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "CityObject(type='{}', geometries={}, children={:?}, parents={:?})",
            self.r#type,
            self.geometry.len(),
            self.children,
            self.parents
        )
    }
}

/// File metadata and schema information
#[pyclass]
#[derive(Clone, Debug)]
pub struct FileInfo {
    #[pyo3(get)]
    pub feature_count: u64,
    #[pyo3(get)]
    pub columns: PyObject,
    #[pyo3(get)]
    pub crs: Option<String>,
    #[pyo3(get)]
    pub bbox: Option<(f64, f64, f64, f64)>,
}

#[pymethods]
impl FileInfo {
    #[new]
    pub fn new(
        feature_count: u64,
        columns: PyObject,
        crs: Option<String>,
        bbox: Option<(f64, f64, f64, f64)>,
    ) -> Self {
        Self {
            feature_count,
            columns,
            crs,
            bbox,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "FileInfo(features={}, columns={}, crs='{}', bbox={:?})",
            self.feature_count,
            "...", // We'll implement proper column display later
            self.crs.as_ref().unwrap_or(&"None".to_string()),
            self.bbox,
        )
    }
}

// Helper functions for converting between Rust and Python types
pub fn value_to_python(py: Python, value: &Value) -> PyResult<PyObject> {
    match value {
        Value::Null => Ok(py.None()),
        Value::Bool(b) => Ok(b.to_object(py)),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(i.to_object(py))
            } else if let Some(f) = n.as_f64() {
                Ok(f.to_object(py))
            } else {
                Ok(py.None())
            }
        }
        Value::String(s) => Ok(s.to_object(py)),
        Value::Array(arr) => {
            let py_list = PyList::empty(py);
            for item in arr {
                py_list.append(value_to_python(py, item)?)?;
            }
            Ok(py_list.to_object(py))
        }
        Value::Object(obj) => {
            let py_dict = PyDict::new(py);
            for (key, value) in obj {
                py_dict.set_item(key, value_to_python(py, value)?)?;
            }
            Ok(py_dict.to_object(py))
        }
    }
}

/// Python representation of CityJSON Transform
#[pyclass]
#[derive(Clone)]
pub struct Transform {
    #[pyo3(get)]
    pub scale: Vec<f64>,
    #[pyo3(get)]
    pub translate: Vec<f64>,
}

#[pymethods]
impl Transform {
    #[new]
    pub fn new(scale: Vec<f64>, translate: Vec<f64>) -> Self {
        Self { scale, translate }
    }

    fn __repr__(&self) -> String {
        format!(
            "Transform(scale={:?}, translate={:?})",
            self.scale, self.translate
        )
    }
}

/// Python representation of CityJSON Metadata
#[pyclass]
#[derive(Clone)]
pub struct Metadata {
    #[pyo3(get)]
    pub geographical_extent: Option<Vec<f64>>,
    #[pyo3(get)]
    pub identifier: Option<String>,
    #[pyo3(get)]
    pub reference_date: Option<String>,
    #[pyo3(get)]
    pub reference_system: Option<String>,
    #[pyo3(get)]
    pub title: Option<String>,
}

#[pymethods]
impl Metadata {
    #[new]
    #[pyo3(signature = (geographical_extent=None, identifier=None, reference_date=None, reference_system=None, title=None))]
    pub fn new(
        geographical_extent: Option<Vec<f64>>,
        identifier: Option<String>,
        reference_date: Option<String>,
        reference_system: Option<String>,
        title: Option<String>,
    ) -> Self {
        Self {
            geographical_extent,
            identifier,
            reference_date,
            reference_system,
            title,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "Metadata(identifier={:?}, title={:?}, reference_system={:?})",
            self.identifier, self.title, self.reference_system
        )
    }
}

/// Python representation of CityJSON (header/metadata)
#[pyclass]
#[derive(Clone)]
pub struct CityJSON {
    #[pyo3(get)]
    pub r#type: String,
    #[pyo3(get)]
    pub version: String,
    #[pyo3(get)]
    pub transform: Transform,
    #[pyo3(get)]
    pub metadata: Option<Metadata>,
    #[pyo3(get)]
    pub feature_count: u64,
}

#[pymethods]
impl CityJSON {
    #[new]
    #[pyo3(signature = (r#type, version, transform, feature_count, metadata=None))]
    pub fn new(
        r#type: String,
        version: String,
        transform: Transform,
        feature_count: u64,
        metadata: Option<Metadata>,
    ) -> Self {
        Self {
            r#type,
            version,
            transform,
            metadata,
            feature_count,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "CityJSON(type='{}', version='{}', features={}, metadata={:?})",
            self.r#type,
            self.version,
            self.feature_count,
            self.metadata.is_some()
        )
    }
}
