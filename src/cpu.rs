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

fn monitor_cpu_usage() -> Vec<CPUMetric> {
    unimplemented!()
}