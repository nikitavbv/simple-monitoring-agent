#![feature(try_trait)]

extern crate custom_error;

mod config;
mod cpu;
mod database;
mod hostname;

use std::time::Duration;

use async_std::task;

use crate::cpu::{monitor_cpu_usage, cpu_metric_from_stats, save_cpu_metric};
use crate::database::connect;
use crate::config::get_metric_report_interval;
use crate::hostname::get_hostname;

#[async_std::main]
async fn main() {
    let database = connect().await
        .expect("failed to connect to database");

    let hostname = get_hostname();

    let mut previous_cpu_stat = monitor_cpu_usage().await;

    println!("ready");

    sqlx::query!("select * from metric_cpu");

    loop {
        task::sleep(Duration::from_secs(get_metric_report_interval() as u64)).await;

        match monitor_cpu_usage().await {
            Ok(v) => {
                if previous_cpu_stat.is_ok() {
                    let metric = cpu_metric_from_stats(previous_cpu_stat.unwrap(), v.clone());
                    save_cpu_metric(database.clone(), hostname.clone(), metric).await;
                }
                previous_cpu_stat = Ok(v);
            },
            Err(err) => eprintln!("failed to get cpu stats: {}", err)
        };
    }
}
