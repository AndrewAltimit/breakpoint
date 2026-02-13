use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

#[derive(Debug)]
pub enum AppError {
    BadRequest(String),
    NotFound(String),
    #[allow(dead_code)]
    Unauthorized(String),
    #[allow(dead_code)]
    Internal(String),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadRequest(m) | Self::NotFound(m) | Self::Unauthorized(m) | Self::Internal(m) => {
                write!(f, "{m}")
            },
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            Self::BadRequest(m) => (StatusCode::BAD_REQUEST, m.clone()),
            Self::NotFound(m) => (StatusCode::NOT_FOUND, m.clone()),
            Self::Unauthorized(m) => (StatusCode::UNAUTHORIZED, m.clone()),
            Self::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m.clone()),
        };
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}
