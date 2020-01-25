use std::option::NoneError;
use std::num::ParseIntError;
use std::process::Command;

use chrono::{Utc, DateTime};
use custom_error::custom_error;
use futures::future::join_all;

use crate::database::Database;

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

custom_error!{pub FilesystemUsageMetricError
    FailedToRead{source: std::io::Error} = "failed to read metric",
    FailedToParse = "failed to parse metric"
}

impl From<std::option::NoneError> for FilesystemUsageMetricError {
    fn from(err: NoneError) -> Self {
        FilesystemUsageMetricError::FailedToParse
    }
}

impl From<std::num::ParseIntError> for FilesystemUsageMetricError {
    fn from(err: ParseIntError) -> Self {
        FilesystemUsageMetricError::FailedToParse
    }
}

pub async fn monitor_filesystem_usage() -> Result<FilesystemUsageMetric, FilesystemUsageMetricError> {
    let timestamp = Utc::now();

    let stat = String::from_utf8_lossy(
        &Command::new("df")
            .arg("-x")
            .arg("squashfs")
            .arg("-x")
            .arg("devtmpfs")
            .arg("-x")
            .arg("tmpfs")
            .arg("-x")
            .arg("fuse")
            .arg("--output=source,size,used")
            .output()?.stdout
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
        .collect();

    Ok(FilesystemUsageMetric { timestamp, stat })
}

pub async fn save_filesystem_usage_metric(mut database: &Database, hostname: &str, metric: &FilesystemUsageMetric) {
    let metric = metric.clone();
    let timestamp = metric.timestamp.clone();

    let futures = metric.stat.into_iter()
        .map(|entry| save_metric_entry(&database, &hostname, timestamp, entry));

    join_all(futures).await;
}

async fn save_metric_entry(mut database: &Database, hostname: &str, timestamp: DateTime<Utc>, entry: FilesystemUsageMetricEntry) {
    sqlx::query!(
        "insert into metric_fs (hostname, timestamp, filesystem, total, used) values ($1, $2, $3, $4, $5)",
        hostname.to_string(), timestamp, entry.filesystem, entry.total, entry.used
    ).fetch_one(&mut database).await;
}