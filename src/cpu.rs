use std::str::SplitWhitespace;
use std::option::NoneError;
use std::num::ParseIntError;

use async_std::fs::read_to_string;
use custom_error::custom_error;
use futures::future::try_join_all;
use chrono::{self, Utc, DateTime, Duration};
use async_trait::async_trait;
use sqlx::{PgConnection, Pool};

use crate::database::Database;
use crate::config::get_max_metrics_age;
use crate::types::{Metric, MetricCollectionError, MetricSaveError, MetricCleanupError, MetricCollector, MetricEncodingError};

#[derive(Debug, Clone)]
pub struct InstantCPUMetric  {
    timestamp: DateTime<Utc>,
    stat: Vec<InstantCPUMetricEntry>,
}

#[derive(Debug, Copy, Clone)]
pub struct InstantCPUMetricEntry {
    cpu: u16,
    user: u64,
    nice: u64,
    system: u64,
    idle: u64,
    iowait: u64,
    irq: u64,
    softirq: u64,
    guest: u64,
    steal: u64,
    guest_nice: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CPUMetric {
    timestamp: DateTime<Utc>,
    stat: Vec<CPUMetricEntry>
}

#[derive(Debug, Copy, Clone, Serialize)]
pub struct CPUMetricEntry {
    cpu: u16,
    user: u64,
    nice: u64,
    system: u64,
    idle: u64,
    iowait: u64,
    irq: u64,
    softirq: u64,
    guest: u64,
    steal: u64,
    guest_nice: u64,
}

custom_error! {pub CPUMetricError
    FailedToRead{source: std::io::Error} = "failed to read metric",
    FailedToParse{description: String} = "failed to parse metric",
    FailedToGetTimeDiff{source: std::time::SystemTimeError} = "failed to get time diff",
    DatabaseQueryFailed{source: sqlx::error::Error} = "database query failed"
}

impl From<std::option::NoneError> for CPUMetricError {
    fn from(_: NoneError) -> Self {
        CPUMetricError::FailedToParse{description: "NoneError".to_string()}
    }
}

impl From<std::num::ParseIntError> for CPUMetricError {
    fn from(err: ParseIntError) -> Self {
        CPUMetricError::FailedToParse{description: err.to_string()}
    }
}

impl Metric for InstantCPUMetric {
}

pub struct CpuMetricCollector {
    previous: Option<InstantCPUMetric>,
    metric: Option<CPUMetric>
}

impl CpuMetricCollector {

    pub fn new() -> CpuMetricCollector {
        CpuMetricCollector {
            previous: None,
            metric: None
        }
    }
}

#[async_trait]
impl MetricCollector for CpuMetricCollector {

    fn key(&self) -> String {
        "cpu".to_string()
    }

    async fn collect(&mut self) -> Result<(), MetricCollectionError> {
        let timestamp = Utc::now();

        let stat = read_to_string("/proc/stat").await?.lines()
            .map(|line| line.split_whitespace())
            .filter(|spl| is_cpu_line(spl).unwrap_or(false))
            .map(|mut spl: SplitWhitespace| Ok(InstantCPUMetricEntry {
                cpu: spl.next()?[3..].parse()?,
                user: spl.next()?.parse()?,
                nice: spl.next()?.parse()?,
                system: spl.next()?.parse()?,
                idle: spl.next()?.parse()?,
                iowait: spl.next()?.parse()?,
                irq: spl.next()?.parse()?,
                softirq: spl.next()?.parse()?,
                guest: spl.next()?.parse()?,
                steal: spl.next()?.parse()?,
                guest_nice: spl.next()?.parse()?
            }))
            .filter_map(|v: Result<InstantCPUMetricEntry, MetricCollectionError>| v.ok())
            .collect();

        let metric = InstantCPUMetric { stat, timestamp };

        if let Some(prev) = &self.previous {
            self.metric = Some(cpu_metric_from_stats(prev, &metric));
        }

        self.previous = Some(metric);

        Ok(())
    }

    async fn save(&self, mut database: &Database, hostname: &str) -> Result<(), MetricSaveError> {
        if let Some(metric) = &self.metric {
            let timestamp = metric.timestamp.clone();

            let futures = metric.clone().stat.into_iter()
                .map(|entry| save_metric_entry(database, &hostname, timestamp, entry));

            try_join_all(futures).await?;
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

    async fn cleanup(&self, mut database: &Pool<PgConnection>) -> Result<(), MetricCleanupError> {
        let min_timestamp = Utc::now() - get_max_metrics_age();

        sqlx::query!("delete from metric_cpu where timestamp < $1 returning 1 as result", min_timestamp)
            .fetch_one(&mut database).await?;

        Ok(())
    }
}

fn is_cpu_line(spl: &SplitWhitespace) -> Result<bool, CPUMetricError> {
    let mut spl_clone = spl.clone();
    let first_word: &str = spl_clone.nth(0)?;
    Ok(first_word.starts_with("cpu") && first_word.len() > 3 && spl_clone.count() == 10)
}

fn cpu_metric_from_stats(first: &InstantCPUMetric, second: &InstantCPUMetric) -> CPUMetric {
    let time_diff = second.clone().timestamp - first.clone().timestamp;

    let first_iter = first.clone().stat.into_iter();

    let stat: Vec<CPUMetricEntry> = second.clone().stat.into_iter()
        .filter_map(|v| first_iter.clone()
            .find(|item| item.cpu == v.cpu)
            .map(|item| (item, v))
        )
        .map(|two_entries| cpu_metric_entry_from_two_stats(time_diff, two_entries.0, two_entries.1))
        .collect();

    CPUMetric { stat, timestamp: second.timestamp }
}

fn cpu_metric_entry_from_two_stats(time_diff: Duration, first: InstantCPUMetricEntry, second: InstantCPUMetricEntry) -> CPUMetricEntry {
    let diff = time_diff.num_milliseconds() as f64 / 1000.0;

    CPUMetricEntry {
        cpu: second.cpu,
        user: ((second.user - first.user) as f64 / diff) as u64,
        nice: ((second.nice - first.nice) as f64 / diff) as u64,
        system: ((second.system - first.system) as f64 / diff) as u64,
        idle: ((second.idle - first.idle) as f64 / diff) as u64,
        iowait: ((second.iowait - first.iowait) as f64 / diff) as u64,
        irq: ((second.irq - first.irq) as f64/ diff) as u64,
        softirq: ((second.softirq - first.softirq) as f64 / diff) as u64,
        guest: ((second.guest - first.guest) as f64 / diff) as u64,
        steal: ((second.steal - first.steal) as f64 / diff) as u64,
        guest_nice: ((second.guest_nice - first.guest_nice) as f64 / diff) as u64,
    }
}

async fn save_metric_entry(mut database: &Database, hostname: &str, timestamp: DateTime<Utc>, entry: CPUMetricEntry) -> Result<(), MetricSaveError> {
    sqlx::query!(
        "insert into metric_cpu (hostname, timestamp, cpu, \"user\", nice, system, idle, iowait, irq, softirq, guest, steal, guest_nice) values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) returning cpu",
        hostname.to_string(), timestamp,
        entry.cpu as i32, entry.user as i32, entry.nice as i32, entry.system as i32, entry.idle as i32,
        entry.iowait as i32, entry.irq as i32, entry.softirq as i32, entry.guest as i32, entry.steal as i32,
        entry.guest_nice as i32
    ).fetch_one(&mut database).await?;

    Ok(())
}