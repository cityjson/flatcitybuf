use thiserror::Error;

/// Errors that can occur during FCB writing operations
#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error during write: {0}")]
    Io(#[from] std::io::Error),

    #[error("R-tree error: {0}")]
    Rtree(#[from] crate::packed_rtree::Error),

    #[error("StaticBTree error: {0}")]
    StaticBTree(#[from] crate::static_btree::Error),

    #[error("Invalid attribute name: {name}")]
    InvalidAttributeName { name: String },

    #[error("Attribute value exceeds maximum size: {name} ({size} bytes)")]
    AttributeSizeExceeded { name: String, size: usize },

    #[error("Feature has no geometry")]
    NoGeometry,

    #[error("Invalid geometry: {msg}")]
    InvalidGeometry { msg: String },

    #[error(
        "Buffer overflow: attempted to write {attempted} bytes but buffer capacity is {capacity}"
    )]
    BufferOverflow { attempted: usize, capacity: usize },
}

impl Error {
    /// Creates a new InvalidAttributeName error
    pub fn invalid_attribute_name(name: impl Into<String>) -> Self {
        Self::InvalidAttributeName { name: name.into() }
    }

    /// Creates a new InvalidGeometry error
    pub fn invalid_geometry(msg: impl Into<String>) -> Self {
        Self::InvalidGeometry { msg: msg.into() }
    }

    /// Returns true if the error is related to attribute validation
    pub fn is_attribute_error(&self) -> bool {
        matches!(
            self,
            Error::InvalidAttributeName { .. } | Error::AttributeSizeExceeded { .. }
        )
    }

    /// Returns true if the error is related to geometry validation
    pub fn is_geometry_error(&self) -> bool {
        matches!(self, Error::NoGeometry | Error::InvalidGeometry { .. })
    }
}

impl From<Error> for crate::error::Error {
    fn from(err: Error) -> Self {
        match err {
            Error::Io(e) => Self::IoError(e),
            Error::Rtree(e) => Self::RtreeError(e),
            Error::StaticBTree(e) => Self::StaticBTree { source: e },
            Error::InvalidAttributeName { name } => Self::InvalidAttributeValue {
                msg: format!("Invalid attribute name: {name}"),
            },
            Error::AttributeSizeExceeded { name, size } => Self::InvalidAttributeValue {
                msg: format!("Attribute '{name}' exceeds maximum size ({size} bytes)"),
            },
            Error::NoGeometry => Self::InvalidAttributeValue {
                msg: "Feature has no geometry".to_string(),
            },
            Error::InvalidGeometry { msg } => Self::InvalidAttributeValue {
                msg: format!("Invalid geometry: {msg}"),
            },
            Error::BufferOverflow {
                attempted,
                capacity,
            } => Self::InvalidAttributeValue {
                msg: format!(
                    "Buffer overflow: attempted to write {attempted} bytes but capacity is {capacity}"
                ),
            },
        }
    }
}
