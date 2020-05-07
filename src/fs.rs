use std::option::NoneError;
use std::num::ParseIntError;
use std::process::Command;

use chrono::{Utc, DateTime};
use custom_error::custom_error;
use futures::future::try_join_all;
use async_trait::async_trait;

use crate::database::Database;
use std::collections::HashMap;
use crate::config::get_max_metrics_age;
use crate::types::{Metric, MetricCollectionError, MetricSaveError, MetricCleanupError, MetricCollector, MetricEncodingError};
use sqlx::{PgConnection, Pool};

#[derive(Debug, Clone)]
pub struct FilesystemUsageMetric {
    timestamp: DateTime<Utc>,
    stat: Vec<FilesystemUsageMetricEntry>
}

#[derive(Debug, Clone)]
pub struct FilesystemUsageMetricEntry {
    filesystem: String,
    total: i64,
    used: i64
}

#[async_trait]
impl Metric for FilesystemUsageMetric {
}

pub struct FilesystemMetricCollector {
    metric: Option<FilesystemUsageMetric>
}

impl FilesystemMetricCollector {

    pub fn new() -> Self {
        FilesystemMetricCollector {
            metric: None
        }
    }
}

#[async_trait]
impl MetricCollector for FilesystemMetricCollector {

    fn key(&self) -> String {
        "fs".to_string()
    }

    async fn collect(&mut self) -> Result<(), MetricCollectionError> {
        let timestamp = Utc::now();

        let stat = String::from_utf8_lossy(
            &Command::new("df").output()?.stdout
        )
            .lines()
            .skip(1)
            .map(|line| line.split_whitespace())
            .map(|mut line| Ok(FilesystemUsageMetricEntry {
                filesystem: line.next()?.to_string(),
                total: line.next()?.parse()?,
                used: line.next()?.parse()?
            }))
            .filter_map(|v: Result<FilesystemUsageMetricEntry, FilesystemUsageMetricError>| v.ok())
            .filter(|v| v.filesystem != "tmpfs" && v.filesystem != "overlay" && v.filesystem != "shm")
            .map(|v| (v.filesystem.clone(), v))
            .collect::<HashMap<String, FilesystemUsageMetricEntry>>() // deduplicate
            .into_iter()
            .map(|v| v.1)
            .collect();

        self.metric = Some(FilesystemUsageMetric { timestamp, stat });

        Ok(())
    }

    async fn save(&self, mut database: &Database, hostname: &str) -> Result<(), MetricSaveError> {
        if let Some(metric) = &self.metric {
            let timestamp = &metric.timestamp.clone();

            let futures = metric.clone().stat.into_iter()
                .map(|entry| save_metric_entry(&database, &hostname, *timestamp, entry));

            try_join_all(futures).await?;
        }

        Ok(())
    }

    async fn encode(&self) -> Result<String, MetricEncodingError> {
        if let Some(metric) = &self.metric {
            let v = serde_json::to_string(metric)?;
            Ok(v)
        }

        Err(MetricEncodingError::NoRecord)
    }

    async fn cleanup(&self, mut database: &Pool<PgConnection>) -> Result<(), MetricCleanupError> {
        let min_timestamp = Utc::now() - get_max_metrics_age();

        sqlx::query!("delete from metric_fs where timestamp < $1 returning 1 as result", min_timestamp)
            .fetch_one(&mut database).await?;

        Ok(())
    }
}

custom_error!{pub FilesystemUsageMetricError
    FailedToRead{source: std::io::Error} = "failed to read metric",
    FailedToParse{description: String} = "failed to parse metric",
    DatabaseQueryFailed{source: sqlx::error::Error} = "database query failed"
}

impl From<std::option::NoneError> for FilesystemUsageMetricError {
    fn from(_: NoneError) -> Self {
        FilesystemUsageMetricError::FailedToParse{description: "NoneError".to_string()}
    }
}

impl From<std::num::ParseIntError> for FilesystemUsageMetricError {
    fn from(err: ParseIntError) -> Self {
        FilesystemUsageMetricError::FailedToParse{description: err.to_string()}
    }
}

async fn save_metric_entry(mut database: &Database, hostname: &str, timestamp: DateTime<Utc>, entry: FilesystemUsageMetricEntry) -> Result<(), MetricSaveError> {
    sqlx::query!(
        "insert into metric_fs (hostname, timestamp, filesystem, total, used) values ($1, $2, $3, $4, $5)",
        hostname.to_string(), timestamp, entry.filesystem, entry.total, entry.used
    ).fetch_one(&mut database).await?;

    Ok(())
}
