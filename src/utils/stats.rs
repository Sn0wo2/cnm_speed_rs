use std::collections::VecDeque;

pub struct RollingRateWindow {
    bytes: VecDeque<u64>,
    times: VecDeque<f64>,
    sum_bytes: u64,
    sum_time: f64,
    capacity: usize,
}

impl RollingRateWindow {
    pub fn new(capacity: usize) -> Self {
        Self {
            bytes: VecDeque::with_capacity(capacity),
            times: VecDeque::with_capacity(capacity),
            sum_bytes: 0,
            sum_time: 0.0,
            capacity,
        }
    }

    pub fn push(&mut self, bytes: u64, seconds: f64) {
        if self.bytes.len() == self.capacity {
            if let Some(old) = self.bytes.pop_front() {
                self.sum_bytes = self.sum_bytes.saturating_sub(old);
            }
            if let Some(old) = self.times.pop_front() {
                self.sum_time = (self.sum_time - old).max(0.0);
            }
        }

        self.bytes.push_back(bytes);
        self.times.push_back(seconds);
        self.sum_bytes = self.sum_bytes.saturating_add(bytes);
        self.sum_time += seconds;
    }

    pub fn bits_per_sec(&self) -> f64 {
        if self.sum_time <= 0.0 {
            return 0.0;
        }
        (self.sum_bytes as f64 * 8.0) / self.sum_time
    }
}

pub struct SampleStats {
    pub avg_bps: f64,
    pub max_bps: f64,
}

impl SampleStats {
    pub fn from_samples(
        samples: &[(f64, f64)],
        duration_sec: u64,
        smoothing_window_sec: f64,
    ) -> Self {
        let stable: Vec<f64> = samples
            .iter()
            .filter(|sample| sample.0 >= (duration_sec as f64 * 0.3).min(2.0))
            .map(|sample| sample.1)
            .collect();

        let stable = if stable.is_empty() {
            samples.iter().map(|sample| sample.1).collect()
        } else {
            stable
        };

        let mut sorted = stable.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let trim_ratio = if smoothing_window_sec >= 2.5 {
            0.1
        } else {
            0.05
        };
        let start = (sorted.len() as f64 * trim_ratio) as usize;
        let end = (sorted.len() as f64 * (1.0 - trim_ratio)) as usize;
        let trimmed = if end > start {
            &sorted[start..end.max(start + 1)]
        } else {
            &sorted
        };

        Self {
            avg_bps: trimmed.iter().sum::<f64>() / trimmed.len() as f64,
            max_bps: *stable
                .iter()
                .max_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap_or(&0.0),
        }
    }
}

pub struct DelayStats {
    pub avg_ms: f64,
    pub jitter_ms: f64,
}

impl DelayStats {
    pub fn from_values(values: &[f64]) -> Self {
        if values.is_empty() {
            return Self {
                avg_ms: 0.0,
                jitter_ms: 0.0,
            };
        }

        let avg = values.iter().sum::<f64>() / values.len() as f64;
        let jitter = if values.len() > 1 {
            values.iter().map(|v| (v - avg).abs()).sum::<f64>() / values.len() as f64
        } else {
            0.0
        };

        Self {
            avg_ms: avg,
            jitter_ms: jitter,
        }
    }
}
