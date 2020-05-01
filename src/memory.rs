use std::option::NoneError;
use std::num::ParseFloatError;

use chrono::{Utc, DateTime};
use async_std::fs::read_to_string;
use custom_error::custom_error;
use std::collections::HashMap;
use async_trait::async_trait;

use crate::database::Database;
use crate::config::get_max_metrics_age;
use crate::types::{Metric, MetricCollectionError, MetricSaveError, MetricCleanupError, MetricCollector};
use sqlx::{PgConnection, Pool};

pub struct MemoryMetric {
    timestamp: DateTime<Utc>,
    total: Option<i64>,
    free: Option<i64>,
    available: Option<i64>,
    buffers: Option<i64>,
    cached: Option<i64>,
    swap_total: Option<i64>,
    swap_free: Option<i64>
}

impl Metric for MemoryMetric {
}

pub struct MemoryMetricCollector {
    metric: Option<MemoryMetric>
}

impl MemoryMetricCollector {

    pub fn new() -> Self {
        MemoryMetricCollector {
            metric: None
        }
    }

    async fn save_metric(&self, previous: &MemoryMetric, metric: &MemoryMetric, mut database: &Database, hostname: &str) -> Result<(), MetricSaveError> {
        sqlx::query!(
            "insert into metric_memory (hostname, timestamp, total, free, available, buffers, cached, swap_total, swap_free) values ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
            hostname.to_string(), metric.timestamp, metric.total.unwrap_or(0), metric.free.unwrap_or(0),
            metric.available.unwrap_or(0), metric.buffers.unwrap_or(0), metric.cached.unwrap_or(0),
            metric.swap_total.unwrap_or(0), metric.swap_free.unwrap_or(0)
        ).fetch_one(&mut database).await?;

        Ok(())
    }
}

#[async_trait]
impl MetricCollector for MemoryMetricCollector {

    fn key(&self) -> String {
        "memory".to_string()
    }

    async fn collect(&mut self) -> Result<(), MetricCollectionError> {
        let timestamp = Utc::now();

        let data: String = read_to_string("/proc/meminfo").await.map_err(|err| MetricCollectionError::from(err))?;
        let mut stats: HashMap<String, i64> = data.lines().into_iter().filter_map(|v| {
            let mut spl = v.split_whitespace();
            let label = spl.next();
            let value = spl.next().into_iter().filter_map(|v| v.parse::<i64>().ok()).next();

            if label.is_some() && value.is_some() {
                Some((label.unwrap().to_string(), value.unwrap()))
            } else {
                None
            }
        }).collect();

        self.metric = Some(MemoryMetric {
            timestamp,
            total: stats.remove("MemTotal:"),
            free: stats.remove("MemFree:"),
            available: stats.remove("MemAvailable:"),
            buffers: stats.remove("Buffers:"),
            cached: stats.remove("Cached:"),
            swap_total: stats.remove("SwapTotal:"),
            swap_free: stats.remove("SwapFree:")
        });

        Ok(())
    }

    async fn save(&self, mut database: &Database, hostname: &str) -> Result<(), MetricSaveError> {
        if let Some (metric) = &self.metric {
            self.save_metric(metric, metric, database, hostname).await?;
        }

        Ok(())
    }

    async fn cleanup(&self, mut database: &Database) -> Result<(), MetricCleanupError> {
        let min_timestamp = Utc::now() - get_max_metrics_age();

        sqlx::query!("delete from metric_memory where timestamp < $1 returning 1 as result", min_timestamp)
            .fetch_one(&mut database).await?;

        Ok(())
    }
}

custom_error! {pub MemoryMetricError
    FailedToRead{source: std::io::Error} = "failed to read metric",
    FailedToParse = "failed to parse metric",
    DatabaseQueryFailed{source: sqlx::error::Error} = "database query failed"
}

impl From<std::option::NoneError> for MemoryMetricError {
    fn from(_: NoneError) -> Self {
        MemoryMetricError::FailedToParse
    }
}

impl From<std::num::ParseFloatError> for MemoryMetricError {
    fn from(_: ParseFloatError) -> Self {
        MemoryMetricError::FailedToParse
    }
}
