use std::str::SplitWhitespace;
use std::option::NoneError;
use std::num::ParseIntError;

use async_std::fs::read_to_string;
use custom_error::custom_error;
use futures::future::try_join_all;
use chrono::{Utc, DateTime, Duration};
use async_trait::async_trait;
use serde::Serialize;

use crate::database::Database;
use std::env;
use crate::config::get_max_metrics_age;
use crate::types::{Metric, MetricCollectionError, MetricSaveError, MetricCleanupError, MetricCollector, MetricEncodingError};
use sqlx::{PgConnection, Pool};

#[derive(Debug, Clone)]
pub struct InstantNetworkMetric {
    timestamp: DateTime<Utc>,
    stat: Vec<InstantNetworkMetricEntry>,
}

#[derive(Debug, Clone)]
pub struct InstantNetworkMetricEntry {
    device: String,
    rx: u64,
    tx: u64
}

#[derive(Debug, Clone, Serialize)]
pub struct NetworkMetric {
    timestamp: DateTime<Utc>,
    stat: Vec<NetworkMetricEntry>
}

#[derive(Debug, Clone, Serialize)]
pub struct NetworkMetricEntry {
    device: String,
    rx: f64,
    tx: f64
}

#[async_trait]
impl Metric for InstantNetworkMetric {
}

pub struct NetworkMetricCollector {
    previous: Option<InstantNetworkMetric>,
    metric: Option<NetworkMetric>
}

impl NetworkMetricCollector {

    pub fn new() -> Self {
        NetworkMetricCollector {
            previous: None,
            metric: None
        }
    }

    async fn collect_metric(&self) -> Result<Box<InstantNetworkMetric>, MetricCollectionError> {
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

                Ok(InstantNetworkMetricEntry {
                    device,
                    rx,
                    tx
                })
            })
            .filter_map(|v: Result<InstantNetworkMetricEntry, NetworkMetricError>| v.ok())
            .collect();

        Ok(Box::new(InstantNetworkMetric { stat, timestamp }))
    }
}

#[async_trait]
impl MetricCollector for NetworkMetricCollector {

    fn key(&self) -> String {
        "network".to_string()
    }

    async fn collect(&mut self) -> Result<(), MetricCollectionError> {
        let metric = self.collect_metric().await?;
        if let Some(prev) = &self.previous {
            self.metric = Some(network_metric_from_stats(prev, &metric));
        }
        self.previous = Some(*metric);
        Ok(())
    }

    async fn save(&self, mut database: &Database, hostname: &str) -> Result<(), MetricSaveError> {
        if let Some(metric) = &self.metric {
            let timestamp = metric.timestamp.clone();

            let futures = metric.clone().stat.into_iter()
                .map(|entry| save_metric_entry(&database, hostname, &timestamp, entry));

            try_join_all(futures).await?;
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

        sqlx::query!("delete from metric_network where timestamp < $1 returning 1 as result", min_timestamp)
            .fetch_one(&mut database).await?;

        Ok(())
    }
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

fn network_metric_from_stats(first: &InstantNetworkMetric, second: &InstantNetworkMetric) -> NetworkMetric {
    let time_diff = second.timestamp - first.timestamp;

    let first_iter = first.stat.clone().into_iter();

    let stat: Vec<NetworkMetricEntry> = second.stat.clone().into_iter()
        .filter_map(|v| first_iter.clone()
            .find(|item| item.device == v.device)
            .map(|item| network_metric_from_two_stats(time_diff, item, v))
        ).collect();

    NetworkMetric { stat, timestamp: second.timestamp }
}

fn network_metric_from_two_stats(time_diff: Duration, first: InstantNetworkMetricEntry, second: InstantNetworkMetricEntry) -> NetworkMetricEntry {
    let diff = time_diff.num_milliseconds() as f64 / 1000.0; // seconds

    let rx = (second.rx - first.rx) as f64 / diff;
    let tx = (second.tx - first.tx) as f64 / diff;

    NetworkMetricEntry {
        device: second.device,
        rx,
        tx
    }
}

async fn save_metric_entry(mut database: &Database, hostname: &str, timestamp: &DateTime<Utc>, entry: NetworkMetricEntry) -> Result<(), MetricSaveError> {
    sqlx::query!(
        "insert into metric_network (hostname, timestamp, device, rx, tx) values ($1, $2, $3, $4, $5)",
        hostname.to_string(), *timestamp, entry.device.to_string(), entry.rx, entry.tx
    ).fetch_one(&mut database).await?;

    Ok(())
}
