use actix_web::{error::ResponseError, http::StatusCode, HttpResponse};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SentinelError {
    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Authentication failed: {0}")]
    AuthError(String),

    #[error("Invalid input: {0}")]
    ValidationError(String),

    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("Internal server error: {0}")]
    InternalError(String),
}

impl ResponseError for SentinelError {
    fn error_response(&self) -> HttpResponse {
        let status = self.status_code();
        let error_message = ErrorResponse {
            error: self.to_string(),
            status: status.as_u16(),
        };

        HttpResponse::build(status).json(error_message)
    }

    fn status_code(&self) -> StatusCode {
        match self {
            SentinelError::DatabaseError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            SentinelError::ConfigError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            SentinelError::AuthError(_) => StatusCode::UNAUTHORIZED,
            SentinelError::ValidationError(_) => StatusCode::BAD_REQUEST,
            SentinelError::NotFound(_) => StatusCode::NOT_FOUND,
            SentinelError::RateLimitExceeded => StatusCode::TOO_MANY_REQUESTS,
            SentinelError::InternalError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

#[derive(serde::Serialize)]
struct ErrorResponse {
    error: String,
    status: u16,
}

pub type SentinelResult<T> = Result<T, SentinelError>;
