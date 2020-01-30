use std::str::SplitWhitespace;
use std::option::NoneError;
use std::num::ParseIntError;

use async_std::fs::read_to_string;
use custom_error::custom_error;
use futures::future::join_all;
use chrono::{Utc, DateTime, Duration};

use crate::database::Database;
use std::env;

#[derive(Debug, Clone)]
pub struct NetworkStat {
    timestamp: DateTime<Utc>,
    stat: Vec<NetworkStatEntry>,
}

#[derive(Debug, Clone)]
pub struct NetworkStatEntry {
    device: String,
    rx: u64,
    tx: u64
}

#[derive(Debug, Clone)]
pub struct NetworkMetric {
    timestamp: DateTime<Utc>,
    stat: Vec<NetworkMetricEntry>
}

#[derive(Debug, Clone)]
pub struct NetworkMetricEntry {
    device: String,
    rx: f64,
    tx: f64
}

custom_error!{pub NetworkMetricError
    FailedToRead{source: std::io::Error} = "failed to read metric",
    FailedToParse{description: String} = "failed to parse metric",
    DatabaseQueryFailed{source: sqlx::error::Error} = "database query failed"
}

impl From<std::option::NoneError> for NetworkMetricError {
    fn from(_: NoneError) -> Self {
        NetworkMetricError::FailedToParse{description: "NoneError".to_string()}
    }
}

impl From<std::num::ParseIntError> for NetworkMetricError {
    fn from(err: ParseIntError) -> Self {
        NetworkMetricError::FailedToParse{description: err.to_string()}
    }
}

fn network_stats_file_name() -> String {
    env::var("NETWORK_STATS_FILE").unwrap_or("/proc/net/dev".to_string())
}

pub async fn monitor_network() -> Result<NetworkStat, NetworkMetricError> {
    let timestamp = Utc::now();

    let stat = read_to_string(&network_stats_file_name()).await?.lines()
        .skip(2)
        .map(|line| line.split_whitespace())
        .map(|spl: SplitWhitespace| {
            let mut spl = spl.clone();

            let device = spl.next()?.to_string();
            let rx = spl.next()?.parse()?;

            let mut spl = spl.skip(7);
            let tx = spl.next()?.parse()?;

            Ok(NetworkStatEntry {
                device,
                rx,
                tx
            })
        })
        .filter_map(|v: Result<NetworkStatEntry, NetworkMetricError>| v.ok())
        .collect();

    Ok(NetworkStat { stat, timestamp })
}

pub fn network_metric_from_stats(first: &NetworkStat, second: &NetworkStat) -> NetworkMetric {
    let time_diff = second.timestamp - first.timestamp;

    let first_iter = first.stat.clone().into_iter();

    let stat: Vec<NetworkMetricEntry> = second.stat.clone().into_iter()
        .filter_map(|v| first_iter.clone()
            .find(|item| item.device == v.device)
            .map(|item| network_metric_from_two_stats(time_diff, item, v))
        ).collect();

    NetworkMetric { stat, timestamp: second.timestamp }
}

fn network_metric_from_two_stats(time_diff: Duration, first: NetworkStatEntry, second: NetworkStatEntry) -> NetworkMetricEntry {
    let diff = time_diff.num_milliseconds() as f64 / 1000.0; // seconds

    let rx = (second.rx - first.rx) as f64 / diff;
    let tx = (second.tx - first.tx) as f64 / diff;

    NetworkMetricEntry {
        device: second.device,
        rx,
        tx
    }
}

pub async fn save_network_metric(database: &Database, hostname: &str, metric: &NetworkMetric) {
    let metric = metric.clone();
    let timestamp = metric.timestamp.clone();

    let futures = metric.stat.into_iter()
        .map(|entry| save_metric_entry(&database, hostname, &timestamp, entry));

    join_all(futures).await;
}

async fn save_metric_entry(mut database: &Database, hostname: &str, timestamp: &DateTime<Utc>, entry: NetworkMetricEntry) -> Result<(), NetworkMetricError> {
    sqlx::query!(
        "insert into metric_network (hostname, timestamp, device, rx, tx) values ($1, $2, $3, $4, $5)",
        hostname.to_string(), *timestamp, entry.device.to_string(), entry.rx, entry.tx
    ).fetch_one(&mut database).await?;

    Ok(())
}