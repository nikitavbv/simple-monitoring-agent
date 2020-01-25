use std::str::SplitWhitespace;
use std::option::NoneError;
use std::num::ParseIntError;

use async_std::fs::read_to_string;
use custom_error::custom_error;
use futures::future::join_all;
use chrono::{Utc, DateTime, Duration};
use sqlx::error::Error as SQLXError;
use log::warn;

use crate::database::Database;
use std::error::Error;

#[derive(Debug, Clone)]
pub struct IOStat {
    timestamp: DateTime<Utc>,
    stat: Vec<IOStatEntry>,
}

#[derive(Debug, Clone)]
pub struct IOStatEntry {
    device_name: String,
    sectors_read: u64,
    sectors_written: u64
}

#[derive(Debug, Clone)]
pub struct IOMetric {
    timestamp: DateTime<Utc>,
    stat: Vec<IOMetricEntry>
}

#[derive(Debug, Clone)]
pub struct IOMetricEntry {
    device: String,
    read: f64,
    write: f64
}

custom_error!{pub IOMetricError
    FailedToRead{source: std::io::Error} = "failed to read metric",
    FailedToParse = "failed to parse metric"
}

impl From<std::option::NoneError> for IOMetricError {
    fn from(err: NoneError) -> Self {
        IOMetricError::FailedToParse
    }
}

impl From<std::num::ParseIntError> for IOMetricError {
    fn from(err: ParseIntError) -> Self {
        IOMetricError::FailedToParse
    }
}

// Block size according to:
// https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git
// /tree/include/linux/types.h?id=v4.4-rc6#n121
const DEVICE_BLOCK_SIZE: i32 = 512;

pub async fn monitor_io() -> Result<IOStat, IOMetricError> {
    let timestamp = Utc::now();

    let stat = read_to_string("/proc/diskstats").await?.lines()
        .map(|line| line.split_whitespace())
        .map(|s: SplitWhitespace| {
            let mut spl = s.clone();
            let mut spl = spl.skip(2);

            let device_name = spl.next()?.to_string();
            let mut spl = spl.skip(2);

            let sectors_read = spl.next()?.parse()?;
            let sectors_written = spl.next()?.parse()?;

            Ok(IOStatEntry {
                device_name,
                sectors_read,
                sectors_written
            })
        })
        .filter_map(|v: Result<IOStatEntry, IOMetricError>| v.ok())
        .collect();

    Ok(IOStat { stat, timestamp })
}

pub fn io_metric_from_stats(first: IOStat, second: IOStat) -> IOMetric {
    let time_diff = second.timestamp - first.timestamp;

    let first_iter = first.stat.into_iter();

    let stat: Vec<IOMetricEntry> = second.stat.into_iter()
        .filter_map(|v| first_iter.clone()
            .find(|item| item.device_name == v.device_name)
            .map(|item| (item, v))
        )
        .map(|two_entries| io_metric_entry_from_two_stats(time_diff, two_entries.0, two_entries.1))
        .collect();

    IOMetric { stat, timestamp: second.timestamp }
}

fn io_metric_entry_from_two_stats(time_diff: Duration, first: IOStatEntry, second: IOStatEntry) -> IOMetricEntry {
    let diff = time_diff.num_milliseconds() as f64 / 1000.0; // seconds

    let read = ((second.sectors_read - first.sectors_read) * DEVICE_BLOCK_SIZE as u64) as f64 / diff;
    let write = ((second.sectors_written - first.sectors_written) * DEVICE_BLOCK_SIZE as u64) as f64 / diff;

    IOMetricEntry {
        device: second.device_name,
        read,
        write
    }
}

pub async fn save_io_metric(mut database: &Database, hostname: &str, metric: &IOMetric) {
    let metric = metric.clone();

    let timestamp = metric.timestamp.clone();

    let futures = metric.stat.into_iter()
        .map(|entry| save_metric_entry(&database, hostname, &timestamp, entry));

    join_all(futures).await;
}

async fn save_metric_entry(mut database: &Database, hostname: &str, timestamp: &DateTime<Utc>, entry: IOMetricEntry) {
    sqlx::query!(
        "insert into metric_io (hostname, timestamp, device, read, write) values ($1, $2, $3, $4, $5) returning hostname",
        hostname.to_string(), *timestamp, entry.device.to_string(), entry.read, entry.write
    ).fetch_one(&mut database).await;
}