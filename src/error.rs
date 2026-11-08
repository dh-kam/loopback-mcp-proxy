use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    User,
    Auth,
    Session,
    Upstream,
    Internal,
}

#[derive(Debug, Clone)]
pub struct AppError {
    pub status: StatusCode,
    pub message: String,
    pub kind: ErrorKind,
}

impl AppError {
    pub fn new(status: StatusCode, kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
            kind,
        }
    }

    pub fn bad(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, ErrorKind::User, message)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, ErrorKind::User, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, ErrorKind::User, message)
    }

    pub fn unauthorized() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            ErrorKind::Auth,
            "admin token required",
        )
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, ErrorKind::Auth, message)
    }

    pub fn auth(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, ErrorKind::Auth, message)
    }

    pub fn session(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, ErrorKind::Session, message)
    }

    pub fn upstream(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_GATEWAY, ErrorKind::Upstream, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            ErrorKind::Internal,
            message,
        )
    }

    pub fn is_auth(&self) -> bool {
        self.kind == ErrorKind::Auth
    }

    pub fn is_session(&self) -> bool {
        self.kind == ErrorKind::Session
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AppError {}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status;
        let body = Json(json!({ "error": self.message }));
        (status, body).into_response()
    }
}
