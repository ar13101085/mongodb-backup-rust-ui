use actix_web::{http::StatusCode, HttpResponse, ResponseError};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("not found")]
    NotFound,

    #[error("unauthorized")]
    Unauthorized,

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("mongodb error: {0}")]
    Mongo(#[from] mongodb::error::Error),

    #[error("password hash error: {0}")]
    Hash(String),

    #[error("multipart error: {0}")]
    Multipart(String),

    #[error("internal: {0}")]
    Internal(String),
}

impl AppError {
    fn status(&self) -> StatusCode {
        match self {
            AppError::NotFound => StatusCode::NOT_FOUND,
            AppError::Unauthorized => StatusCode::UNAUTHORIZED,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::BadRequest(_) | AppError::Multipart(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl ResponseError for AppError {
    fn error_response(&self) -> HttpResponse {
        let status = self.status();
        if status.is_server_error() {
            tracing::error!(error = %self, "server error");
        }
        HttpResponse::build(status).json(json!({ "error": self.to_string() }))
    }
}

pub type AppResult<T> = Result<T, AppError>;
