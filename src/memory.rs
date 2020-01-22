use std::option::NoneError;
use std::num::ParseFloatError;

use chrono::{Utc, DateTime};
use async_std::fs::read_to_string;
use custom_error::custom_error;
use std::collections::HashMap;
use crate::database::Database;

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

custom_error! {pub MemoryMetricError
    FailedToRead{source: std::io::Error} = "failed to read metric",
    FailedToParse = "failed to parse metric"
}

impl From<std::option::NoneError> for MemoryMetricError {
    fn from(err: NoneError) -> Self {
        MemoryMetricError::FailedToParse
    }
}

impl From<std::num::ParseFloatError> for MemoryMetricError {
    fn from(err: ParseFloatError) -> Self {
        MemoryMetricError::FailedToParse
    }
}

pub async fn monitor_memory() -> Result<MemoryMetric, MemoryMetricError> {
    let timestamp = Utc::now();

    let data: String = read_to_string("/proc/meminfo").await?;
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

    Ok(MemoryMetric {
        timestamp,
        total: stats.remove("MemTotal:"),
        free: stats.remove("MemFree:"),
        available: stats.remove("MemAvailable:"),
        buffers: stats.remove("Buffers:"),
        cached: stats.remove("Cached:"),
        swap_total: stats.remove("SwapTotal:"),
        swap_free: stats.remove("SwapFree:")
    })
}

pub async fn save_memory_metric(mut database: &Database, hostname: &str, metric: &MemoryMetric) {
    sqlx::query!(
        "insert into metric_memory (hostname, timestamp, total, free, available, buffers, cached, swap_total, swap_free) values ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        hostname.to_string(), metric.timestamp, metric.total.unwrap_or(0), metric.free.unwrap_or(0),
        metric.available.unwrap_or(0), metric.buffers.unwrap_or(0), metric.cached.unwrap_or(0),
        metric.swap_total.unwrap_or(0), metric.swap_free.unwrap_or(0)
    ).fetch_one(&mut database).await;
}