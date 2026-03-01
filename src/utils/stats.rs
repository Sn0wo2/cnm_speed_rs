use std::collections::VecDeque;

#[derive(Clone, Copy)]
struct RateSample {
    bytes: u64,
    seconds: f64,
}

pub struct RollingRateWindow {
    samples: VecDeque<RateSample>,
    sum_bytes: u64,
    sum_time: f64,
    capacity: usize,
}

impl RollingRateWindow {
    pub fn new(capacity: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(capacity),
            sum_bytes: 0,
            sum_time: 0.0,
            capacity,
        }
    }

    pub fn push(&mut self, bytes: u64, seconds: f64) {
        if self.samples.len() == self.capacity {
            if let Some(old) = self.samples.pop_front() {
                self.sum_bytes = self.sum_bytes.saturating_sub(old.bytes);
                self.sum_time = (self.sum_time - old.seconds).max(0.0);
            }
        }

        self.samples.push_back(RateSample { bytes, seconds });
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
        _smoothing_window_sec: f64,
    ) -> Self {
        let start = std::time::Instant::now();
        if samples.is_empty() {
            return Self {
                avg_bps: 0.0,
                max_bps: 0.0,
            };
        }

        let start_threshold = (duration_sec as f64 * 0.3).min(2.0);

        let mut max_bps = 0.0f64;
        let mut stable_sum = 0.0f64;
        let mut stable_count = 0usize;

        for &(ts, bps) in samples {
            if bps > max_bps {
                max_bps = bps;
            }
            if ts >= start_threshold {
                stable_sum += bps;
                stable_count += 1;
            }
        }

        let avg_bps = if stable_count > 0 {
            stable_sum / stable_count as f64
        } else {
            samples.iter().map(|s| s.1).sum::<f64>() / samples.len() as f64
        };

        let res = Self { avg_bps, max_bps };
        tracing::debug!("Stats processed in {:?}", start.elapsed());
        res
    }
}

#[derive(Clone, Copy, Default)]
pub struct OnlineDelayStats {
    count: usize,
    mean_ms: f64,
    m2: f64,
}

impl OnlineDelayStats {
    pub fn push(&mut self, value_ms: f64) {
        self.count += 1;
        let delta = value_ms - self.mean_ms;
        self.mean_ms += delta / self.count as f64;
        let delta2 = value_ms - self.mean_ms;
        self.m2 += delta * delta2;
    }

    pub fn avg_ms(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.mean_ms
        }
    }

    pub fn jitter_ms(&self) -> f64 {
        if self.count <= 1 {
            0.0
        } else {
            (self.m2 / self.count as f64).sqrt()
        }
    }
}
