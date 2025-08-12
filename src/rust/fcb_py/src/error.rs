use pyo3::exceptions::PyException;
use pyo3::prelude::*;

// Python exception type for FlatCityBuf errors
// Using create_exception! macro for PyO3 0.21 compatibility
pyo3::create_exception!(flatcitybuf, FcbError, PyException);

// Helper functions to convert errors to PyErr
pub fn fcb_error_to_py_err(err: fcb_core::error::Error) -> PyErr {
    PyErr::new::<FcbError, _>(err.to_string())
}

pub fn io_error_to_py_err(err: std::io::Error) -> PyErr {
    PyErr::new::<FcbError, _>(format!("IO Error: {}", err))
}

pub fn json_error_to_py_err(err: serde_json::Error) -> PyErr {
    PyErr::new::<FcbError, _>(format!("JSON Error: {}", err))
}
