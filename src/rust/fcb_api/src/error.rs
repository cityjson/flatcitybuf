use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};

#[derive(Debug)]
pub struct ApiError(pub StatusCode);
impl From<StatusCode> for ApiError {
    fn from(status: StatusCode) -> Self {
        Self(status)
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(serde_json::json!({"code": self.0.as_u16().to_string(), "description":self.0.canonical_reason().unwrap_or("Request failed")}))).into_response()
    }
}
impl ApiError {
    pub fn storage(error: impl std::fmt::Display) -> Self {
        tracing::warn!(error=%error,"dataset access failed");
        Self(StatusCode::SERVICE_UNAVAILABLE)
    }
}
