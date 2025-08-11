use pyo3::prelude::*;

mod async_reader;
mod error;
mod query;
mod reader;
mod type_conversion;
mod types;
mod utils;

#[cfg(feature = "http")]
use async_reader::{AsyncFeatureIterator, AsyncReader, AsyncReaderOpened};
use error::FcbError;
use query::{AttrFilter, BBox, Operator};
use reader::{FeatureIterator, Reader};
use types::{Feature, FileInfo, Geometry, Vertex};

/// Python bindings for FlatCityBuf
#[pymodule]
fn _fcb(_py: Python, m: &PyModule) -> PyResult<()> {
    // Core classes
    m.add_class::<Reader>()?;

    // Iterator classes
    m.add_class::<FeatureIterator>()?;

    #[cfg(feature = "http")]
    {
        m.add_class::<AsyncReader>()?;
        m.add_class::<AsyncReaderOpened>()?;
        m.add_class::<AsyncFeatureIterator>()?;
    }

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
