use thiserror::Error;

/// Errors that can occur during CityJSON operations
#[derive(Error, Debug)]
pub enum CjError {
    #[error("IO error during CityJSON operation: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid CityJSON: {source}")]
    CityJSON {
        #[from]
        source: serde_json::Error,
    },

    #[error("Missing required field: {field}")]
    MissingField { field: String },

    #[error("Invalid CityJSON version: {version}")]
    InvalidVersion { version: String },

    #[error("Invalid CityObject type: {type_}")]
    InvalidObjectType { type_: String },

    #[error("Invalid geometry: {msg}")]
    InvalidGeometry { msg: String },

    #[error("Invalid semantic surface: {msg}")]
    InvalidSemantics { msg: String },
}

impl CjError {
    /// Creates a new MissingField error
    pub fn missing_field(field: impl Into<String>) -> Self {
        Self::MissingField {
            field: field.into(),
        }
    }

    /// Creates a new InvalidVersion error
    pub fn invalid_version(version: impl Into<String>) -> Self {
        Self::InvalidVersion {
            version: version.into(),
        }
    }

    /// Creates a new InvalidObjectType error
    pub fn invalid_object_type(type_: impl Into<String>) -> Self {
        Self::InvalidObjectType {
            type_: type_.into(),
        }
    }

    /// Creates a new InvalidGeometry error
    pub fn invalid_geometry(msg: impl Into<String>) -> Self {
        Self::InvalidGeometry { msg: msg.into() }
    }

    /// Creates a new InvalidSemantics error
    pub fn invalid_semantics(msg: impl Into<String>) -> Self {
        Self::InvalidSemantics { msg: msg.into() }
    }

    /// Returns true if the error is related to missing or invalid fields
    pub fn is_field_error(&self) -> bool {
        matches!(self, CjError::MissingField { .. })
    }

    /// Returns true if the error is related to CityJSON validation
    pub fn is_validation_error(&self) -> bool {
        matches!(
            self,
            CjError::InvalidVersion { .. }
                | CjError::InvalidObjectType { .. }
                | CjError::InvalidGeometry { .. }
                | CjError::InvalidSemantics { .. }
        )
    }
}
