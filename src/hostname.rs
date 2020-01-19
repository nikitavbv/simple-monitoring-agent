use std::env;
use std::fs::read_to_string;
use std::io::Result;

pub fn get_hostname() -> String {
    env::var("HOST").ok().unwrap_or_else(|| read_hostname().expect("failed to get hostname"))
}

fn read_hostname() -> Result<String> {
    read_to_string("/proc/sys/kernel/hostname")
}