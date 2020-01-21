use std::str::SplitWhitespace;
use std::option::NoneError;
use std::num::ParseIntError;

use async_std::fs::read_to_string;
use custom_error::custom_error;
use futures::future::join_all;
use chrono::{Utc, DateTime, Duration};

use crate::database::Database;

#[derive(Debug, Clone)]
pub struct CPUStat  {
    timestamp: DateTime<Utc>,
    stat: Vec<CPUStatEntry>,
}

#[derive(Debug, Copy, Clone)]
pub struct CPUStatEntry {
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

#[derive(Debug)]
pub struct CPUMetric {
    timestamp: DateTime<Utc>,
    stat: Vec<CPUMetricEntry>
}

#[derive(Debug)]
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

custom_error!{pub CPUMetricError
    FailedToRead{source: std::io::Error} = "failed to read metric",
    FailedToParse = "failed to parse metric",
    FailedToGetTimeDiff{source: std::time::SystemTimeError} = "failed to get time diff",
}

impl From<std::option::NoneError> for CPUMetricError {
    fn from(err: NoneError) -> Self {
        CPUMetricError::FailedToParse
    }
}

impl From<std::num::ParseIntError> for CPUMetricError {
    fn from(err: ParseIntError) -> Self {
        CPUMetricError::FailedToParse
    }
}

pub async fn monitor_cpu_usage() -> Result<CPUStat, CPUMetricError> {
    let timestamp = Utc::now();

    let stat = read_to_string("/proc/stat").await?.lines()
        .map(|line| line.split_whitespace())
        .filter(is_cpu_line)
        .map(|mut spl: SplitWhitespace| Ok(CPUStatEntry {
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
        .filter_map(|v: Result<CPUStatEntry, CPUMetricError>| v.ok())
        .collect();

    Ok(CPUStat { stat, timestamp })
}

fn is_cpu_line(spl: &SplitWhitespace) -> bool {
    let mut spl_clone = spl.clone();
    let first_word: &str = spl_clone.nth(0).expect("expected spl to contain at least one word");
    first_word.starts_with("cpu") && first_word.len() > 3 && spl_clone.count() == 10
}

pub fn cpu_metric_from_stats(first: CPUStat, second: CPUStat) -> CPUMetric {
    let time_diff = second.timestamp - first.timestamp;

    let first_iter = first.stat.into_iter();

    let stat: Vec<CPUMetricEntry> = second.stat.into_iter()
        .filter_map(|v| first_iter.clone()
            .find(|item| item.cpu == v.cpu)
            .map(|item| (item, v))
        )
        .map(|two_entries| cpu_metric_entry_from_two_stats(time_diff, two_entries.0, two_entries.1))
        .collect();

    CPUMetric { stat, timestamp: second.timestamp }
}

fn cpu_metric_entry_from_two_stats(time_diff: Duration, first: CPUStatEntry, second: CPUStatEntry) -> CPUMetricEntry {
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

pub async fn save_cpu_metric(mut database: Database, hostname: String, metric: CPUMetric) {
    let timestamp = metric.timestamp.clone();

    let futures = metric.stat.into_iter()
        .map(|entry| save_metric_entry(database.clone(), &hostname, timestamp, entry));

    join_all(futures).await;
}

async fn save_metric_entry(mut database: Database, hostname: &str, timestamp: DateTime<Utc>, entry: CPUMetricEntry) {
    sqlx::query!(
        "insert into metric_cpu (hostname, timestamp, cpu, \"user\", nice, system, idle, iowait, irq, softirq, guest, steal, guest_nice) values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) returning cpu",
        hostname.to_string(), timestamp,
        entry.cpu as i32, entry.user as i32, entry.nice as i32, entry.system as i32, entry.idle as i32,
        entry.iowait as i32, entry.irq as i32, entry.softirq as i32, entry.guest as i32, entry.steal as i32,
        entry.guest_nice as i32
    ).fetch_one(&mut database).await;
}