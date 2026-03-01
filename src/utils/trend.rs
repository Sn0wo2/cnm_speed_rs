use std::collections::VecDeque;

#[derive(Clone, Copy)]
pub struct TrendRenderer {
    pub warmup_min_speed_mbps: f64,
}

impl Default for TrendRenderer {
    fn default() -> Self {
        Self {
            warmup_min_speed_mbps: 0.8,
        }
    }
}

impl TrendRenderer {
    pub fn should_start_capture(&self, ratio: f32, speed_mbps: f64) -> bool {
        ratio > 0.0 && speed_mbps >= self.warmup_min_speed_mbps
    }

    pub fn render_rtl_lines(
        &self,
        hist: &VecDeque<f64>,
        width: usize,
        ratio: f32,
        start_ratio: Option<f32>,
        rows: usize,
    ) -> Vec<String> {
        let rows = rows.clamp(1, 3);
        if width == 0 {
            return vec![String::new(); rows];
        }

        let start = start_ratio.unwrap_or(0.0).clamp(0.0, 0.9999);
        // Coverage is derived from elapsed ratio in the effective interval [start, 1].
        let effective_ratio = if ratio <= start {
            0.0
        } else {
            ((ratio - start) / (1.0 - start)).clamp(0.0, 1.0)
        };

        let covered = ((effective_ratio * width as f32).ceil() as usize).min(width);
        let empty_slots = width.saturating_sub(covered);

        if covered == 0 || hist.is_empty() {
            return vec!["░".repeat(width); rows];
        }

        let max_v = hist.iter().copied().fold(0.1, f64::max);
        let min_v = hist.iter().copied().fold(max_v, f64::min);
        let range = (max_v - min_v).max(0.1);
        let hist_len = hist.len();

        let mut norms = Vec::with_capacity(covered);
        for i in 0..covered {
            let hist_idx = (i * hist_len) / covered;
            let v = hist[hist_idx.min(hist_len.saturating_sub(1))];
            norms.push(((v - min_v) / range).powf(0.5).clamp(0.0, 1.0));
        }

        // Keep short-term movement but reduce single-column jitter.
        let mut smoothed = Vec::with_capacity(norms.len());
        let mut prev = norms[0];
        smoothed.push(prev);
        for &curr in norms.iter().skip(1) {
            let next = prev * 0.35 + curr * 0.65;
            smoothed.push(next);
            prev = next;
        }

        if rows == 1 {
            let chars = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇'];
            let mut line = String::with_capacity(width * 3);
            line.push_str(&"░".repeat(empty_slots));
            for norm in smoothed {
                let idx = (norm * 7.0).round() as usize;
                line.push(chars[idx.min(7)]);
            }
            return vec![line];
        }

        let mut out = vec![String::with_capacity(width * 3); rows];
        for line in &mut out {
            line.push_str(&"░".repeat(empty_slots));
        }

        let row_chars = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
        let total_levels = rows * 8;
        for norm in smoothed {
            let level = (norm * total_levels as f64).round() as usize;
            for (row_idx, line) in out.iter_mut().enumerate() {
                let row_base = (rows - row_idx - 1) * 8;
                let row_level = level.saturating_sub(row_base).min(8);
                line.push(row_chars[row_level]);
            }
        }

        out
    }
}
