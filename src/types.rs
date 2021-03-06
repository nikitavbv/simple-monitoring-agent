use std::option::NoneError;
use std::num::{ParseIntError, ParseFloatError};
use custom_error::custom_error;
use async_trait::async_trait;
use crate::database::Database;
use crate::docker::client::DockerClientError;

custom_error! {pub MetricCollectionError
    FailedToRead{source: std::io::Error} = "failed to read metric: {source}",
    FailedToParse{description: String} = "failed to parse metric: {description}",
    RequestFailed{source: reqwest::Error} = "failed to request nginx status: {source}",
    DatabaseQueryFailed{source: sqlx::error::Error} = "database query failed: {source}",
    NotConfigured{description: String} = "not configured: {description}",
    DockerClientError{source: DockerClientError} = "docker client error: {source}"
}

custom_error! {pub MetricSaveError
    DatabaseQueryFailed{source: sqlx::error::Error} = "database query failed: {source}"
}

custom_error! {pub MetricCleanupError
    DatabaseQueryFailed{source: sqlx::error::Error} = "database query failed: {source}"
}

custom_error! {pub MetricEncodingError
    NoRecord = "no record",
    FailedToSerialize{source: serde_json::error::Error} = "failed to serialize"
}

impl From<std::option::NoneError> for MetricCollectionError {
    fn from(_: NoneError) -> Self {
        MetricCollectionError::FailedToParse{description: "NoneError".to_string()}
    }
}

impl From<std::num::ParseIntError> for MetricCollectionError {
    fn from(err: ParseIntError) -> Self {
        MetricCollectionError::FailedToParse{description: err.to_string()}
    }
}

impl From<std::num::ParseFloatError> for MetricCollectionError {
    fn from(err: ParseFloatError) -> Self {
        MetricCollectionError::FailedToParse{description: err.to_string()}
    }
}

impl From<http::uri::InvalidUri> for MetricCollectionError {
    fn from(err: http::uri::InvalidUri) -> Self {
        MetricCollectionError::FailedToParse {
            description: err.to_string()
        }
    }
}

#[async_trait]
pub trait Metric {
}

#[async_trait]
pub trait MetricCollector {
    fn key(&self) -> String;
    async fn collect(&mut self) -> Result<(), MetricCollectionError>;
    async fn save(&self, mut database: &Database, hostname: &str) -> Result<(), MetricSaveError>;
    async fn encode(&self) -> Result<String, MetricEncodingError>;
    async fn cleanup(&self, mut database: &Database) -> Result<(), MetricCleanupError>;
}