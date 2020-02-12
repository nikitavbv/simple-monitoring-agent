trait Metric {
    fn collect() -> dyn Metric;
}