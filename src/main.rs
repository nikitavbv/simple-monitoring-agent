#![feature(try_trait)]
#![feature(associated_type_bounds)]

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

const METRICS_CLEANUP_INTERVAL: i64 = 100; // once in 100 collection iterations

#[tokio::main]
async fn main() {
    env::set_var("RUST_LOG", "agent=debug");
    env_logger::init();

    let mut database = connect().await
        .expect("failed to connect to database");

    let hostname = get_hostname();

    let mut cpu_collector = CpuMetricCollector::new();
    let fs_collector = FilesystemMetricCollector {};
    let io_collector = IOMetricCollector {};
    let la_collector = LoadAverageMetricCollector {};
    let memory_collector = MemoryMetricCollector {};
    let network_collector = NetworkMetricCollector {};
    let nginx_collector = NginxMetricCollector {};
    let postgres_collector = PostgresMetricCollector {};
    let docker_collector = DockerMetricCollector {};

    let collectors: Vec<Box<dyn MetricCollector> = vec![
        Box::new(cpu_collector), Box::new(fs_collector), Box::new(io_collector), Box::new(la_collector),
        Box::new(memory_collector), Box::new(network_collector), Box::new(nginx_collector),
        Box::new(postgres_collector), Box::new(docker_collector)
    ];

    let mut previous_io_stat = io_collector.collect(&mut database).await;
    let mut previous_network_stat = network_collector.collect(&mut database).await;
    let mut previous_docker_stat = docker_collector.collect(&mut database).await;
    let mut previous_nginx_stat = nginx_collector.collect(&mut database).await;
    let mut previous_postgres_stat = postgres_collector.collect(&mut database).await;

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

        if let Err(err) = cpu_collector.collect(&database, &hostname).await {
            warn!("failed to collect cpu metric: {}", err);
        }

        match la_collector.collect(&database).await {
            Ok(v) => if let Err(err) = la_collector.save(&v, &v, &database, &hostname).await {
                warn!("failed to record load average metric: {}", err);
            },
            Err(err) => warn!("failed to collect load average metric: {}", err)
        };

        match memory_collector.collect(&database).await {
            Ok(v) => if let Err(err) = memory_collector.save(&v, &v, &database, &hostname).await {
                warn!("failed to record memory metric: {}", err)
            },
            Err(err) => warn!("failed to collect memory metric: {}", err)
        };

        match io_collector.collect(&database).await {
            Ok(v) => {
                if previous_io_stat.is_ok() {
                    if let Err(err) = io_collector.save(&previous_io_stat.unwrap(), &v, &database, &hostname).await {
                        warn!("failed to save io metric: {}", err);
                    }
                }
                previous_io_stat = Ok(v);
            },
            Err(err) => warn!("failed to get io stats: {}", err)
        };

        match fs_collector.collect(&database).await {
            Ok(v) => if let Err(err ) = fs_collector.save(&v, &v, &database, &hostname).await {
                warn!("failed to save filesystem metric: {}", err);
            },
            Err(err) => warn!("failed to record filesystem usage metric: {}", err)
        };

        match network_collector.collect(&database).await {
            Ok(v) => {
                if previous_network_stat.is_ok() {
                    if let Err(err) = network_collector.save(&previous_network_stat.unwrap(), &v, &database, &hostname).await {
                        warn!("failed to save network metric: {}", err);
                    }
                }
                previous_network_stat = Ok(v);
            },
            Err(err) => {
                warn!("failed to get network stats: {}", err);
            }
        };

        match docker_collector.collect(&mut database).await {
            Ok(v) => {
                if previous_docker_stat.is_ok() {
                    if let Err(err) = docker_collector.save(&previous_docker_stat.unwrap(), &v, &database, &hostname).await {
                        warn!("failed to save docker metric: {}", err);
                    }
                }
                previous_docker_stat = Ok(v);
            },
            Err(err) => warn!("failed to get docker stats: {}", err)
        };

        match nginx_collector.collect(&database).await {
            Ok(v) => {
                if previous_nginx_stat.is_ok() {
                    if let Err(err) = nginx_collector.save(&previous_nginx_stat.unwrap(), &v, &database, &hostname).await {
                        warn!("failed to record nginx metric: {}", err);
                    }
                }
                previous_nginx_stat = Ok(v);
            },
            Err(err) => warn!("failed to get nginx stats: {}", err)
        }

        match postgres_collector.collect(&database).await {
            Ok(v) => {
                if previous_postgres_stat.is_ok() {
                    if let Err(err) = postgres_collector.save(&previous_postgres_stat.unwrap(), &v, &database, &hostname).await {
                        warn!("failed to record postgres metric: {}", err);
                    }
                }
                previous_postgres_stat = Ok(v);
            },
            Err(err) => warn!("failed to get postgres stats: {}", err)
        }

        if iter_count % METRICS_CLEANUP_INTERVAL == 0 {
            // time to clean up
            try_join_all(collectors.map(|collector| collector.cleanup(&database))).await;
        }
    }
}

async fn check_if_database_connection_is_live(mut database: &Database) -> bool {
    sqlx::query!("SELECT 'DBD::Pg ping test' as ping_response").fetch_one(&mut database).await.is_ok()
}
