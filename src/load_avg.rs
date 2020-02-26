use std::option::NoneError;
use std::num::ParseFloatError;

use async_std::fs::read_to_string;
use chrono::{Utc, DateTime};
use custom_error::custom_error;
use async_trait::async_trait;

use crate::database::Database;
use crate::config::get_max_metrics_age;
use crate::types::{Metric, MetricCollectionError, MetricSaveError};
use sqlx::{PgConnection, Pool};

pub struct LoadAverageMetric {
    timestamp: DateTime<Utc>,
    one: f64,
    five: f64,
    fifteen: f64
}

#[async_trait]
impl Metric for LoadAverageMetric {

    async fn collect(mut database: &Database) -> Result<Box<Self>, MetricCollectionError> {
        let timestamp = Utc::now();

        let metric = read_to_string("/proc/loadavg").await?;
        let mut spl = metric.split_whitespace();

        Ok(Box::new(LoadAverageMetric {
            timestamp,
            one: spl.next()?.parse()?,
            five: spl.next()?.parse()?,
            fifteen: spl.next()?.parse()?,
        }))
    }

    async fn save(&self, mut database: &Pool<PgConnection>, previous: &Self, hostname: &str) -> Result<(), MetricSaveError> {
        sqlx::query!(
            "insert into metric_load_average (hostname, timestamp, one, five, fifteen) values ($1, $2, $3, $4, $5) returning hostname",
            hostname, self.timestamp, self.one, self.five, self.fifteen
        ).fetch_one(&mut database).await?;

        Ok(())
    }
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

async fn cleanup_load_average_metric(mut database: &Database) -> Result<(), LoadAverageMetricError> {
    let min_timestamp = Utc::now() - get_max_metrics_age();

    sqlx::query!("delete from metric_load_average where timestamp < $1 returning 1 as result", min_timestamp)
        .fetch_one(&mut database).await?;

    Ok(())
}