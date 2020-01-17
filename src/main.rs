mod cpu;

use async_std::prelude::*;

use crate::cpu::monitor_cpu_usage;

#[async_std::main]
async fn main() -> Result<(), String> {
    println!("Hello, world!");

    monitor_cpu_usage().await?;

    Ok(())
}
