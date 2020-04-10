use std::option::NoneError;
use std::num::ParseIntError;
use std::env;

use chrono::{Utc, DateTime};
use custom_error::custom_error;
use async_trait::async_trait;

use crate::database::Database;
use crate::config::get_max_metrics_age;
use crate::types::{Metric, MetricCollectionError, MetricSaveError, MetricCleanupError, MetricCollector, MetricCollectorError};
use sqlx::{PgConnection, Pool};

#[derive(Debug, Clone)]
pub struct NginxInstantMetric {
    timestamp: DateTime<Utc>,
    handled_requests: u64
}

#[derive(Debug, Clone)]
pub struct NginxMetric {
    timestamp: DateTime<Utc>,
    handled_requests: u32
}

pub struct NginxMetricCollector {
    previous: Option<NginxInstantMetric>
}

impl NginxMetricCollector {

    pub fn new() -> Self {
        NginxMetricCollector {
            previous: None
        }
    }

    async fn collect_metric(&self, mut database: &Database) -> Result<Box<NginxInstantMetric>, MetricCollectionError> {
        let url = match get_nginx_status_endpoint_url() {
            Some(v) => v,
            None => return Err(MetricCollectionError::NotConfigured { description: "nginx not configured".to_string() })
        };

        let timestamp = Utc::now();

        let res = reqwest::get(&url).await?.text().await?;
        let mut stat = res.lines().skip(2).next()?.split_whitespace();

        Ok(Box::new(NginxInstantMetric {
            timestamp,
            handled_requests: stat.nth(2)?.parse()?
        }))
    }

    async fn save(&self, previous: &NginxInstantMetric, metric: &NginxInstantMetric, mut database: &Database, hostname: &str) -> Result<(), MetricSaveError> {
        let metric = nginx_metric_from_stats(&previous, &metric);
        sqlx::query!(
            "insert into metric_nginx (hostname, timestamp, handled_requests) values ($1, $2, $3) returning hostname",
            hostname.to_string(), metric.timestamp, metric.handled_requests as i32
        ).fetch_one(&mut database).await?;

        Ok(())
    }
}

#[async_trait]
impl MetricCollector for NginxMetricCollector {

    async fn collect(&mut self, mut database: &Database, hostname: &str) -> Result<(), MetricCollectorError> {
        let metric = self.collect_metric(database).await?;
        if let Some(prev) = &self.previous {
            self.save(prev, &metric, database, hostname).await?;
        }
        self.previous = Some(*metric);
        Ok(())
    }

    async fn cleanup(&self, mut database: &Database) -> Result<(), MetricCleanupError> {
        let min_timestamp = Utc::now() - get_max_metrics_age();

        sqlx::query!("delete from metric_nginx where timestamp < $1 returning 1 as result", min_timestamp)
            .fetch_one(&mut database).await?;

        Ok(())
    }
}

custom_error!{pub NginxMetricError
    NotConfigured = "nginx status endpoint is not set",
    RequestFailed{source: reqwest::Error} = "failed to request nginx status: {source}",
    FailedToParse{description: String} = "failed to parse metric",
    DatabaseQueryFailed{source: sqlx::error::Error} = "database query failed"
}

impl From<std::option::NoneError> for NginxMetricError {
    fn from(_: NoneError) -> Self {
        NginxMetricError::FailedToParse {
            description: "NoneError".to_string()
        }
    }
}

impl From<std::num::ParseIntError> for NginxMetricError {
    fn from(err: ParseIntError) -> Self {
        NginxMetricError::FailedToParse {
            description: err.to_string()
        }
    }
}

impl From<http::uri::InvalidUri> for NginxMetricError {
    fn from(err: http::uri::InvalidUri) -> Self {
        NginxMetricError::FailedToParse {
            description: err.to_string()
        }
    }
}

fn get_nginx_status_endpoint_url() -> Option<String> {
    env::var("NGINX_STATUS_ENDPOINT").ok()
}

fn nginx_metric_from_stats(first: &NginxInstantMetric, second: &NginxInstantMetric) -> NginxMetric {
    let time_diff = ((second.timestamp - first.timestamp).num_milliseconds() / (1000 * 60)) as u64; // minutes

    NginxMetric {
        timestamp: second.timestamp,
        handled_requests: ((second.handled_requests - first.handled_requests) / time_diff) as u32
    }
}
