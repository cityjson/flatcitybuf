use napi::Error as NapiError;

/// Convert fcb_core errors to napi errors
pub fn to_napi_error(err: fcb_core::Error) -> NapiError {
    NapiError::from_reason(err.to_string())
}
