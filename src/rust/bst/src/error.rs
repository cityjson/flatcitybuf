use crate::byte_serializable::ByteSerializableType;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid type: {0}")]
    InvalidType(String),
    #[error("invalid type id: {0}")]
    InvalidTypeId(u32),
    #[error("type mismatch: expected {1:?}, got {0:?}")]
    TypeMismatch(ByteSerializableType, ByteSerializableType),
    #[error("invalid byte serializable value")]
    InvalidByteSerializableValue,
}
