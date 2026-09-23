pub mod alerts;
pub mod events;
pub mod export;
pub mod flights;
pub mod history;
pub mod search;
pub mod ships;
pub mod static_data;
pub mod status;
pub mod tiles;
pub mod traffic;
pub mod watchlist;
pub mod weather;

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};

/// JSON error body: `{"error": "..."}`.
#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self { status, message: message.into() }
    }
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, message)
    }
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new(StatusCode::SERVICE_UNAVAILABLE, message)
    }
    pub fn bad_gateway(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_GATEWAY, message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(serde_json::json!({ "error": self.message }))).into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        tracing::error!("Internal error: {e:#}");
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
    }
}

impl From<crate::upstream::UpstreamError> for ApiError {
    fn from(e: crate::upstream::UpstreamError) -> Self {
        use crate::upstream::UpstreamError::*;
        match e {
            RateLimited { retry_after } => Self::unavailable(match retry_after {
                Some(s) => format!("Data provider rate limit reached. Retrying in {}.", human_duration(s)),
                None => "Data provider rate limit reached. Retrying shortly.".into(),
            }),
            other => Self::bad_gateway(format!("Data provider unavailable ({other})")),
        }
    }
}

pub fn human_duration(secs: i64) -> String {
    match secs {
        s if s < 90 => format!("{s} s"),
        s if s < 5400 => format!("{} min", (s + 30) / 60),
        s => format!("{} h", (s + 1800) / 3600),
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

pub async fn api_not_found() -> ApiError {
    ApiError::not_found("Unknown API endpoint")
}

#[cfg(test)]
mod tests {
    #[test]
    fn durations_read_naturally() {
        assert_eq!(super::human_duration(30), "30 s");
        assert_eq!(super::human_duration(600), "10 min");
        assert_eq!(super::human_duration(7200), "2 h");
    }
}
