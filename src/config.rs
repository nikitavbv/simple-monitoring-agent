use std::env;
use chrono::Duration;

const DEFAULT_REPORT_INTERVAL: u16 = 60; // every minute

pub fn get_metric_report_interval() -> u16 {
    env::var("REPORT_INTERVAL").ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(DEFAULT_REPORT_INTERVAL)
}

pub fn get_max_metrics_age() -> Duration {
    env::var("MAX_METRICS_AGE").ok()
        .and_then(|v| v.parse::<i64>().ok())
        .map(|v| Duration::hours(v))
        .unwrap_or(Duration::weeks(2))
}