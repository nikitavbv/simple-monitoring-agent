use std::option::NoneError;
use std::num::ParseIntError;
use custom_error::custom_error;
use async_trait::async_trait;

custom_error! {pub MetricCollectionError
    FailedToRead{source: std::io::Error} = "failed to read metric: {source}",
    FailedToParse{description: String} = "failed to parse metric: {description}",
    DatabaseQueryFailed{source: sqlx::error::Error} = "database query failed: {source}"
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

#[async_trait]
pub trait Metric {
    async fn collect() -> Result<Box<Self>, MetricCollectionError>;
}
