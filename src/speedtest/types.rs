use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeInfo {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub node_id: String,
    #[serde(
        default,
        alias = "nodeIp",
        alias = "nodeip",
        alias = "ip",
        alias = "innerIp"
    )]
    pub node_ip: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub status: i32,
}

#[derive(Debug, Clone)]
pub struct TestResult {
    pub dl_avg: f64,
    pub dl_max: f64,
    pub ul_avg: f64,
    pub ul_max: f64,
    pub ping_idle: f64,
    pub jitter_idle: f64,
    pub ping_loaded: f64,
    pub jitter_loaded: f64,
    pub dl_bytes: u64,
    pub ul_bytes: u64,
    pub loaded_ping_samples: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestPriority {
    DownloadFirst,
    UploadFirst,
}

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub duration_sec: u64,
    pub concurrency: usize,
    pub smoothing_window_sec: f64,
    pub speed_refresh_ms: u64,
    pub ping_refresh_ms: u64,
    pub priority: TestPriority,
    pub allow_official_cheat_calc: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            duration_sec: 10,
            concurrency: 8,
            smoothing_window_sec: 2.5,
            speed_refresh_ms: 250,
            ping_refresh_ms: 500,
            priority: TestPriority::DownloadFirst,
            allow_official_cheat_calc: true,
        }
    }
}

pub enum ProgressEvent {
    Status(String),
    ServerSelected {
        base_url: String,
        province_label: String,
    },
    Info {
        user: String,
        ip: String,
        city: String,
        bw: String,
    },
    Nodes(Vec<NodeInfo>),
    DownloadProgress {
        ratio: f32,
        speed: f64,
    },
    UploadProgress {
        ratio: f32,
        speed: f64,
    },
    PingUpdate {
        ping: f64,
        jitter: f64,
    },
    NodeIpUpdate {
        node_id: String,
        node_ip: String,
    },
    Finished(TestResult),
}

pub struct ActiveTestHandle {
    pub stop: Arc<AtomicBool>,
}

impl ActiveTestHandle {
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
