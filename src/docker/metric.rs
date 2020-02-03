use chrono::{Utc, DateTime, Duration};
use custom_error::custom_error;
use futures::future::join_all;
use log::warn;

use crate::database::Database;
use crate::docker::client::{containers, DockerClientError, stats, Container, ContainerStats};
use futures::FutureExt;

#[derive(Debug, Clone)]
pub struct DockerContainerStats {
    timestamp: DateTime<Utc>,
    stat: Vec<DockerContainerStatEntry>
}

#[derive(Debug, Clone)]
pub struct DockerContainerStatEntry {
    name: String,
    state: String,

    cpu_usage: u64,
    system_cpu_usage: u64,

    memory_usage: u64,
    memory_cache: u64,

    network_tx: u64,
    network_rx: u64
}

#[derive(Debug, Clone)]
pub struct DockerContainerMetric {
    timestamp: DateTime<Utc>,
    stat: Vec<DockerContainerMetricEntry>
}

#[derive(Debug, Clone)]
pub struct DockerContainerMetricEntry {
    name: String,
    state: String,

    cpu_usage: f64,

    memory_usage: u64,
    memory_cache: u64,

    network_tx: f64,
    network_rx: f64,
}

custom_error!{pub DockerMetricError
    DockerClientError{source: DockerClientError} = "docker client error",
    DatabaseQueryFailed{source: sqlx::error::Error} = "database query failed"
}

pub async fn monitor_docker() -> Result<DockerContainerStats, DockerMetricError> {
    let timestamp = Utc::now();

    let stat: Vec<DockerContainerStatEntry> = join_all(containers().await?.into_iter()
        .map(|v| stats(v.id.clone()).map(|s| (v, s)))
    ).await.into_iter().filter_map(|v| match v.1 {
        Ok(stats) => Some((v.0, stats)),
        Err(err) => {
            warn!("failed to get container stats ({}): {}", v.0.id, err);
            None
        }
    }).map(|v: (Container, ContainerStats)| DockerContainerStatEntry {
        name: v.1.name[1..].to_string(),
        state: v.0.state,

        cpu_usage: (v.1.cpu_stats.cpu_usage.total_usage / 1000) as u64,
        system_cpu_usage: (v.1.cpu_stats.system_cpu_usage / 1000000) as u64,

        memory_usage: v.1.memory_stats.usage,
        memory_cache: v.1.memory_stats.stats.cache,

        network_tx: v.1.networks.iter().map(|v| v.1.tx_bytes).fold(0, |a, b| a + b),
        network_rx: v.1.networks.iter().map(|v| v.1.rx_bytes).fold(0, |a, b| a + b)
    }).collect();

    Ok(DockerContainerStats { timestamp, stat })
}

pub fn docker_metric_from_stats(first: &DockerContainerStats, second: &DockerContainerStats) -> DockerContainerMetric {
    let first = first.clone();
    let second = second.clone();
    let time_diff = second.timestamp - first.timestamp;

    let first_iter = first.stat.into_iter();

    let stat: Vec<DockerContainerMetricEntry> = second.stat.into_iter()
        .filter_map(|v| first_iter.clone()
            .find(|item| item.name == v.name)
            .map(|item| (item, v))
        )
        .map(|two_entries| docker_metric_entry_from_two_stats(time_diff, two_entries.0, two_entries.1))
        .collect();

    DockerContainerMetric { stat, timestamp: second.timestamp }
}

fn docker_metric_entry_from_two_stats(time_diff: Duration, first: DockerContainerStatEntry, second: DockerContainerStatEntry) -> DockerContainerMetricEntry {
    let diff = time_diff.num_milliseconds() as f64 / 1000.0; // seconds

    DockerContainerMetricEntry {
        name: second.name,
        state: second.state,

        cpu_usage: ((second.cpu_usage - first.cpu_usage) as f64 / (second.system_cpu_usage - first.system_cpu_usage) as f64) / diff,

        memory_usage: second.memory_usage,
        memory_cache: second.memory_cache,

        network_tx: (second.network_tx - first.network_tx) as f64 / diff,
        network_rx: (second.network_rx - first.network_rx) as f64 / diff
    }
}

pub async fn save_docker_metric(mut database: &Database, hostname: &str, metric: &DockerContainerMetric) {
    let metric = metric.clone();

    let timestamp = metric.timestamp.clone();

    let futures = metric.stat.into_iter()
        .map(|entry| save_metric_entry(&mut database, hostname, &timestamp, entry));

    join_all(futures).await;
}

async fn save_metric_entry(mut database: &Database, hostname: &str, timestamp: &DateTime<Utc>, entry: DockerContainerMetricEntry) -> Result<(), DockerMetricError> {
    sqlx::query!(
        "insert into metric_docker_containers (hostname, timestamp, name, state, cpu_usage, memory_usage, memory_cache, network_tx, network_rx) values ($1, $2, $3, $4, $5, $6, $7, $8, $9) returning name",
        hostname.to_string(), *timestamp, entry.name, entry.state, entry.cpu_usage, entry.memory_usage as i64, entry.memory_cache as i64, entry.network_tx, entry.network_rx
    ).fetch_one(&mut database).await?;

    Ok(())
}