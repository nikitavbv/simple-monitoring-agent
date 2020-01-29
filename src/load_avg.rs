use std::option::NoneError;
use std::num::ParseFloatError;

use async_std::fs::read_to_string;
use chrono::{Utc, DateTime};
use custom_error::custom_error;

use crate::database::Database;

pub struct LoadAverageMetric {
    timestamp: DateTime<Utc>,
    one: f64,
    five: f64,
    fifteen: f64
}

custom_error! {pub LoadAverageMetricError
    FailedToRead{source: std::io::Error} = "failed to read metric",
    FailedToParse{description: String} = "failed to parse metric",
    DatabaseQueryFailed{source: sqlx::error::Error} = "database query failed"
}

impl From<std::option::NoneError> for LoadAverageMetricError {
    fn from(_: NoneError) -> Self {
        LoadAverageMetricError::FailedToParse{description: "NoneError".to_string()}
    }
}

impl From<std::num::ParseFloatError> for LoadAverageMetricError {
    fn from(err: ParseFloatError) -> Self {
        LoadAverageMetricError::FailedToParse{description: err.to_string()}
    }
}

pub async fn monitor_load_average() -> Result<LoadAverageMetric, LoadAverageMetricError> {
    let timestamp = Utc::now();

    let metric = read_to_string("/proc/loadavg").await?;
    let mut spl = metric.split_whitespace();

    Ok(LoadAverageMetric {
        timestamp,
        one: spl.next()?.parse()?,
        five: spl.next()?.parse()?,
        fifteen: spl.next()?.parse()?,
    })
}

pub async fn save_load_average_metric(mut database: &Database, hostname: String, metric: LoadAverageMetric) -> Result<(), LoadAverageMetricError> {
    sqlx::query!(
        "insert into metric_load_average (hostname, timestamp, one, five, fifteen) values ($1, $2, $3, $4, $5) returning hostname",
        hostname, metric.timestamp, metric.one, metric.five, metric.fifteen
    ).fetch_one(&mut database).await?;

    Ok(())
}