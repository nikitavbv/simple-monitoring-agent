#![feature(try_trait)]
#![feature(associated_type_bounds)]
#![feature(async_closure)]

extern crate custom_error;

mod config;
mod cpu;
mod database;
mod docker;
mod fs;
mod hostname;
mod io;
mod load_avg;
mod memory;
mod network;
mod nginx;
mod postgres;
mod types;

use std::time::Duration;
use std::env;

use async_std::task;
use log::{info, warn};
use futures::future::{try_join_all, try_join};

use crate::cpu::CpuMetricCollector;
use crate::database::{connect, Database};
use crate::config::get_metric_report_interval;
use crate::hostname::get_hostname;
use crate::load_avg::LoadAverageMetricCollector;
use crate::memory::MemoryMetricCollector;
use crate::io::IOMetricCollector;
use crate::fs::FilesystemMetricCollector;
use crate::network::NetworkMetricCollector;
use crate::docker::metric::DockerMetricCollector;
use crate::nginx::NginxMetricCollector;
use crate::postgres::PostgresMetricCollector;
use crate::types::{Metric, MetricCollector};
use futures::FutureExt;

const METRICS_CLEANUP_INTERVAL: i64 = 100; // once in 100 collection iterations

#[tokio::main]
async fn main() {
    env::set_var("RUST_LOG", "agent=debug");
    env_logger::init();

    let mut database = connect().await
        .expect("failed to connect to database");

    let hostname = get_hostname();

    let mut cpu_collector = CpuMetricCollector::new();
    let mut fs_collector = FilesystemMetricCollector::new();
    let mut io_collector = IOMetricCollector::new();
    let mut la_collector = LoadAverageMetricCollector::new();
    let mut memory_collector = MemoryMetricCollector::new();
    let mut network_collector = NetworkMetricCollector::new();
    let mut nginx_collector = NginxMetricCollector::new();
    let mut postgres_collector = PostgresMetricCollector::new();
    let mut docker_collector = DockerMetricCollector::new();

    let mut collectors: Vec<Box<dyn MetricCollector>> = vec![
        Box::new(cpu_collector), Box::new(fs_collector), Box::new(io_collector), Box::new(la_collector),
        Box::new(memory_collector), Box::new(network_collector), Box::new(nginx_collector),
        Box::new(postgres_collector), Box::new(docker_collector)
    ];

    info!("ready");

    let mut iter_count: i64 = 0;

    loop {
        iter_count += 1;
        task::sleep(Duration::from_secs(get_metric_report_interval() as u64)).await;

        if !check_if_database_connection_is_live(&database).await {
            warn!("database connection is not live, reconnecting...");

            database = connect().await.expect("failed to connect to database");
            if !check_if_database_connection_is_live(&database).await {
                warn!("database connection is not live after reconnect. Exiting... Hopefully we will be restarted.");
                return;
            }
        }

        for collector in &mut collectors {
            if let Err(err) = collector.collect(&database, &hostname).await {
                warn!("failed to collect metric: {}", err);
            }
        }

        if iter_count % METRICS_CLEANUP_INTERVAL == 0 {
            // time to clean up
            try_join_all(collectors.iter().map(|collector| collector.cleanup(&database))).await;
        }
    }
}

async fn check_if_database_connection_is_live(mut database: &Database) -> bool {
    sqlx::query!("SELECT 'DBD::Pg ping test' as ping_response").fetch_one(&mut database).await.is_ok()
}
