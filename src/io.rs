use std::str::SplitWhitespace;
use std::option::NoneError;
use std::num::ParseIntError;

use async_std::fs::read_to_string;
use custom_error::custom_error;
use futures::future::join_all;
use chrono::{Utc, DateTime, Duration};
use async_trait::async_trait;
use serde::Serialize;

use crate::database::Database;
use crate::config::get_max_metrics_age;
use crate::types::{Metric, MetricCollectionError, MetricSaveError, MetricCleanupError, MetricCollector, MetricEncodingError};

#[derive(Debug, Clone)]
pub struct InstantIOMetric {
    timestamp: DateTime<Utc>,
    stat: Vec<InstantIOMetricEntry>,
}

#[derive(Debug, Clone)]
pub struct InstantIOMetricEntry {
    device_name: String,
    sectors_read: u64,
    sectors_written: u64
}

#[derive(Debug, Clone, Serialize)]
pub struct IOMetric {
    timestamp: DateTime<Utc>,
    stat: Vec<IOMetricEntry>
}

#[derive(Debug, Clone, Serialize)]
pub struct IOMetricEntry {
    device: String,
    read: f64,
    write: f64
}

impl Metric for InstantIOMetric {
}

pub struct IOMetricCollector {
    previous: Option<InstantIOMetric>,
    metric: Option<IOMetric>
}

impl IOMetricCollector {

    pub fn new() -> Self {
        IOMetricCollector {
            previous: None,
            metric: None
        }
    }

    async fn collect_metric(&self) -> Result<Box<InstantIOMetric>, MetricCollectionError> {
        let timestamp = Utc::now();

        let stat = read_to_string("/proc/diskstats").await?.lines()
            .map(|line| line.split_whitespace())
            .map(|s: SplitWhitespace| {
                let spl = s.clone();
                let mut spl = spl.skip(2);

                let device_name = spl.next()?.to_string();
                let mut spl = spl.skip(2);

                let sectors_read = spl.next()?.parse()?;
                let sectors_written = spl.next()?.parse()?;

                Ok(InstantIOMetricEntry {
                    device_name,
                    sectors_read,
                    sectors_written
                })
            })
            .filter_map(|v: Result<InstantIOMetricEntry, IOMetricError>| v.ok())
            .collect();

        Ok(Box::new(InstantIOMetric { stat, timestamp }))
    }
}

#[async_trait]
impl MetricCollector for IOMetricCollector {

    fn key(&self) -> String {
        "io".to_string()
    }

    async fn collect(&mut self) -> Result<(), MetricCollectionError> {
        let metric = self.collect_metric().await?;
        if let Some(previous) = &self.previous {
            self.metric = Some(io_metric_from_stats(previous, &metric));
        }

        self.previous = Some(*metric);
        Ok(())
    }

    async fn save(&self, mut database: &Database, hostname: &str) -> Result<(), MetricSaveError> {
        if let Some(metric) = &self.metric {
            save_io_metric(&database, hostname, &metric).await;
        }

        Ok(())
    }

    async fn encode(&self) -> Result<String, MetricEncodingError> {
        if let Some(metric) = &self.metric {
            let v = serde_json::to_string(metric)?;
            return Ok(v)
        }

        Err(MetricEncodingError::NoRecord)
    }

    async fn cleanup(&self, mut database: &Database) -> Result<(), MetricCleanupError> {
        let min_timestamp = Utc::now() - get_max_metrics_age();

        sqlx::query!("delete from metric_io where timestamp < $1 returning 1 as result", min_timestamp)
            .fetch_one(&mut database).await?;

        Ok(())
    }
}

custom_error!{pub IOMetricError
    FailedToRead{source: std::io::Error} = "failed to read metric",
    FailedToParse{description: String} = "failed to parse metric",
    DatabaseQueryFailed{source: sqlx::error::Error} = "database query failed"
}

impl From<std::option::NoneError> for IOMetricError {
    fn from(_: NoneError) -> Self {
        IOMetricError::FailedToParse{description: "NoneError".to_string()}
    }
}

impl From<std::num::ParseIntError> for IOMetricError {
    fn from(err: ParseIntError) -> Self {
        IOMetricError::FailedToParse{description: err.to_string()}
    }
}

// Block size according to:
// https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git
// /tree/include/linux/types.h?id=v4.4-rc6#n121
const DEVICE_BLOCK_SIZE: i32 = 512;

fn io_metric_from_stats(first: &InstantIOMetric, second: &InstantIOMetric) -> IOMetric {
    let time_diff = second.timestamp - first.timestamp;

    let first_iter = first.clone().stat.into_iter();

    let stat: Vec<IOMetricEntry> = second.clone().stat.into_iter()
        .filter_map(|v| first_iter.clone()
            .find(|item| item.device_name == v.device_name)
            .map(|item| (item, v))
        )
        .map(|two_entries| io_metric_entry_from_two_stats(time_diff, two_entries.0, two_entries.1))
        .collect();

    IOMetric { stat, timestamp: second.timestamp }
}

fn io_metric_entry_from_two_stats(time_diff: Duration, first: InstantIOMetricEntry, second: InstantIOMetricEntry) -> IOMetricEntry {
    let diff = time_diff.num_milliseconds() as f64 / 1000.0; // seconds

    let read = ((second.sectors_read - first.sectors_read) * DEVICE_BLOCK_SIZE as u64) as f64 / diff;
    let write = ((second.sectors_written - first.sectors_written) * DEVICE_BLOCK_SIZE as u64) as f64 / diff;

    IOMetricEntry {
        device: second.device_name,
        read,
        write
    }
}

async fn save_io_metric(database: &Database, hostname: &str, metric: &IOMetric) {
    let metric = metric.clone();

    let timestamp = metric.timestamp.clone();

    let futures = metric.stat.into_iter()
        .map(|entry| save_metric_entry(&database, hostname, &timestamp, entry));

    join_all(futures).await;
}

async fn save_metric_entry(mut database: &Database, hostname: &str, timestamp: &DateTime<Utc>, entry: IOMetricEntry) -> Result<(), IOMetricError> {
    sqlx::query!(
        "insert into metric_io (hostname, timestamp, device, read, write) values ($1, $2, $3, $4, $5) returning hostname",
        hostname.to_string(), *timestamp, entry.device.to_string(), entry.read, entry.write
    ).fetch_one(&mut database).await?;

    Ok(())
}
