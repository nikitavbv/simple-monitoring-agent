use std::env;

const DEFAULT_REPORT_INTERVAL: u16 = 60; // every minute

pub fn get_metric_report_interval() -> u16 {
    env::var("REPORT_INTERVAL").ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(DEFAULT_REPORT_INTERVAL)
}