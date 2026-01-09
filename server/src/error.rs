use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use thiserror::Error;

use crate::models::ErrorResponse;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(#[from] redb::Error),

    #[error("Database error: {0}")]
    DatabaseError(#[from] redb::DatabaseError),

    #[error("Database transaction error: {0}")]
    Transaction(#[from] redb::TransactionError),

    #[error("Database table error: {0}")]
    Table(#[from] redb::TableError),

    #[error("Database storage error: {0}")]
    Storage(#[from] redb::StorageError),

    #[error("Database commit error: {0}")]
    Commit(#[from] redb::CommitError),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Unauthorized")]
    Unauthorized,

    #[error("User already exists")]
    UserExists,

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Document field missing")]
    DocumentMissing,

    #[error("Version conflict")]
    VersionConflict,
}

impl AppError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::UserExists => StatusCode::PAYMENT_REQUIRED, // 402, matching original
            Self::InvalidRequest(_) => StatusCode::FORBIDDEN,
            Self::DocumentMissing => StatusCode::FORBIDDEN,
            Self::VersionConflict => StatusCode::CONFLICT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_code(&self) -> u32 {
        match self {
            Self::Database(_)
            | Self::DatabaseError(_)
            | Self::Transaction(_)
            | Self::Table(_)
            | Self::Storage(_)
            | Self::Commit(_)
            | Self::Serialization(_) => 2000,
            Self::Unauthorized => 2001,
            Self::UserExists => 2002,
            Self::InvalidRequest(_) => 2003,
            Self::DocumentMissing => 2004,
            Self::VersionConflict => 2005,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = ErrorResponse::new(self.error_code(), self.to_string());
        (status, Json(body)).into_response()
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
