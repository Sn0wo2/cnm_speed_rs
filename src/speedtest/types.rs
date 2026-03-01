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
#[allow(dead_code)]
pub struct TestResult {
    pub dl_avg: f64,
    pub dl_raw_avg: f64,
    pub dl_max: f64,
    pub ul_avg: f64,
    pub ul_raw_avg: f64,
    pub ul_max: f64,
    pub ping_idle: f64,
    pub jitter_idle: f64,
    pub ping_idle_total: usize,
    pub ping_idle_failed: usize,

    pub ping_dl: f64,
    pub jitter_dl: f64,
    pub ping_dl_total: usize,
    pub ping_dl_failed: usize,

    pub ping_ul: f64,
    pub jitter_ul: f64,
    pub ping_ul_total: usize,
    pub ping_ul_failed: usize,

    pub dl_bytes: u64,
    pub ul_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TestPriority {
    DownloadFirst,
    UploadFirst,
    DownloadOnly,
    UploadOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub duration_sec: u64,
    pub concurrency: usize,
    pub smoothing_window_sec: f64,
    pub speed_refresh_ms: u64,
    pub ping_refresh_ms: u64,
    pub priority: TestPriority,
    pub allow_official_cheat_calculation: bool,
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
            allow_official_cheat_calculation: false,
        }
    }
}

pub enum ProgressEvent {
    Status(String),
    ServerSelected {
        base_url: String,
        province_label: String,
    },
    UserInfo {
        user: String,
        ip: String,
        city: String,
        bw: String,
    },
    NodesUpdate(Vec<NodeInfo>),
    DownloadUpdate {
        ratio: f32,
        speed: f64,
        raw_speed: f64,
    },
    UploadUpdate {
        ratio: f32,
        speed: f64,
        raw_speed: f64,
    },
    LatencyUpdate {
        ping: f64,
        jitter: f64,
        failed_count: usize,
        total_count: usize,
    },
    NodeIpFound {
        node_id: String,
        node_ip: String,
    },
    TestAborted {
        reason: String,
    },
    TestFinished(TestResult),
}

pub struct ActiveTestHandle {
    pub stop: Arc<AtomicBool>,
}

impl ActiveTestHandle {
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
