use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("http error: {0}")]
    Http(#[from] http_range_client::HttpError),

    #[error("rtree error: {0}")]
    RTreeError(String),
}

impl From<&str> for Error {
    fn from(s: &str) -> Self {
        Error::RTreeError(s.to_string())
    }
}
