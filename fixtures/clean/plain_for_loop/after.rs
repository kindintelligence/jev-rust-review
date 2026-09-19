pub struct Summary {
    pub count: usize,
    pub max: f64,
}

/// Counts valid readings and tracks the largest one.
pub fn summarize(readings: &[f64]) -> Summary {
    let mut count = 0;
    let mut max = f64::MIN;
    for r in readings {
        if r.is_nan() {
            continue;
        }
        count += 1;
        if *r > max {
            max = *r;
        }
    }
    Summary { count, max }
}
