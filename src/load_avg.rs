use std::option::NoneError;
use std::num::ParseFloatError;

use async_std::fs::read_to_string;
use chrono::{Utc, DateTime};
use custom_error::custom_error;
use async_trait::async_trait;
use serde::Serialize;

use crate::database::Database;
use crate::config::get_max_metrics_age;
use crate::types::{Metric, MetricCollectionError, MetricSaveError, MetricCleanupError, MetricCollector, MetricEncodingError};

#[derive(Serialize)]
pub struct LoadAverageMetric {
    timestamp: DateTime<Utc>,
    one: f64,
    five: f64,
    fifteen: f64
}

#[async_trait]
impl Metric for LoadAverageMetric {
}

pub struct LoadAverageMetricCollector {
    metric: Option<LoadAverageMetric>
}

impl LoadAverageMetricCollector {

    pub fn new() -> Self {
        LoadAverageMetricCollector {
            metric: None
        }
    }

    async fn save(&self, previous: &LoadAverageMetric, metric: &LoadAverageMetric, mut database: &Database, hostname: &str) -> Result<(), MetricSaveError> {
        sqlx::query!(
            "insert into metric_load_average (hostname, timestamp, one, five, fifteen) values ($1, $2, $3, $4, $5) returning hostname",
            hostname.to_string(), metric.timestamp, metric.one, metric.five, metric.fifteen
        ).fetch_one(&mut database).await?;

        Ok(())
    }
}

#[async_trait]
impl MetricCollector for LoadAverageMetricCollector {

    fn key(&self) -> String {
        "la".to_string()
    }

    async fn collect(&mut self) -> Result<(), MetricCollectionError> {
        let timestamp = Utc::now();

        let metric = read_to_string("/proc/loadavg").await
            .map_err(|err| MetricCollectionError::from(err))?;
        let mut spl = metric.split_whitespace();

        self.metric = Some(LoadAverageMetric {
            timestamp,
            one: spl.next()?.parse()?,
            five: spl.next()?.parse()?,
            fifteen: spl.next()?.parse()?,
        });

        Ok(())
    }

    async fn save(&self, mut database: &Database, hostname: &str) -> Result<(), MetricSaveError> {
        if let Some(metric) = &self.metric {
            self.save(&metric, &metric, database, hostname).await?;
        }
        Ok(())
    }

    async fn encode(&self) -> Result<String, MetricEncodingError> {
        if let Some(metric) = &self.metric {
            let v = serde_json::to_string(metric)?;
            return Ok(v);
        }

        Err(MetricEncodingError::NoRecord)
    }

    async fn cleanup(&self, mut database: &Database) -> Result<(), MetricCleanupError> {
        let min_timestamp = Utc::now() - get_max_metrics_age();

        sqlx::query!("delete from metric_load_average where timestamp < $1 returning 1 as result", min_timestamp)
            .fetch_one(&mut database).await?;

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