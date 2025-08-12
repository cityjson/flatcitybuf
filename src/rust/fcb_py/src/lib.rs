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
use types::{CityJSON, CityObject, Feature, FileInfo, Geometry, Metadata, Transform, Vertex};

/// Python bindings for FlatCityBuf
#[pymodule]
fn flatcitybuf(_py: Python, m: &PyModule) -> PyResult<()> {
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
    m.add_class::<CityObject>()?;
    m.add_class::<Geometry>()?;
    m.add_class::<Vertex>()?;
    m.add_class::<FileInfo>()?;
    m.add_class::<CityJSON>()?;
    m.add_class::<Transform>()?;
    m.add_class::<Metadata>()?;

    // Query types
    m.add_class::<BBox>()?;
    m.add_class::<AttrFilter>()?;
    m.add_class::<Operator>()?;

    // Exceptions - use the exception created by create_exception! macro
    m.add("FcbError", _py.get_type_bound::<FcbError>())?;

    Ok(())
}
