#![feature(try_trait)]

extern crate custom_error;

mod config;
mod cpu;
mod database;
mod hostname;
mod load_avg;

use std::time::Duration;
use std::env;

use async_std::task;
use log::{info, warn};

use crate::cpu::{monitor_cpu_usage, cpu_metric_from_stats, save_cpu_metric, get_cpu_latest_insert};
use crate::database::connect;
use crate::config::get_metric_report_interval;
use crate::hostname::get_hostname;
use crate::load_avg::{monitor_load_average, save_load_average_metric};

#[async_std::main]
async fn main() {
    env::set_var("RUST_LOG", "agent=debug");
    env_logger::init();

    let mut database = connect().await
        .expect("failed to connect to database");

    let hostname = get_hostname();

    let mut previous_cpu_stat = monitor_cpu_usage().await;

    info!("ready");

    loop {
        info!("sleeping for {} seconds until next iteration", get_metric_report_interval());
        task::sleep(Duration::from_secs(get_metric_report_interval() as u64)).await;

        match monitor_cpu_usage().await {
            Ok(v) => {
                if previous_cpu_stat.is_ok() {
                    let metric = cpu_metric_from_stats(previous_cpu_stat.unwrap(), v.clone());
                    save_cpu_metric(database.clone(), hostname.clone(), metric).await;
                    info!("cpu metric record is saved");
                    info!("cpu metric latest insert: {:?}", get_cpu_latest_insert(&mut database).await);
                }
                previous_cpu_stat = Ok(v);
            },
            Err(err) => warn!("failed to get cpu stats: {}", err)
        };

        match monitor_load_average().await {
            Ok(v) => save_load_average_metric(&database, hostname.clone(), v).await,
            Err(err) => warn!("failed to record load average metric: {}", err)
        };
    }
}
