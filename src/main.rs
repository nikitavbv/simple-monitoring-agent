#![feature(try_trait)]

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

use crate::cpu::InstantCPUMetric;
use crate::database::{connect, Database};
use crate::config::get_metric_report_interval;
use crate::hostname::get_hostname;
use crate::load_avg::{cleanup_load_average_metric, LoadAverageMetric};
use crate::memory::{cleanup_memory_metric, MemoryMetric};
use crate::io::InstantIOMetric;
use crate::fs::FilesystemUsageMetric;
use crate::network::{cleanup_network_metric, InstantNetworkMetric};
use crate::docker::metric::{docker_metric_from_stats, InstantDockerContainerMetric, DockerContainerMetric};
use crate::nginx::{cleanup_nginx_metric, NginxInstantMetric};
use crate::postgres::{cleanup_postgres_metric, InstantPostgresMetric};
use crate::types::Metric;

const METRICS_CLEANUP_INTERVAL: i64 = 100; // once in 100 collection iterations

#[tokio::main]
async fn main() {
    env::set_var("RUST_LOG", "agent=debug");
    env_logger::init();

    let mut database = connect().await
        .expect("failed to connect to database");

    let hostname = get_hostname();

    let mut previous_cpu_stat = InstantCPUMetric::collect(&mut database).await;
    let mut previous_io_stat = InstantIOMetric::collect(&mut database).await;
    let mut previous_network_stat = InstantNetworkMetric::collect(&mut database).await;
    let mut previous_docker_stat = InstantDockerContainerMetric::collect(&mut database).await;
    let mut previous_nginx_stat = NginxInstantMetric::collect(&mut database).await;
    let mut previous_postgres_stat = InstantPostgresMetric::collect(&mut database).await;

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

        match InstantCPUMetric::collect(&database).await {
            Ok(v) => {
                if previous_cpu_stat.is_ok() {
                    if let Err(err) = v.save(&database, &previous_cpu_stat.unwrap(), &hostname).await {
                        warn!("failed to save cpu metric: {}", err);
                    }
                }
                previous_cpu_stat = Ok(v);
            },
            Err(err) => warn!("failed to get cpu stats: {}", err)
        };

        match LoadAverageMetric::collect(&database).await {
            Ok(v) => if let Err(err) = v.save(&database, &v, &hostname).await {
                warn!("failed to record load average metric: {}", err);
            },
            Err(err) => warn!("failed to collect load average metric: {}", err)
        };

        match MemoryMetric::collect(&database).await {
            Ok(v) => if let Err(err) = v.save(&database, &v, &hostname).await {
                warn!("failed to record memory metric: {}", err)
            },
            Err(err) => warn!("failed to collect memory metric: {}", err)
        };

        match InstantIOMetric::collect(&database).await {
            Ok(v) => {
                if previous_io_stat.is_ok() {
                    if let Err(err) = v.save(&database, &previous_io_stat.unwrap(), &hostname).await {
                        warn!("failed to save io metric: {}", err);
                    }
                }
                previous_io_stat = Ok(v);
            },
            Err(err) => warn!("failed to get io stats: {}", err)
        };

        match FilesystemUsageMetric::collect(&database).await {
            Ok(v) => if let Err(err) = v.save(&database, &v, &hostname).await {
                warn!("failed to save filesystem metric: {}", err);
            },
            Err(err) => warn!("failed to record filesystem usage metric: {}", err)
        };

        match InstantNetworkMetric::collect(&database).await {
            Ok(v) => {
                if previous_network_stat.is_ok() {
                    if let Err(err) = v.save(&database, &previous_network_stat.unwrap(), &hostname).await {
                        warn!("failed to save network metric: {}", err);
                    }
                }
                previous_network_stat = Ok(v);
            },
            Err(err) => {
                warn!("failed to get network stats: {}", err);
            }
        };

        match InstantDockerContainerMetric::collect(&mut database).await {
            Ok(v) => {
                if previous_docker_stat.is_ok() {
                    if let Err(err) = v.save(&database, &previous_docker_stat.unwrap(), &hostname).await {
                        warn!("failed to save docker metric: {}", err);
                    }
                }
                previous_docker_stat = Ok(v);
            },
            Err(err) => warn!("failed to get docker stats: {}", err)
        };

        match NginxInstantMetric::collect(&database).await {
            Ok(v) => {
                if previous_nginx_stat.is_ok() {
                    if let Err(err) = v.save(&database, &previous_nginx_stat.unwrap(), &hostname).await {
                        warn!("failed to record nginx metric: {}", err);
                    }
                }
                previous_nginx_stat = Ok(v);
            },
            Err(err) => warn!("failed to get nginx stats: {}", err)
        }

        match InstantPostgresMetric::collect(&database).await {
            Ok(v) => {
                if previous_postgres_stat.is_ok() {
                    if let Err(err) = v.save(&database, &previous_postgres_stat.unwrap(), &hostname).await {
                        warn!("failed to record postgres metric: {}", err);
                    }
                }
                previous_postgres_stat = Ok(v);
            },
            Err(err) => warn!("failed to get postgres stats: {}", err)
        }

        if iter_count % METRICS_CLEANUP_INTERVAL == 0 {
            // time to clean up
            if let Err(err) = InstantDockerContainerMetric::cleanup(&database).await {
                warn!("docker metric cleanup failed: {}", err);
            }

            if let Err(err) = InstantCPUMetric::cleanup(&database).await {
                warn!("cpu metric cleanup failed: {}", err);
            }

            if let Err(err) = FilesystemUsageMetric::cleanup(&database).await {
                warn!("fs metric cleanup failed: {}", err);
            }

            if let Err(err) = InstantIOMetric::cleanup(&database).await {
                warn!("io metric cleanup failed: {}", err);
            }

            if let Err(err) = cleanup_load_average_metric(&database).await {
                warn!("load average metric cleanup failed: {}", err);
            }

            if let Err(err) = cleanup_memory_metric(&database).await {
                warn!("memory metric cleanup failed: {}", err);
            }

            if let Err(err) = cleanup_network_metric(&database).await {
                warn!("database metric cleanup failed: {}", err);
            }

            if let Err(err) = cleanup_nginx_metric(&database).await {
                warn!("nginx metric cleanup failed: {}", err);
            }

            if let Err(err) = cleanup_postgres_metric(&database).await {
                warn!("postgres metric cleanup failed: {}", err);
            }
        }
    }
}

async fn check_if_database_connection_is_live(mut database: &Database) -> bool {
    sqlx::query!("SELECT 'DBD::Pg ping test' as ping_response").fetch_one(&mut database).await.is_ok()
}
