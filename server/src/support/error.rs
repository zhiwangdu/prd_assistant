use axum::{
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use tracing::{error, warn};

#[derive(Debug)]
pub struct AppError {
    status: StatusCode,
    message: String,
    details: Option<serde_json::Value>,
}

impl AppError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
            details: None,
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
            details: None,
        }
    }

    pub fn conflict(message: impl Into<String>, details: serde_json::Value) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: message.into(),
            details: Some(details),
        }
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: message.into(),
            details: None,
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
            details: None,
        }
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for AppError {}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        // Client-visible 4xx failures are useful warnings, while 5xx means a server fault.
        if self.status.is_server_error() {
            error!(status = %self.status, message = %self.message, "request failed");
        } else {
            warn!(status = %self.status, message = %self.message, "request rejected");
        }
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        let mut body = serde_json::json!({ "error": self.message });
        if let Some(details) = self.details {
            if let (Some(target), Some(source)) = (body.as_object_mut(), details.as_object()) {
                target.extend(source.clone());
            }
        }
        let body = Json(body);
        (self.status, headers, body).into_response()
    }
}
