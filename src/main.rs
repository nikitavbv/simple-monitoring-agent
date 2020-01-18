#![feature(try_trait)]

extern crate custom_error;

mod cpu;

use crate::cpu::monitor_cpu_usage;

#[async_std::main]
async fn main() {
    println!("Hello, world!");

    let cpu = monitor_cpu_usage().await.unwrap();
    cpu.into_iter().for_each(|item| println!("{:?}", item.unwrap()));
}
