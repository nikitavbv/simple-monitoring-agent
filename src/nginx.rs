use std::str::{SplitWhitespace, FromStr};
use std::option::NoneError;
use std::num::ParseIntError;
use std::env;

use chrono::{Utc, DateTime};
use hyper::{Client, Uri};
use custom_error::custom_error;
use futures::TryStreamExt;
use log::warn;

use crate::database::Database;

#[derive(Debug, Clone)]
pub struct NginxStat {
    timestamp: DateTime<Utc>,
    handled_requests: u64
}

#[derive(Debug, Clone)]
pub struct NginxMetric {
    timestamp: DateTime<Utc>,
    handled_requests: u32
}

custom_error!{pub NginxMetricError
    NotConfigured = "nginx status endpoint is not set",
    RequestFailed{source: reqwest::Error} = "failed to request nginx status: {source}",
    FailedToParse = "failed to parse metric"
}

impl From<std::option::NoneError> for NginxMetricError {
    fn from(err: NoneError) -> Self {
        warn!("err: {:?}", err);
        NginxMetricError::FailedToParse
    }
}

impl From<std::num::ParseIntError> for NginxMetricError {
    fn from(err: ParseIntError) -> Self {
        warn!("err: {:?}", err);
        NginxMetricError::FailedToParse
    }
}

impl From<http::uri::InvalidUri> for NginxMetricError {
    fn from(err: http::uri::InvalidUri) -> Self {
        NginxMetricError::FailedToParse
    }
}

fn get_nginx_status_endpoint_url() -> Option<String> {
    env::var("NGINX_STATUS_ENDPOINT").ok()
}

pub async fn monitor_nginx() -> Result<NginxStat, NginxMetricError> {
    let url = match get_nginx_status_endpoint_url() {
        Some(v) => v,
        None => return Err(NginxMetricError::NotConfigured)
    };

    let timestamp = Utc::now();

    let res = reqwest::get(&url).await?.text().await?;
    let mut stat = res.lines().skip(2).next()?.split_whitespace();

    Ok(NginxStat {
        timestamp,
        handled_requests: stat.nth(2)?.parse()?
    })
}

pub fn nginx_metric_from_stats(first: &NginxStat, second: &NginxStat) -> NginxMetric {
    let time_diff = ((second.timestamp - first.timestamp).num_milliseconds() / (1000 * 60)) as u64; // minutes

    NginxMetric {
        timestamp: second.timestamp,
        handled_requests: ((second.handled_requests - first.handled_requests) / time_diff) as u32
    }
}

pub async fn save_nginx_metric(mut database: &Database, hostname: &str, metric: &NginxMetric) {
    sqlx::query!(
        "insert into metric_nginx (hostname, timestamp, handled_requests) values ($1, $2, $3) returning hostname",
        hostname.to_string(), metric.timestamp, metric.handled_requests as i32
    ).fetch_one(&mut database).await;
}