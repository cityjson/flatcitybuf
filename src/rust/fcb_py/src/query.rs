use pyo3::prelude::*;
use pyo3::types::PyType;

/// Bounding box for spatial queries
#[pyclass]
#[derive(Clone)]
pub struct BBox {
    #[pyo3(get)]
    pub min_x: f64,
    #[pyo3(get)]
    pub min_y: f64,
    #[pyo3(get)]
    pub max_x: f64,
    #[pyo3(get)]
    pub max_y: f64,
}

#[pymethods]
impl BBox {
    #[new]
    pub fn new(min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> Self {
        Self {
            min_x,
            min_y,
            max_x,
            max_y,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "BBox({}, {}, {}, {})",
            self.min_x, self.min_y, self.max_x, self.max_y
        )
    }

    fn __str__(&self) -> String {
        self.__repr__()
    }

    fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.min_x && x <= self.max_x && y >= self.min_y && y <= self.max_y
    }

    fn intersects(&self, other: &BBox) -> bool {
        !(self.max_x < other.min_x
            || self.min_x > other.max_x
            || self.max_y < other.min_y
            || self.min_y > other.max_y)
    }

    fn area(&self) -> f64 {
        (self.max_x - self.min_x) * (self.max_y - self.min_y)
    }
}

impl From<BBox> for fcb_core::packed_rtree::Query {
    fn from(bbox: BBox) -> Self {
        fcb_core::packed_rtree::Query::BBox(bbox.min_x, bbox.min_y, bbox.max_x, bbox.max_y)
    }
}

/// Query operators for attribute filtering
#[pyclass]
#[derive(Clone, Debug)]
pub enum Operator {
    Eq, // ==
    Ne, // !=
    Gt, // >
    Ge, // >=
    Lt, // <
    Le, // <=
}

#[pymethods]
impl Operator {
    fn __repr__(&self) -> String {
        match self {
            Operator::Eq => "Eq".to_string(),
            Operator::Ne => "Ne".to_string(),
            Operator::Gt => "Gt".to_string(),
            Operator::Ge => "Ge".to_string(),
            Operator::Lt => "Lt".to_string(),
            Operator::Le => "Le".to_string(),
        }
    }
}

impl From<Operator> for fcb_core::static_btree::query::Operator {
    fn from(op: Operator) -> Self {
        match op {
            Operator::Eq => fcb_core::static_btree::query::Operator::Eq,
            Operator::Ne => fcb_core::static_btree::query::Operator::Ne,
            Operator::Gt => fcb_core::static_btree::query::Operator::Gt,
            Operator::Ge => fcb_core::static_btree::query::Operator::Ge,
            Operator::Lt => fcb_core::static_btree::query::Operator::Lt,
            Operator::Le => fcb_core::static_btree::query::Operator::Le,
        }
    }
}

/// Attribute filter for querying features by attributes
#[pyclass]
#[derive(Clone)]
pub struct AttrFilter {
    #[pyo3(get)]
    pub field: String,
    #[pyo3(get)]
    pub operator: Operator,
    #[pyo3(get)]
    pub value: PyObject,
}

#[pymethods]
impl AttrFilter {
    #[new]
    pub fn new(field: String, operator: Operator, value: PyObject) -> Self {
        Self {
            field,
            operator,
            value,
        }
    }

    fn __repr__(&self) -> String {
        format!("AttrFilter('{}', {:?}, value)", self.field, self.operator)
    }

    /// Create an equality filter
    #[classmethod]
    fn eq(_cls: &PyType, field: String, value: PyObject) -> Self {
        Self::new(field, Operator::Eq, value)
    }

    /// Create a not-equal filter
    #[classmethod]
    fn ne(_cls: &PyType, field: String, value: PyObject) -> Self {
        Self::new(field, Operator::Ne, value)
    }

    /// Create a greater-than filter
    #[classmethod]
    fn gt(_cls: &PyType, field: String, value: PyObject) -> Self {
        Self::new(field, Operator::Gt, value)
    }

    /// Create a greater-than-or-equal filter
    #[classmethod]
    fn ge(_cls: &PyType, field: String, value: PyObject) -> Self {
        Self::new(field, Operator::Ge, value)
    }

    /// Create a less-than filter
    #[classmethod]
    fn lt(_cls: &PyType, field: String, value: PyObject) -> Self {
        Self::new(field, Operator::Lt, value)
    }

    /// Create a less-than-or-equal filter
    #[classmethod]
    fn le(_cls: &PyType, field: String, value: PyObject) -> Self {
        Self::new(field, Operator::Le, value)
    }
}

// Note: We removed the From<AttrFilter> implementation since we now handle
// conversion manually in the reader methods using proper type conversion

// Helper function to convert Python value to a string for now
// TODO: Implement proper KeyType conversion when static_btree API is available
pub fn python_value_to_string(_py: Python, obj: &PyObject) -> PyResult<String> {
    Python::with_gil(|py| {
        let any = obj.as_ref(py);
        let s = any.str()?.to_str()?.to_string();
        Ok(s)
    })
}
