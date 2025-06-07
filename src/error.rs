use rmcp::{Error as McpError, ServiceError};
use serde::Serialize;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("internal error: {0}")]
    Internal(#[from] Box<dyn std::error::Error>),
    #[error("service error: {0}")]
    Service(#[from] ServiceError),
    #[error("request error: {0}")]
    Request(#[from] reqwest::Error),
    #[error("sql error: {0}")]
    Sql(#[from] sqlx::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("serde deserialize error: {0}")]
    JsonDeserialize(#[from] serde::de::value::Error),
    #[error("missing OpenRouter API key")]
    MissingApiKey,
    #[error("missing database URL")]
    MissingDatabaseUrl,
}

pub type AppResult<T> = Result<T, AppError>;

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(Serialize)]
        struct ErrorProto {
            message: String,
        }

        ErrorProto {
            message: self.to_string(),
        }
        .serialize(serializer)
    }
}

impl From<AppError> for McpError {
    fn from(err: AppError) -> Self {
        match err {
            AppError::Internal(err) => McpError::internal_error(err.to_string(), None),
            AppError::Service(err) => McpError::internal_error(err.to_string(), None),
            AppError::Request(err) => McpError::invalid_request(err.to_string(), None),
            AppError::Sql(err) => McpError::internal_error(err.to_string(), None),
            AppError::Io(err) => McpError::internal_error(err.to_string(), None),
            AppError::Json(err) => McpError::parse_error(err.to_string(), None),
            AppError::JsonDeserialize(err) => McpError::parse_error(err.to_string(), None),
            AppError::MissingApiKey => {
                McpError::invalid_request("Missing OpenRouter API key", None)
            }
            AppError::MissingDatabaseUrl => McpError::invalid_request("Missing database URL", None),
        }
    }
}
