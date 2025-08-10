use pyo3::prelude::*;

mod error;
mod query;
mod reader;
mod types;
mod utils;

use error::FcbError;
use query::{AttrFilter, BBox, Operator};
#[cfg(feature = "http")]
use reader::AsyncReader;
use reader::Reader;
use types::{Feature, FileInfo, Geometry, Vertex};

/// Python bindings for FlatCityBuf
#[pymodule]
fn _fcb(_py: Python, m: &PyModule) -> PyResult<()> {
    // Core classes
    m.add_class::<Reader>()?;
    #[cfg(feature = "http")]
    m.add_class::<AsyncReader>()?;
    m.add_class::<Feature>()?;
    m.add_class::<Geometry>()?;
    m.add_class::<Vertex>()?;
    m.add_class::<FileInfo>()?;

    // Query types
    m.add_class::<BBox>()?;
    m.add_class::<AttrFilter>()?;
    m.add_class::<Operator>()?;

    // Exceptions
    m.add("FcbError", _py.get_type::<FcbError>())?;

    Ok(())
}
