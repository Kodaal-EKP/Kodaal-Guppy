use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::fmt;

#[derive(Debug, Clone)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
    pub field: Option<&'static str>,
}

#[derive(Debug, Serialize)]
struct ErrorEnvelope<'a> {
    error: ErrorBody<'a>,
}

#[derive(Debug, Serialize)]
struct ErrorBody<'a> {
    code: &'a str,
    message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    field: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<Value>,
}

impl ApiError {
    pub fn invalid_payload(message: impl Into<String>, field: Option<&'static str>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "INVALID_PAYLOAD", message, field)
    }

    pub fn invalid_query(message: impl Into<String>, field: Option<&'static str>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "INVALID_QUERY", message, field)
    }

    pub fn unauthorized() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "UNAUTHORIZED",
            "Missing or invalid authentication".to_string(),
            None,
        )
    }

    pub fn not_found(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, code, message, None)
    }

    pub fn conflict(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, code, message, None)
    }

    pub fn too_large(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::PAYLOAD_TOO_LARGE, code, message, None)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
            message,
            None,
        )
    }

    pub fn new(
        status: StatusCode,
        code: &'static str,
        message: impl Into<String>,
        field: Option<&'static str>,
    ) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            field,
        }
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ApiError {}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = ErrorEnvelope {
            error: ErrorBody {
                code: self.code,
                message: &self.message,
                field: self.field,
                details: Some(json!({})),
            },
        };
        (self.status, Json(body)).into_response()
    }
}

impl From<rusqlite::Error> for ApiError {
    fn from(error: rusqlite::Error) -> Self {
        Self::internal(error.to_string())
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(error: serde_json::Error) -> Self {
        Self::invalid_payload(error.to_string(), None)
    }
}
