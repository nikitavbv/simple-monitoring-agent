use async_std::prelude::*;
use async_std::fs::read_to_string;

struct CPUMetric  {
    cpu: u16,
    user: u64,
    nice: u64,
    system: u64,
    idle: u64,
    iowait: u64,
    irq: u64,
    softirq: u64,
    guest: u64,
    steal: u64,
    guest_nice: u64,
}

pub async fn monitor_cpu_usage() -> Result<(), String> {
    let data = read_to_string("/proc/stat").await.map_err(|_| "failed to read /proc/stat")?;

    data.lines()
        .filter(|a| *a != "")
        .filter(|line| line.split_whitespace().nth(0).unwrap().starts_with("cpu"))
        .for_each(|line| println!("{}", line));

    Ok(())
}