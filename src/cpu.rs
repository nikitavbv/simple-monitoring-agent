use async_std::fs::read_to_string;
use custom_error::custom_error;
use std::str::SplitWhitespace;
use std::option::NoneError;
use std::num::ParseIntError;

#[derive(Debug)]
pub struct CPUMetric  {
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

custom_error!{pub CPUMetricError
    FailedToRead{source: std::io::Error} = "failed to read metric",
    FailedToParse = "failed to parse metric"
}

impl From<std::option::NoneError> for CPUMetricError {
    fn from(err: NoneError) -> Self {
        println!("{}", "none");
        CPUMetricError::FailedToParse
    }
}

impl From<std::num::ParseIntError> for CPUMetricError {
    fn from(err: ParseIntError) -> Self {
        println!("{}", err);
        CPUMetricError::FailedToParse
    }
}

pub async fn monitor_cpu_usage() -> Result<Vec<Result<CPUMetric, CPUMetricError>>, CPUMetricError> {
    Ok(read_to_string("/proc/stat").await?.lines()
        .map(|line| line.split_whitespace())
        .filter(is_cpu_line)
        .map(|mut spl: SplitWhitespace| Ok(CPUMetric {
            cpu: spl.next()?[3..].parse()?,
            user: spl.next()?.parse()?,
            nice: spl.next()?.parse()?,
            system: spl.next()?.parse()?,
            idle: spl.next()?.parse()?,
            iowait: spl.next()?.parse()?,
            irq: spl.next()?.parse()?,
            softirq: spl.next()?.parse()?,
            guest: spl.next()?.parse()?,
            steal: spl.next()?.parse()?,
            guest_nice: spl.next()?.parse()?
        }))
        .collect())
}

fn is_cpu_line(spl: &SplitWhitespace) -> bool {
    let mut spl_clone = spl.clone();
    let first_word: &str = spl_clone.nth(0).expect("expected spl to contain at least one word");
    first_word.starts_with("cpu") && first_word.len() > 3 && spl_clone.count() == 10
}