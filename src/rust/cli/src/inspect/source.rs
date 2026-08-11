//! Resolve an inspect source (local path or HTTP URL) into an `InspectModel`.

use std::fs::File;
use std::io::BufReader;

use fcb_core::{FcbReader, HttpFcbReader};

use crate::inspect::model::{from_header, InspectModel};
use crate::CliError;

/// True when `source` is an `http://` or `https://` URL (scheme match is
/// case-insensitive). A bare Windows drive letter (`C:/...`) is deliberately
/// not treated as a scheme.
pub fn is_url(source: &str) -> bool {
    let lower = source.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// Load an `InspectModel` from a local path or an HTTP(S) URL. Only the header
/// is read; feature bytes are never fetched.
pub fn load_model(source: &str) -> Result<InspectModel, CliError> {
    if is_url(source) {
        load_model_http(source)
    } else {
        let file = File::open(source)?;
        let size_bytes = file.metadata()?.len();
        let fcb = FcbReader::open(BufReader::new(file))?;
        Ok(from_header(&fcb.header(), source, Some(size_bytes)))
    }
}

/// Fetch just the header over HTTP on a short-lived current-thread runtime.
///
/// The total object size stays unknown: `HttpFcbReader` reads the header with
/// range requests and exposes no content length, and asking for one would cost
/// an extra HEAD round trip for a cosmetic line.
fn load_model_http(url: &str) -> Result<InspectModel, CliError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let reader = HttpFcbReader::open(url).await?;
        Ok(from_header(&reader.header(), url, None))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn detects_http_and_https_urls() {
        assert!(is_url("http://example.com/a.fcb"));
        assert!(is_url("https://example.com/a.fcb"));
    }

    #[test]
    fn detects_urls_case_insensitively() {
        assert!(is_url("HTTPS://example.com/a.fcb"));
        assert!(is_url("Http://example.com/a.fcb"));
    }

    #[test]
    fn treats_local_paths_as_non_urls() {
        assert!(!is_url("./data/a.fcb"));
        assert!(!is_url("/abs/a.fcb"));
        assert!(!is_url("a.fcb"));
        // Windows drive letters must not be mistaken for a scheme.
        assert!(!is_url("C:/data/a.fcb"));
    }

    #[test]
    fn loads_model_from_local_file() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../conformance/inferable_types.fcb");
        let source = path.to_str().unwrap();
        let model = load_model(source).expect("load model");
        assert!(!model.version.is_empty());
        assert_eq!(model.source, source);
        // A local file's size comes straight from the filesystem metadata.
        let on_disk = std::fs::metadata(&path).expect("stat fixture").len();
        assert_eq!(model.size_bytes, Some(on_disk));
    }

    #[test]
    fn missing_local_file_is_an_error() {
        let err = load_model("/no/such/file.fcb");
        assert!(err.is_err());
    }
}
