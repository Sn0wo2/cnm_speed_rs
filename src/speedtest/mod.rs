use crate::source::cmcc_types::{ApiResponse, BeginTestData};
use crate::speedtest::api::{BeginTestRequest, DefaultNodeRequest, SpeedtestApi};
use crate::speedtest::types::{
    ActiveTestHandle, ProgressEvent, RuntimeConfig, TestPriority, TestResult,
};
use crate::utils::crypto::CMCCCrypto;
use crate::utils::stats::{DelayStats, RollingRateWindow, SampleStats};
use rand::RngExt;
use reqwest::Client;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{error, info};

mod api;
pub mod types;

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/133.0.0.0 Safari/537.36";

fn join_base(base_url: &str, path: &str) -> String {
    format!("{}{}", base_url.trim_end_matches('/'), path)
}

fn node_url(node_ip: &str, port: u16, path: &str) -> String {
    format!("http://{}:{}{}", node_ip, port, path)
}

#[derive(Clone)]
pub struct SpeedtestEndpoints {
    pub task_page_path: &'static str,
    pub get_user_ip_path: &'static str,
    pub query_region_dispatch_path: &'static str,
    pub get_ip_info_path: &'static str,
    pub select_node_by_city_path: &'static str,
    pub get_default_node_path: &'static str,
    pub begin_test_path: &'static str,
    pub node_ping_path: &'static str,
    pub node_download_path: &'static str,
    pub node_upload_path: &'static str,
}

const DL_MULTIPLIER: f64 = 1.14;
const UL_MULTIPLIER: f64 = 1.09;
const TEN_GIG_THRESHOLD_80: f64 = 8000.0;
const TEN_GIG_THRESHOLD_90: f64 = 9000.0;
const TEN_GIG_BOOST_MULTIPLIER: f64 = 1.125;
const TEN_GIG_BOOST_ADD: f64 = 1000.0;

#[derive(Clone, Copy)]
struct SpeedAdjuster {
    is_ten_gig: bool,
}

impl SpeedAdjuster {
    fn from_begin_data(data: &BeginTestData) -> Self {
        Self {
            is_ten_gig: parse_ten_gig_flag(&data.is_ten_thousand),
        }
    }

    fn adjust_download_mbps(&self, raw_mbps: f64) -> f64 {
        self.adjust_mbps(raw_mbps, DL_MULTIPLIER)
    }

    fn adjust_upload_mbps(&self, raw_mbps: f64) -> f64 {
        self.adjust_mbps(raw_mbps, UL_MULTIPLIER)
    }

    fn adjust_mbps(&self, raw_mbps: f64, non_ten_gig_multiplier: f64) -> f64 {
        if self.is_ten_gig {
            if raw_mbps < TEN_GIG_THRESHOLD_80 {
                raw_mbps * TEN_GIG_BOOST_MULTIPLIER
            } else if raw_mbps < TEN_GIG_THRESHOLD_90 {
                raw_mbps + TEN_GIG_BOOST_ADD
            } else {
                raw_mbps
            }
        } else {
            raw_mbps * non_ten_gig_multiplier
        }
    }
}

fn parse_ten_gig_flag(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Bool(v) => *v,
        serde_json::Value::Number(v) => v.as_i64() == Some(1),
        serde_json::Value::String(v) => {
            let s = v.trim();
            s == "1" || s.eq_ignore_ascii_case("true")
        }
        _ => false,
    }
}

pub struct SpeedTester {
    pub client: Client,
    pub crypto: CMCCCrypto,
    pub base_url: String,
    pub origin: String,
    pub referer: String,
    pub node_port: u16,
    pub endpoints: SpeedtestEndpoints,
}

impl SpeedTester {
    pub fn new(base_url: String, node_port: u16, endpoints: SpeedtestEndpoints) -> Self {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .pool_max_idle_per_host(0)
            .pool_idle_timeout(Duration::from_secs(1))
            .build()
            .unwrap_or_default();
        let origin = base_url.clone();
        let referer = join_base(&base_url, endpoints.task_page_path);
        Self {
            client,
            crypto: CMCCCrypto::new(),
            base_url,
            origin,
            referer,
            node_port,
            endpoints,
        }
    }

