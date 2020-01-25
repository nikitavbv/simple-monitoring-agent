#![feature(try_trait)]

extern crate custom_error;

mod config;
mod cpu;
mod database;
mod fs;
mod hostname;
mod io;
mod load_avg;
mod memory;

use std::time::Duration;
use std::env;

use async_std::task;
use log::{info, warn};

use crate::cpu::{monitor_cpu_usage, cpu_metric_from_stats, save_cpu_metric};
use crate::database::{connect, Database};
use crate::config::get_metric_report_interval;
use crate::hostname::get_hostname;
use crate::load_avg::{monitor_load_average, save_load_average_metric};
use crate::memory::{monitor_memory, save_memory_metric};
use crate::io::{monitor_io, io_metric_from_stats, save_io_metric};
use crate::fs::{monitor_filesystem_usage, save_filesystem_usage_metric};

#[async_std::main]
async fn main() {
    env::set_var("RUST_LOG", "agent=debug");
    env_logger::init();

    let mut database = connect().await
        .expect("failed to connect to database");

    let hostname = get_hostname();

    let mut previous_cpu_stat = monitor_cpu_usage().await;
    let mut previous_io_stat = monitor_io().await;

    info!("ready");

    loop {
        task::sleep(Duration::from_secs(get_metric_report_interval() as u64)).await;

        if !check_if_database_connection_is_live(&database).await {
            warn!("database connection is not live, reconnecting...");

            database = connect().await.expect("failed to connect to database");
            if !check_if_database_connection_is_live(&database).await {
                warn!("database connection is not live after reconnect. Exiting... Hopefully we will be restarted.");
                return;
            }
        }

        match monitor_cpu_usage().await {
            Ok(v) => {
                if previous_cpu_stat.is_ok() {
                    let metric = cpu_metric_from_stats(previous_cpu_stat.unwrap(), v.clone());
                    save_cpu_metric(database.clone(), hostname.clone(), metric).await;
                }
                previous_cpu_stat = Ok(v);
            },
            Err(err) => warn!("failed to get cpu stats: {}", err)
        };

        match monitor_load_average().await {
            Ok(v) => save_load_average_metric(&database, hostname.clone(), v).await,
            Err(err) => warn!("failed to record load average metric: {}", err)
        };

        match monitor_memory().await {
            Ok(v) => save_memory_metric(&database, &hostname, &v).await,
            Err(err) => warn!("failed to record memory metric: {}", err)
        };

        match monitor_io().await {
            Ok(v) => {
                if previous_io_stat.is_ok() {
                    let metric = io_metric_from_stats(previous_io_stat.unwrap(), v.clone());
                    save_io_metric(&database, &hostname, &metric).await;
                }
                previous_io_stat = Ok(v);
            },
            Err(err) => warn!("failed to get io stats: {}", err)
        }

        match monitor_filesystem_usage().await {
            Ok(v) => save_filesystem_usage_metric(&database, &hostname, &v).await,
            Err(err) => warn!("failed to record filesystem usage metric: {}", err)
        }
    }
}

async fn check_if_database_connection_is_live(mut database: &Database) -> bool {
    sqlx::query!("SELECT 'DBD::Pg ping test' as ping_response").fetch_one(&mut database).await.is_ok()
}
