// use chrono::{DateTime, Utc}; // Unused for now to avoid dependency issues
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use serde_json::Value;
// use std::collections::HashMap; // Unused for now

/// Python representation of a 3D vertex
#[pyclass]
#[derive(Clone)]
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
#[derive(Clone)]
pub struct Geometry {
    #[pyo3(get)]
    pub geometry_type: String,
    #[pyo3(get)]
    pub vertices: Vec<Vertex>,
    #[pyo3(get)]
    pub boundaries: Vec<Vec<u32>>,
    #[pyo3(get)]
    pub semantics: Option<PyObject>,
}

#[pymethods]
impl Geometry {
    #[new]
    pub fn new(
        geometry_type: String,
        vertices: Vec<Vertex>,
        boundaries: Vec<Vec<u32>>,
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
            self.boundaries.len()
        )
    }
}

/// Python representation of a FlatCityBuf feature
#[pyclass]
#[derive(Clone)]
pub struct Feature {
    #[pyo3(get)]
    pub id: Option<String>,
    #[pyo3(get)]
    pub feature_type: String,
    #[pyo3(get)]
    pub geometry: Vec<Geometry>,
    #[pyo3(get)]
    pub attributes: PyObject,
}

#[pyclass]
#[derive(Clone)]
pub struct CityObject {
    #[pyo3(get)]
    pub id: Option<String>,
    #[pyo3(get)]
    pub type_: String,
}

#[pymethods]
impl Feature {
    #[new]
    #[pyo3(signature = (feature_type, id=None, geometry=Vec::new(), attributes=None))]
    pub fn new(
        feature_type: String,
        id: Option<String>,
        geometry: Vec<Geometry>,
        attributes: Option<PyObject>,
    ) -> Self {
        Python::with_gil(|py| Self {
            id,
            feature_type,
            geometry,
            attributes: attributes.unwrap_or_else(|| py.None()),
        })
    }

    fn __repr__(&self) -> String {
        format!(
            "Feature(id='{}', type='{}', geometries={})",
            self.id.as_ref().unwrap_or(&"None".to_string()),
            self.feature_type,
            self.geometry.len()
        )
    }
}

/// File metadata and schema information
#[pyclass]
#[derive(Clone)]
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

pub fn python_to_value(obj: &PyAny) -> PyResult<Value> {
    if obj.is_none() {
        Ok(Value::Null)
    } else if let Ok(b) = obj.extract::<bool>() {
        Ok(Value::Bool(b))
    } else if let Ok(i) = obj.extract::<i64>() {
        Ok(Value::Number(serde_json::Number::from(i)))
    } else if let Ok(f) = obj.extract::<f64>() {
        Ok(Value::Number(
            serde_json::Number::from_f64(f).unwrap_or(serde_json::Number::from(0)),
        ))
    } else if let Ok(s) = obj.extract::<String>() {
        Ok(Value::String(s))
    // Removed DateTime handling for now to avoid dependency issues
    } else {
        // For complex types, convert to JSON string and parse
        let json_str = obj.str()?.to_str()?;
        serde_json::from_str(json_str).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                "Cannot convert to JSON: {}",
                e
            ))
        })
    }
}