    pub fn build_headers(&self) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Accept", "application/json, text/plain, */*".parse().unwrap());
        headers.insert("Content-Type", "application/json;charset=UTF-8".parse().unwrap());
        headers.insert("Origin", self.origin.parse().unwrap());
        headers.insert("Referer", self.referer.parse().unwrap());
        headers
    }

    async fn fetch_user_ip(&self) -> String {
        info!("Fetching user IP...");
        let url = join_base(&self.base_url, self.endpoints.get_user_ip_path);
        match self.client.get(&url)
            .headers(self.build_headers())
            .send()
            .await
        {
            Ok(resp) => {
                if let Ok(json) = resp.json::<ApiResponse>().await {
                    let ip = json.data.as_str().unwrap_or("").to_string();
                    info!("Got User IP: {}", ip);
                    return ip;
                }
            }
            Err(e) => error!("getUserIp failed: {}", e),
        }
        String::new()
    }

    pub fn parse_begin_test(&self, data: serde_json::Value) -> Option<BeginTestData> {
        if data.is_string() {
            let s = data.as_str().unwrap();
            if s == "{}" || s.is_empty() {
                return None;
            }
            if let Ok(nested) = serde_json::from_str::<BeginTestData>(s) {
                return Some(nested);
            }
        }
        serde_json::from_value::<BeginTestData>(data).ok()
    }

    pub fn spawn_test(
        self: &Arc<Self>,
        cfg: RuntimeConfig,
        node_id_override: Option<String>,
        tx: mpsc::Sender<ProgressEvent>,
        prefetched_ip: Option<String>,
    ) -> ActiveTestHandle {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = Arc::clone(&stop);
        let tester = Arc::clone(self);
        tokio::spawn(async move {
            tester.run_test(cfg, node_id_override, tx, stop_clone, prefetched_ip).await;
        });
        ActiveTestHandle { stop }
    }

    pub async fn run_test(
        &self,
        cfg: RuntimeConfig,
        node_id_override: Option<String>,
        tx: mpsc::Sender<ProgressEvent>,
        stop: Arc<AtomicBool>,
        prefetched_ip: Option<String>,
    ) {
        let is_init = cfg.duration_sec == 0;
        info!(
            "Speed test starting (init={}). Duration: {}s",
            is_init, cfg.duration_sec
        );

        let _ = tx.send(ProgressEvent::Status("Connecting...".into())).await;

        let user_ip = self.resolve_user_ip(prefetched_ip).await;

        if stop.load(Ordering::Relaxed) {
            return;
        }

        let (province, city, isp) = self.resolve_region(&user_ip).await;

        if stop.load(Ordering::Relaxed) {
            return;
        }

        let api = SpeedtestApi::new(self);
        let context = self.load_runtime_context(&api, &province, &city, &isp, &user_ip, &tx).await;

        if stop.load(Ordering::Relaxed) {
            return;
        }

        let default_node = self.resolve_default_node(&api, &context, &tx).await;
        if let Some(ref node) = default_node {
            if !node.node_ip.is_empty() {
                let _ = tx.send(ProgressEvent::NodeIpFound {
                    node_id: node.node_id.clone(),
                    node_ip: node.node_ip.clone(),
                }).await;
            }
        }

        if context.nodes.is_empty() && default_node.is_none() {
            let _ = tx.send(ProgressEvent::Status("Server Error".into())).await;
            return;
        }

        let selected = if let Some(ref n) = default_node {
            n.clone()
        } else {
            context.nodes[0].clone()
        };
        let mut node_ip = selected.node_ip.clone();

        if is_init {
            self.prefetch_partial_node_ips(&api, &context, &tx, 3).await;
        }

        if node_ip.is_empty() {
            if let Some(prefetched_ip) =
                self.prefetch_node_ip_for(&api, &context, selected.node_id.as_str()).await
            {
                node_ip = prefetched_ip.clone();
                let _ = tx.send(ProgressEvent::NodeIpFound {
                    node_id: selected.node_id.clone(),
                    node_ip: prefetched_ip,
                }).await;
            }
        }

        let ping_count = if is_init { 2 } else { 5 };
        let (p_idle, j_idle) = self.measure_ping(&node_ip, self.node_port, ping_count).await;
        let _ = tx.send(ProgressEvent::LatencyUpdate {
            ping: p_idle,
            jitter: j_idle,
        }).await;

        if is_init {
            let _ = tx.send(ProgressEvent::Status("Ready".into())).await;
            return;
        }

        if stop.load(Ordering::Relaxed) {
            return;
        }

        let _ = tx.send(ProgressEvent::Status("Testing...".into())).await;
        let down_task = self.begin_download_task(&api, &context, node_id_override.as_deref(), &tx).await;
        let d_task_id = down_task.0;
        let d_node_ip = down_task.1;
        let speed_adjuster = down_task.2;

        if d_task_id.is_empty() {
            let _ = tx.send(ProgressEvent::Status("Task Failed".into())).await;
            return;
        }

        let final_node_ip = if !d_node_ip.is_empty() {
            d_node_ip
        } else {
            node_ip.clone()
        };

        let u_task_id = self.begin_upload_task(
            &api,
            &context,
            node_id_override
                .as_deref()
                .unwrap_or(selected.node_id.as_str()),
            &d_task_id,
        ).await;

        let (dl_res, ul_res) = if cfg.priority == TestPriority::DownloadFirst {
            let dl = self.run_workers(
                true,
                &final_node_ip,
                self.node_port,
                &d_task_id,
                cfg.duration_sec,
                cfg.concurrency,
                cfg.smoothing_window_sec,
                cfg.speed_refresh_ms,
                cfg.ping_refresh_ms,
                speed_adjuster,
                cfg.allow_official_cheat_calculation,
                tx.clone(),
                Arc::clone(&stop),
            ).await;
            let ul = self.run_workers(
                false,
                &final_node_ip,
                self.node_port,
                &u_task_id,
                cfg.duration_sec,
                cfg.concurrency,
                cfg.smoothing_window_sec,
                cfg.speed_refresh_ms,
                cfg.ping_refresh_ms,
                speed_adjuster,
                cfg.allow_official_cheat_calculation,
                tx.clone(),
                Arc::clone(&stop),
            ).await;
            (dl, ul)
        } else {
            let ul = self.run_workers(
                false,
                &final_node_ip,
                self.node_port,
                &u_task_id,
                cfg.duration_sec,
                cfg.concurrency,
                cfg.smoothing_window_sec,
                cfg.speed_refresh_ms,
                cfg.ping_refresh_ms,
                speed_adjuster,
                cfg.allow_official_cheat_calculation,
                tx.clone(),
                Arc::clone(&stop),
            ).await;
            let dl = self.run_workers(
                true,
                &final_node_ip,
                self.node_port,
                &d_task_id,
                cfg.duration_sec,
                cfg.concurrency,
                cfg.smoothing_window_sec,
                cfg.speed_refresh_ms,
                cfg.ping_refresh_ms,
                speed_adjuster,
                cfg.allow_official_cheat_calculation,
                tx.clone(),
                Arc::clone(&stop),
            ).await;
            (dl, ul)
        };

        if stop.load(Ordering::Relaxed) {
            return;
        }

        let _ = tx.send(ProgressEvent::TestFinished(TestResult {
            dl_avg: dl_res.0,
            dl_raw_avg: dl_res.1,
            dl_max: dl_res.2,
            ul_avg: ul_res.0,
            ul_raw_avg: ul_res.1,
            ul_max: ul_res.2,
            ping_idle: p_idle,
            jitter_idle: j_idle,
            ping_loaded: (dl_res.3 + ul_res.3) / 2.0,
            jitter_loaded: (dl_res.4 + ul_res.4) / 2.0,
            dl_bytes: dl_res.5,
            ul_bytes: ul_res.5,
            loaded_ping_samples: dl_res.6 + ul_res.6,
        })).await;
    }

    async fn resolve_user_ip(&self, prefetched_ip: Option<String>) -> String {
        if let Some(ip) = prefetched_ip.filter(|value| !value.is_empty()) {
            info!("Using prefetched IP: {}", ip);
            return ip;
        }
        self.fetch_user_ip().await
    }

    async fn resolve_region(&self, user_ip: &str) -> (String, String, String) {
        if user_ip.is_empty() {
            return ("Unknown".into(), "Unknown".into(), "Unknown".into());
        }

        let enc_ip = self.crypto.encrypt(user_ip);
        let region_url = format!(
            "{}?ip={}",
            join_base(&self.base_url, self.endpoints.query_region_dispatch_path),
            urlencoding::encode(&enc_ip)
        );

        if let Ok(resp) = self.client.get(&region_url)
            .headers(self.build_headers())
            .send()
            .await
        {
            if let Ok(json) = resp.json::<ApiResponse>().await {
                if let Some(plain) = json.data.as_str().map(|value| self.crypto.decrypt(value)) {
                    let parts: Vec<&str> = plain.split('|').collect();
                    if parts.len() >= 2 {
                        return (
                            parts[0].to_string(),
                            parts[1].to_string(),
                            parts.get(2).copied().unwrap_or("Unknown").to_string(),
                        );
                    }
                }
            }
        }

        ("Unknown".into(), "Unknown".into(), "Unknown".into())
    }

    async fn load_runtime_context(
        &self,
        api: &SpeedtestApi<'_>,
        province: &str,
        city: &str,
        isp: &str,
        user_ip: &str,
        tx: &mpsc::Sender<ProgressEvent>,
    ) -> RuntimeContext {
        let (dbw, ubw, account) = api.get_ip_info(province, city, isp, user_ip).await;
        let nodes = api.select_nodes_by_city(city).await;
        let _ = tx.send(ProgressEvent::UserInfo {
            user: account.clone(),
            ip: user_ip.to_string(),
            city: city.to_string(),
            bw: format!("{}/{}M", dbw, ubw),
        }).await;
        let _ = tx.send(ProgressEvent::NodesUpdate(nodes.clone())).await;

        RuntimeContext {
            province: province.to_string(),
            city: city.to_string(),
            isp: isp.to_string(),
            user_ip: user_ip.to_string(),
            dbw,
            ubw,
            account,
            nodes,
        }
    }

    async fn resolve_default_node(
        &self,
        api: &SpeedtestApi<'_>,
        context: &RuntimeContext,
        tx: &mpsc::Sender<ProgressEvent>,
    ) -> Option<crate::speedtest::types::NodeInfo> {
        let node = api.get_default_node(&DefaultNodeRequest {
            ip: &context.user_ip,
            city: &context.city,
            account: &context.account,
            down_bw: context.dbw,
            up_bw: context.ubw,
            operator: &context.isp,
            province: &context.province,
        }).await;

        if let Some(ref selected) = node {
            if !selected.node_ip.is_empty() {
                let _ = tx.send(ProgressEvent::NodeIpFound {
                    node_id: selected.node_id.clone(),
                    node_ip: selected.node_ip.clone(),
                }).await;
            }
        }

        node
    }

    async fn begin_download_task(
        &self,
        api: &SpeedtestApi<'_>,
        context: &RuntimeContext,
        node_id_override: Option<&str>,
        tx: &mpsc::Sender<ProgressEvent>,
    ) -> (String, String, SpeedAdjuster) {
        if let Some(data) = api.begin_test(&BeginTestRequest {
            dbw: context.dbw,
            ubw: context.ubw,
            city: &context.city,
            user_ip: &context.user_ip,
            province: &context.province,
            operator: &context.isp,
            mode: "Down",
            node_id: node_id_override.unwrap_or(""),
            is_sign_account: "",
            bd_account: &context.account,
            is_use_plug: 0,
            network_type: "",
            task_id: None,
        }).await {
            if !data.node_ip.is_empty() {
                let _ = tx.send(ProgressEvent::NodeIpFound {
                    node_id: data.node_id.clone(),
                    node_ip: data.node_ip.clone(),
                }).await;
            }
            let adjuster = SpeedAdjuster::from_begin_data(&data);
            return (data.task_id, data.node_ip, adjuster);
        }

        (
            String::new(),
            String::new(),
            SpeedAdjuster { is_ten_gig: false },
        )
    }

    async fn prefetch_partial_node_ips(
        &self,
        api: &SpeedtestApi<'_>,
        context: &RuntimeContext,
        tx: &mpsc::Sender<ProgressEvent>,
        max_nodes: usize,
    ) {
        if max_nodes == 0 {
            return;
        }

        let mut prefetched = 0usize;
        for node in &context.nodes {
            if prefetched >= max_nodes {
                break;
            }
            if node.node_id.is_empty() {
                continue;
            }
            if let Some(node_ip) = self.prefetch_node_ip_for(api, context, node.node_id.as_str()).await {
                let _ = tx.send(ProgressEvent::NodeIpFound {
                    node_id: node.node_id.clone(),
                    node_ip,
                }).await;
                prefetched += 1;
            }
        }
    }

    async fn prefetch_node_ip_for(
        &self,
        api: &SpeedtestApi<'_>,
        context: &RuntimeContext,
        node_id: &str,
    ) -> Option<String> {
        api.begin_test(&BeginTestRequest {
            dbw: context.dbw,
            ubw: context.ubw,
            city: &context.city,
            user_ip: &context.user_ip,
            province: &context.province,
            operator: &context.isp,
            mode: "Down",
            node_id,
            is_sign_account: "",
            bd_account: &context.account,
            is_use_plug: 0,
            network_type: "",
            task_id: None,
        }).await
            .and_then(|data| {
                if data.node_ip.is_empty() {
                    None
                } else {
                    Some(data.node_ip)
                }
            })
    }

    async fn begin_upload_task(
        &self,
        api: &SpeedtestApi<'_>,
        context: &RuntimeContext,
        node_id: &str,
        down_task_id: &str,
    ) -> String {
        api.begin_test(&BeginTestRequest {
            dbw: context.dbw,
            ubw: context.ubw,
            city: &context.city,
            user_ip: &context.user_ip,
            province: &context.province,
            operator: &context.isp,
            mode: "Up",
            node_id,
            is_sign_account: "",
            bd_account: &context.account,
            is_use_plug: 0,
            network_type: "",
            task_id: Some(down_task_id),
        }).await
            .map(|data| data.task_id)
            .unwrap_or_else(|| down_task_id.to_string())
    }

    async fn measure_ping(&self, ip: &str, port: u16, count: usize) -> (f64, f64) {
        let mut delays = Vec::new();
        let url = node_url(ip, port, self.endpoints.node_ping_path);
        for _ in 0..count {
            let start = Instant::now();
            if let Ok(resp) = self.client.get(&url)
                .timeout(Duration::from_secs(2))
                .send()
                .await
            {
                if resp.status() == 200 {
                    delays.push(start.elapsed().as_secs_f64() * 1000.0);
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        if delays.is_empty() {
            return (0.0, 0.0);
        }
        let avg = delays.iter().sum::<f64>() / delays.len() as f64;
        let jitter = if delays.len() > 1 {
            delays.iter().map(|&d| (d - avg).abs()).sum::<f64>() / delays.len() as f64
        } else {
            0.0
        };
        (avg, jitter)
    }

    async fn run_workers(
        &self,
        is_dl: bool,
        node_ip: &str,
        port: u16,
        task_id: &str,
        duration_sec: u64,
        concurrency: usize,
        smoothing_window_sec: f64,
        speed_refresh_ms: u64,
        ping_refresh_ms: u64,
        speed_adjuster: SpeedAdjuster,
        allow_official_cheat_calculation: bool,
        tx: mpsc::Sender<ProgressEvent>,
        stop: Arc<AtomicBool>,
    ) -> (f64, f64, f64, f64, f64, u64, usize) {
        let total_bytes = Arc::new(AtomicU64::new(0));
        let start_time = Instant::now();
        let end_time = start_time + Duration::from_secs(duration_sec);
        let cancel = Arc::new(AtomicBool::new(false));

        let interval = (speed_refresh_ms as f64 / 1000.0).max(0.05);
        let window_size = ((smoothing_window_sec.max(0.2) / interval) as usize).max(1);

        let mut handles = Vec::new();
        let origin = self.origin.clone();
        let referer = self.referer.clone();
        let client = self.client.clone();

        // Memory Optimization: Pre-allocate ONE shared upload buffer for all workers
        let up_data = if !is_dl {
            let mut up_data_raw = vec![0u8; 1024 * 1024];
            rand::rng().fill(&mut up_data_raw[..]);
            Some(bytes::Bytes::from(up_data_raw))
        } else {
            None
        };

        for _ in 0..concurrency {
            let tb = Arc::clone(&total_bytes);
            let stop_worker = Arc::clone(&stop);
            let cancel_worker = Arc::clone(&cancel);
            let origin = origin.clone();
            let referer = referer.clone();
            let client = client.clone();
            let up_data_shared = up_data.clone();
            let path = if is_dl {
                self.endpoints.node_download_path
            } else {
                self.endpoints.node_upload_path
            };
            let url = format!("{}?taskId={}", node_url(node_ip, port, path), task_id);

            handles.push(tokio::spawn(async move {
                while Instant::now() < end_time {
                    if cancel_worker.load(Ordering::Relaxed) || stop_worker.load(Ordering::Relaxed)
                    {
                        break;
                    }
                    if is_dl {
                        if let Ok(resp) = client.get(&url)
                            .header("Accept", "*/*")
                            .header("Origin", &origin)
                            .header("Referer", &referer)
                            .send()
                            .await
                        {
                            let mut stream = resp.bytes_stream();
                            use futures::StreamExt;
                            while let Some(item) = stream.next().await {
                                if let Ok(chunk) = item {
                                    let chunk: bytes::Bytes = chunk;
                                    tb.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                                }
                                if Instant::now() >= end_time {
                                    break;
                                }
                            }
                        }
                    } else {
                        let body_data = up_data_shared.as_ref().cloned().unwrap_or_default();
                        if let Ok(resp) = client.post(&url)
                            .header("Accept", "*/*")
                            .header("Origin", &origin)
                            .header("Referer", &referer)
                            .header("Content-Type", "application/octet-stream")
                            .body(body_data)
                            .send()
                            .await
                        {
                            if resp.status() == 200 {
                                tb.fetch_add(1024 * 1024, Ordering::Relaxed);
                            }
                        }
                    }
                }
            }));
        }

        let loaded_delays = Arc::new(Mutex::new(Vec::new()));
        let ld_clone = Arc::clone(&loaded_delays);
        let tx_ping = tx.clone();
        let cancel_ping = Arc::clone(&cancel);
        let stop_ping = Arc::clone(&stop);
        let origin_ping = self.origin.clone();
        let referer_ping = self.referer.clone();
        let ping_url = node_url(node_ip, port, self.endpoints.node_ping_path);
        let client_ping = self.client.clone();

        tokio::spawn(async move {
            while Instant::now() < end_time {
                if cancel_ping.load(Ordering::Relaxed) || stop_ping.load(Ordering::Relaxed) {
                    break;
                }
                let p_start = Instant::now();
                if let Ok(resp) = client_ping.get(&ping_url)
                    .header("Accept", "*/*")
                    .header("Origin", &origin_ping)
                    .header("Referer", &referer_ping)
                    .send()
                    .await
                {
                    if resp.status() == 200 {
                        let d = p_start.elapsed().as_secs_f64() * 1000.0;
                        let (_avg, jitter) = {
                            let mut lock = ld_clone.lock().unwrap();
                            lock.push(d);
                            let avg = lock.iter().sum::<f64>() / lock.len() as f64;
                            let jitter =
                                lock.iter().map(|&x| (x - avg).abs()).sum::<f64>() / lock.len() as f64;
                            (avg, jitter)
                        };
                        let _ = tx_ping.send(ProgressEvent::LatencyUpdate { ping: d, jitter }).await;
                    }
                }
                tokio::time::sleep(Duration::from_millis(ping_refresh_ms.max(50))).await;
            }
        });

        let mut samples = Vec::new();
        let mut rolling = RollingRateWindow::new(window_size);
        let mut last_bytes = 0u64;
        let mut last_time = start_time;

        while Instant::now() < end_time {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            tokio::time::sleep(Duration::from_millis((interval * 1000.0) as u64)).await;
            let now = Instant::now();
            let current_bytes = total_bytes.load(Ordering::Relaxed);
            let dt = now.duration_since(last_time).as_secs_f64();
            if dt > 0.0 {
                let db = current_bytes.saturating_sub(last_bytes);
                rolling.push(db, dt);
                let win_speed = rolling.bits_per_sec();
                let raw_mbps = win_speed / 1_000_000.0;
                let display_mbps = if allow_official_cheat_calculation {
                    if is_dl {
                        speed_adjuster.adjust_download_mbps(raw_mbps)
                    } else {
                        speed_adjuster.adjust_upload_mbps(raw_mbps)
                    }
                } else {
                    raw_mbps
                };
                samples.push((
                    now.duration_since(start_time).as_secs_f64(),
                    display_mbps * 1_000_000.0,
                ));
                let ratio = (now.duration_since(start_time).as_secs_f64() / duration_sec as f64)
                    .min(1.0) as f32;
                let _ = tx.send(if is_dl {
                    ProgressEvent::DownloadUpdate {
                        ratio,
                        speed: display_mbps,
                        raw_speed: raw_mbps,
                    }
                } else {
                    ProgressEvent::UploadUpdate {
                        ratio,
                        speed: display_mbps,
                        raw_speed: raw_mbps,
                    }
                }).await;
                last_bytes = current_bytes;
                last_time = now;
            }
        }

        cancel.store(true, Ordering::Relaxed);
        for h in handles {
            let _ = h.await;
        }

        if samples.is_empty() {
            return (0.0, 0.0, 0.0, 0.0, 0.0, total_bytes.load(Ordering::Relaxed), 0);
        }
        let speed_stats = SampleStats::from_samples(&samples, duration_sec, smoothing_window_sec);
        let ld = loaded_delays.lock().unwrap();
        let delay_stats = DelayStats::from_values(&ld);
        let total = total_bytes.load(Ordering::Relaxed);
        let loaded_n = ld.len();
        
        let avg_raw = if allow_official_cheat_calculation {
            let multiplier = if is_dl { 1.14 } else { 1.09 };
            speed_stats.avg_bps / 1_000_000.0 / multiplier
        } else {
            speed_stats.avg_bps / 1_000_000.0
        };

        (
            speed_stats.avg_bps / 1_000_000.0,
            avg_raw,
            speed_stats.max_bps / 1_000_000.0,
            delay_stats.avg_ms,
            delay_stats.jitter_ms,
            total,
            loaded_n,
        )
    }
}

struct RuntimeContext {
    province: String,
    city: String,
    isp: String,
    user_ip: String,
    dbw: i64,
    ubw: i64,
    account: String,
    nodes: Vec<crate::speedtest::types::NodeInfo>,
}
