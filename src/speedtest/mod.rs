use crate::source::cmcc_types::{ApiResponse, BeginTestData};
use crate::speedtest::api::{BeginTestRequest, DefaultNodeRequest, SpeedtestApi};
use crate::speedtest::types::{
    ActiveTestHandle, ProgressEvent, RuntimeConfig, TestPriority, TestResult, NodeInfo,
};
use crate::utils::crypto::CMCCCrypto;
use crate::utils::stats::{OnlineDelayStats, RollingRateWindow, SampleStats};
use rand::RngExt as _;
use reqwest::Client;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::error;

mod api;
pub mod types;

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/133.0.0.0 Safari/537.36";
const IDLE_PING_TIMEOUT_MS: u64 = 700;
const LOAD_PING_TIMEOUT_MS: u64 = 700;
const PING_GAP_MS: u64 = 10;
const MIN_PING_REFRESH_MS: u64 = 20;
const NODE_IP_PREFETCH_RETRIES: usize = 3;
const BEGIN_TEST_RETRIES: usize = 3;

#[derive(Clone)]
struct DiscoveryContext {
    province: String,
    city: String,
    isp: String,
    user_ip: String,
    dbw: i64,
    ubw: i64,
    account: String,
    nodes: Vec<NodeInfo>,
}

struct PhaseResult {
    avg_mbps: f64,
    raw_avg_mbps: f64,
    max_mbps: f64,
    ping: f64,
    jitter: f64,
    bytes: u64,
    failed_count: usize,
    total_count: usize,
}

impl PhaseResult {
    fn empty() -> Self {
        Self {
            avg_mbps: 0.0,
            raw_avg_mbps: 0.0,
            max_mbps: 0.0,
            ping: 0.0,
            jitter: 0.0,
            bytes: 0,
            failed_count: 0,
            total_count: 0,
        }
    }
}

struct PingResult {
    avg_ms: f64,
    jitter_ms: f64,
    failed_count: usize,
    total_count: usize,
}

struct WorkerRunResult {
    avg_mbps: f64,
    raw_avg_mbps: f64,
    max_mbps: f64,
    ping_ms: f64,
    jitter_ms: f64,
    bytes: u64,
    failed_count: usize,
    total_count: usize,
}

impl WorkerRunResult {
    fn empty(bytes: u64, failed_count: usize, total_count: usize) -> Self {
        Self {
            avg_mbps: 0.0,
            raw_avg_mbps: 0.0,
            max_mbps: 0.0,
            ping_ms: 0.0,
            jitter_ms: 0.0,
            bytes,
            failed_count,
            total_count,
        }
    }
}

pub struct SpeedTester {
    pub client: Client,
    pub ping_client: Client,
    pub crypto: CMCCCrypto,
    pub base_url: String,
    pub origin: String,
    pub referer: String,
    pub node_port: u16,
    pub endpoints: SpeedtestEndpoints,
    cached_context: Mutex<Option<DiscoveryContext>>,
    cached_default_node: Mutex<Option<NodeInfo>>,
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

fn join_base(base_url: &str, path: &str) -> String {
    format!("{}{}", base_url.trim_end_matches('/'), path)
}

impl SpeedTester {
    pub fn parse_begin_test(&self, data: serde_json::Value) -> Option<BeginTestData> {
        if data.is_string() {
            let s = data.as_str().unwrap();
            if s == "{}" || s.is_empty() { return None; }
            if let Ok(nested) = serde_json::from_str::<BeginTestData>(s) { return Some(nested); }
        }
        serde_json::from_value::<BeginTestData>(data).ok()
    }

    pub fn new(base_url: String, node_port: u16, endpoints: SpeedtestEndpoints) -> Self {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .tcp_nodelay(true)
            .pool_max_idle_per_host(64)
            .pool_idle_timeout(Duration::from_secs(90))
            .timeout(Duration::from_secs(10)) // Add global timeout to prevent zombie connection blocks
            .build()
            .unwrap_or_default();

        let ping_client = Client::builder()
            .user_agent(USER_AGENT)
            .tcp_nodelay(true)
            .pool_max_idle_per_host(16)
            .pool_idle_timeout(Duration::from_secs(30))
            .timeout(Duration::from_millis(LOAD_PING_TIMEOUT_MS + 200))
            .build()
            .unwrap_or_default();
        
        Self {
            client,
            ping_client,
            crypto: CMCCCrypto::new(),
            origin: base_url.clone(),
            referer: join_base(&base_url, endpoints.task_page_path),
            base_url,
            node_port,
            endpoints,
            cached_context: Mutex::new(None),
            cached_default_node: Mutex::new(None),
        }
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
        self.emit_status(&tx, "Connecting...").await;

        let context = match self.resolve_context_for_test(prefetched_ip, &tx, is_init).await {
            Ok(ctx) => ctx,
            Err(_) => return,
        };

        if stop.load(Ordering::Relaxed) { return; }

        let api = SpeedtestApi::new(self);

        let selected_node = match self.resolve_node_for_test(&api, &context, &tx, node_id_override.as_deref()).await {
            Some(n) => n,
            None => {
                self.emit_status(&tx, "No Server").await;
                if !is_init {
                    self.emit_abort(&tx, "No server available").await;
                }
                return;
            }
        };

        let node_ip = self.resolve_node_ip_for_test(&api, &context, &selected_node, &tx).await;

        if node_ip.is_empty() {
            self.emit_status(&tx, "Node IP unresolved, trying task handshake...").await;
        }

        let idle_ping = self.measure_idle_ping(&tx, &node_ip, is_init).await;

        if is_init {
            self.emit_status(&tx, "Ready").await;
            self.prefetch_partial_node_ips(&api, &context, &tx, 3).await;
            return;
        }

        self.emit_status(&tx, "Testing...").await;
        
        let target_node_id = if node_id_override.is_some() { node_id_override.as_deref() } else { Some(selected_node.node_id.as_str()) };
        
        let (d_task_id, d_node_ip, adjuster) = self.begin_download_task(&api, &context, target_node_id, &tx).await;
        if d_task_id.is_empty() {
            self.emit_status(&tx, "Task Error").await;
            self.emit_abort(&tx, "Failed to create test task").await;
            return;
        }

        let test_node_ip = if !d_node_ip.is_empty() { d_node_ip } else { node_ip };
        if test_node_ip.is_empty() {
            self.emit_status(&tx, "IP Resolution Failed").await;
            self.emit_abort(&tx, "Node IP resolution failed").await;
            return;
        }
        let u_task_id = self.begin_upload_task(&api, &context, target_node_id.unwrap_or(&selected_node.node_id), &d_task_id).await;

        let empty_phase = PhaseResult::empty();

        let (dl_res, ul_res) = match cfg.priority {
            TestPriority::DownloadFirst => {
                let dl = self.run_phase(true, &test_node_ip, &d_task_id, &cfg, adjuster, tx.clone(), Arc::clone(&stop)).await;
                let ul = self.run_phase(false, &test_node_ip, &u_task_id, &cfg, adjuster, tx.clone(), Arc::clone(&stop)).await;
                (dl, ul)
            }
            TestPriority::UploadFirst => {
                let ul = self.run_phase(false, &test_node_ip, &u_task_id, &cfg, adjuster, tx.clone(), Arc::clone(&stop)).await;
                let dl = self.run_phase(true, &test_node_ip, &d_task_id, &cfg, adjuster, tx.clone(), Arc::clone(&stop)).await;
                (dl, ul)
            }
            TestPriority::DownloadOnly => {
                let dl = self.run_phase(true, &test_node_ip, &d_task_id, &cfg, adjuster, tx.clone(), Arc::clone(&stop)).await;
                (dl, empty_phase)
            }
            TestPriority::UploadOnly => {
                let ul = self.run_phase(false, &test_node_ip, &u_task_id, &cfg, adjuster, tx.clone(), Arc::clone(&stop)).await;
                (empty_phase, ul)
            }
        };

        if stop.load(Ordering::Relaxed) {
            self.emit_abort(&tx, "Stopped").await;
            return;
        }

        let _ = tx.send(ProgressEvent::TestFinished(TestResult {
            dl_avg: dl_res.avg_mbps, dl_raw_avg: dl_res.raw_avg_mbps, dl_max: dl_res.max_mbps,
            ul_avg: ul_res.avg_mbps, ul_raw_avg: ul_res.raw_avg_mbps, ul_max: ul_res.max_mbps,
            ping_idle: idle_ping.avg_ms, jitter_idle: idle_ping.jitter_ms,
            ping_idle_total: idle_ping.total_count, ping_idle_failed: idle_ping.failed_count,
            ping_dl: dl_res.ping, jitter_dl: dl_res.jitter,
            ping_dl_total: dl_res.total_count, ping_dl_failed: dl_res.failed_count,
            ping_ul: ul_res.ping, jitter_ul: ul_res.jitter,
            ping_ul_total: ul_res.total_count, ping_ul_failed: ul_res.failed_count,
            dl_bytes: dl_res.bytes, ul_bytes: ul_res.bytes,
        })).await;
    }

    async fn emit_status(&self, tx: &mpsc::Sender<ProgressEvent>, status: &str) {
        let _ = tx.send(ProgressEvent::Status(status.to_string())).await;
    }

    async fn emit_abort(&self, tx: &mpsc::Sender<ProgressEvent>, reason: &str) {
        let _ = tx
            .send(ProgressEvent::TestAborted {
                reason: reason.to_string(),
            })
            .await;
    }

    async fn resolve_context_for_test(
        &self,
        prefetched_ip: Option<String>,
        tx: &mpsc::Sender<ProgressEvent>,
        is_init: bool,
    ) -> Result<DiscoveryContext, ()> {
        match self.get_or_discover_context(prefetched_ip, tx).await {
            Ok(ctx) => Ok(ctx),
            Err(_) => {
                self.emit_status(tx, "Discovery Failed").await;
                if !is_init {
                    self.emit_abort(tx, "Discovery failed").await;
                }
                Err(())
            }
        }
    }

    async fn resolve_node_for_test(
        &self,
        api: &SpeedtestApi<'_>,
        context: &DiscoveryContext,
        tx: &mpsc::Sender<ProgressEvent>,
        node_id_override: Option<&str>,
    ) -> Option<NodeInfo> {
        if let Some(target_id) = node_id_override
            && let Some(node) = context.nodes.iter().find(|n| n.node_id == target_id).cloned()
        {
            return Some(node);
        }

        self.get_or_resolve_node(api, context, tx)
            .await
            .or_else(|| context.nodes.first().cloned())
    }

    async fn resolve_node_ip_for_test(
        &self,
        api: &SpeedtestApi<'_>,
        context: &DiscoveryContext,
        selected_node: &NodeInfo,
        tx: &mpsc::Sender<ProgressEvent>,
    ) -> String {
        if !selected_node.node_ip.is_empty() {
            return selected_node.node_ip.clone();
        }

        for _ in 0..NODE_IP_PREFETCH_RETRIES {
            if let Some(ip) = self
                .prefetch_node_ip_for(api, context, &selected_node.node_id)
                .await
            {
                let _ = tx
                    .send(ProgressEvent::NodeIpFound {
                        node_id: selected_node.node_id.clone(),
                        node_ip: ip.clone(),
                    })
                    .await;
                return ip;
            }
            tokio::time::sleep(Duration::from_millis(120)).await;
        }

        String::new()
    }

    async fn measure_idle_ping(
        &self,
        tx: &mpsc::Sender<ProgressEvent>,
        node_ip: &str,
        is_init: bool,
    ) -> PingResult {
        self.emit_status(
            tx,
            &format!(
                "Latency target: http://{}:{}{}",
                node_ip, self.node_port, self.endpoints.node_ping_path
            ),
        )
        .await;

        let idle_ping = self
            .measure_ping(node_ip, self.node_port, if is_init { 2 } else { 5 })
            .await;

        let _ = tx
            .send(ProgressEvent::LatencyUpdate {
                ping: idle_ping.avg_ms,
                jitter: idle_ping.jitter_ms,
                failed_count: idle_ping.failed_count,
                total_count: idle_ping.total_count,
            })
            .await;

        idle_ping
    }

    async fn get_or_discover_context(&self, prefetched_ip: Option<String>, tx: &mpsc::Sender<ProgressEvent>) -> Result<DiscoveryContext, ()> {
        let user_ip = if let Some(ip) = prefetched_ip.filter(|s| !s.is_empty()) { ip } else { self.fetch_user_ip().await };
        if user_ip.is_empty() { return Err(()); }

        let cached_opt = {
            let cached = self.cached_context.lock().unwrap();
            cached.clone()
        };

        if let Some(ctx) = cached_opt {
            if ctx.user_ip == user_ip {
                let _ = tx.send(ProgressEvent::UserInfo { user: ctx.account.clone(), ip: ctx.user_ip.clone(), city: ctx.city.clone(), bw: format!("{}/{}M", ctx.dbw, ctx.ubw) }).await;
                let _ = tx.send(ProgressEvent::NodesUpdate(ctx.nodes.clone())).await;
                return Ok(ctx);
            }
        }

        let (province, city, isp) = self.resolve_region(&user_ip).await;
        let api = SpeedtestApi::new(self);
        let (dbw, ubw, account) = api.get_ip_info(&province, &city, &isp, &user_ip).await;
        let nodes = api.select_nodes_by_city(&city).await;

        let ctx = DiscoveryContext { province: province.clone(), city: city.clone(), isp: isp.clone(), user_ip: user_ip.clone(), dbw, ubw, account, nodes };
        let _ = tx.send(ProgressEvent::UserInfo { user: ctx.account.clone(), ip: ctx.user_ip.clone(), city: ctx.city.clone(), bw: format!("{}/{}M", ctx.dbw, ctx.ubw) }).await;
        let _ = tx.send(ProgressEvent::NodesUpdate(ctx.nodes.clone())).await;

        let mut cached = self.cached_context.lock().unwrap();
        *cached = Some(ctx.clone());
        Ok(ctx)
    }

    async fn get_or_resolve_node(&self, api: &SpeedtestApi<'_>, ctx: &DiscoveryContext, tx: &mpsc::Sender<ProgressEvent>) -> Option<NodeInfo> {
        let cached = self.cached_default_node.lock().unwrap().clone();
        if let Some(n) = cached {
            if ctx.nodes.iter().any(|node| node.node_id == n.node_id) { return Some(n); }
        }

        let node = api.get_default_node(&DefaultNodeRequest {
            ip: &ctx.user_ip, city: &ctx.city, account: &ctx.account,
            down_bw: ctx.dbw, up_bw: ctx.ubw, operator: &ctx.isp, province: &ctx.province,
        }).await;

        if let Some(ref n) = node {
            {
                let mut cache = self.cached_default_node.lock().unwrap();
                *cache = Some(n.clone());
            }
            if !n.node_ip.is_empty() {
                let _ = tx.send(ProgressEvent::NodeIpFound { node_id: n.node_id.clone(), node_ip: n.node_ip.clone() }).await;
            }
        }
        node
    }

    async fn run_phase(&self, is_dl: bool, ip: &str, task_id: &str, cfg: &RuntimeConfig, adjuster: SpeedAdjuster, tx: mpsc::Sender<ProgressEvent>, stop: Arc<AtomicBool>) -> PhaseResult {
        let result = self.run_workers(
            is_dl, ip, self.node_port, task_id, cfg.duration_sec, cfg.concurrency,
            cfg.smoothing_window_sec, cfg.speed_refresh_ms, cfg.ping_refresh_ms,
            adjuster, cfg.allow_official_cheat_calculation, tx, stop
        ).await;
        PhaseResult {
            avg_mbps: result.avg_mbps,
            raw_avg_mbps: result.raw_avg_mbps,
            max_mbps: result.max_mbps,
            ping: result.ping_ms,
            jitter: result.jitter_ms,
            bytes: result.bytes,
            failed_count: result.failed_count,
            total_count: result.total_count,
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
        let url = join_base(&self.base_url, self.endpoints.get_user_ip_path);
        match self.client.get(&url).headers(self.build_headers()).send().await {
            Ok(resp) => {
                if let Ok(json) = resp.json::<ApiResponse>().await {
                    return json.data.as_str().unwrap_or("").to_string();
                }
            }
            Err(e) => error!("IP fetch failed: {}", e),
        }
        String::new()
    }

    async fn resolve_region(&self, user_ip: &str) -> (String, String, String) {
        if user_ip.is_empty() { return ("Unknown".into(), "Unknown".into(), "Unknown".into()); }
        let enc_ip = self.crypto.encrypt(user_ip);
        let url = format!("{}?ip={}", join_base(&self.base_url, self.endpoints.query_region_dispatch_path), urlencoding::encode(&enc_ip));
        if let Ok(resp) = self.client.get(&url).headers(self.build_headers()).send().await {
            if let Ok(json) = resp.json::<ApiResponse>().await {
                if let Some(plain) = json.data.as_str().map(|v| self.crypto.decrypt(v)) {
                    let parts: Vec<&str> = plain.split('|').collect();
                    if parts.len() >= 2 {
                        return (parts[0].to_string(), parts[1].to_string(), parts.get(2).copied().unwrap_or("Unknown").to_string());
                    }
                }
            }
        }
        ("Unknown".into(), "Unknown".into(), "Unknown".into())
    }

    async fn measure_ping(&self, ip: &str, port: u16, count: usize) -> PingResult {
        let mut delays = Vec::new();
        let mut failed = 0;
        let url = format!("http://{}:{}{}", ip, port, self.endpoints.node_ping_path);
        
        // Warm-up request to establish TCP/HTTP connection and DNS caching
        let _ = self
            .ping_client
            .get(&url)
            .header("Origin", &self.origin)
            .header("Referer", &self.referer)
            .header("Connection", "keep-alive")
            .timeout(Duration::from_millis(IDLE_PING_TIMEOUT_MS))
            .send()
            .await;
        
        for _ in 0..count {
            let start = Instant::now();
            let req = self
                .ping_client
                .get(&url)
                .header("Origin", &self.origin)
                .header("Referer", &self.referer)
                .header("Connection", "keep-alive")
                .timeout(Duration::from_millis(IDLE_PING_TIMEOUT_MS));
            if let Ok(resp) = req.send().await {
                if resp.status() == 200 { 
                    delays.push(start.elapsed().as_secs_f64() * 1000.0); 
                } else {
                    failed += 1;
                }
            } else {
                failed += 1;
            }
            tokio::time::sleep(Duration::from_millis(PING_GAP_MS)).await;
        }
        if delays.is_empty() {
            return PingResult {
                avg_ms: 0.0,
                jitter_ms: 0.0,
                failed_count: failed,
                total_count: count,
            };
        }
        let avg = delays.iter().sum::<f64>() / delays.len() as f64;
        let jitter = if delays.len() > 1 { delays.iter().map(|&d| (d - avg).abs()).sum::<f64>() / delays.len() as f64 } else { 0.0 };
        PingResult {
            avg_ms: avg,
            jitter_ms: jitter,
            failed_count: failed,
            total_count: count,
        }
    }

    async fn run_workers(
        &self, is_dl: bool, node_ip: &str, port: u16, task_id: &str, duration_sec: u64, concurrency: usize,
        smoothing_window_sec: f64, speed_refresh_ms: u64, ping_refresh_ms: u64,
        speed_adjuster: SpeedAdjuster, allow_cheat: bool, tx: mpsc::Sender<ProgressEvent>, stop: Arc<AtomicBool>
    ) -> WorkerRunResult {
        let total_bytes = Arc::new(AtomicU64::new(0));
        let start_time = Instant::now();
        let end_time = start_time + Duration::from_secs(duration_sec);
        let cancel = Arc::new(AtomicBool::new(false));
        let interval = (speed_refresh_ms as f64 / 1000.0).max(0.05);
        let window_size = ((smoothing_window_sec.max(0.2) / interval) as usize).max(1);

        let mut handles = Vec::new();
        let up_data = if !is_dl {
            let mut buf = vec![0u8; 1024 * 1024];
            rand::rng().fill(&mut buf[..]);
            Some(bytes::Bytes::from(buf))
        } else { None };

        let url = format!("http://{}:{}{}?taskId={}", node_ip, port, if is_dl { self.endpoints.node_download_path } else { self.endpoints.node_upload_path }, task_id);

        for _ in 0..concurrency {
            let (tb, s, c, cl, ud, u, org, refr) = (Arc::clone(&total_bytes), Arc::clone(&stop), Arc::clone(&cancel), self.client.clone(), up_data.clone(), url.clone(), self.origin.clone(), self.referer.clone());
            handles.push(tokio::spawn(async move {
                while Instant::now() < end_time {
                    if c.load(Ordering::Relaxed) || s.load(Ordering::Relaxed) { break; }
                    if is_dl {
                        if let Ok(resp) = cl.get(&u).header("Origin", &org).header("Referer", &refr).send().await {
                            let mut stream = resp.bytes_stream();
                            use futures::StreamExt;
                            while let Some(Ok(chunk)) = stream.next().await {
                                tb.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                                if Instant::now() >= end_time || c.load(Ordering::Relaxed) { break; }
                            }
                        }
                    } else {
                        if let Ok(resp) = cl.post(&u).header("Origin", &org).header("Referer", &refr).body(ud.clone().unwrap()).send().await {
                            if resp.status() == 200 { tb.fetch_add(1024 * 1024, Ordering::Relaxed); }
                        }
                    }
                }
            }));
        }

        let loaded_delay_stats = Arc::new(std::sync::Mutex::new(OnlineDelayStats::default()));
        let loaded_failed = Arc::new(AtomicUsize::new(0));
        let loaded_total = Arc::new(AtomicUsize::new(0));
        let (ld, lf, lt, tx_p, s_p, c_p, cl_p, p_url, org_p, ref_p) = (
            Arc::clone(&loaded_delay_stats), Arc::clone(&loaded_failed), Arc::clone(&loaded_total),
            tx.clone(), Arc::clone(&stop), Arc::clone(&cancel), self.ping_client.clone(),
            format!("http://{}:{}{}", node_ip, port, self.endpoints.node_ping_path),
            self.origin.clone(), self.referer.clone()
        );
        let ping_handle = tokio::spawn(async move {
            // Warm-up ping for loaded latency
            let _ = cl_p
                .get(&p_url)
                .header("Origin", &org_p)
                .header("Referer", &ref_p)
                .header("Connection", "keep-alive")
                .timeout(Duration::from_millis(LOAD_PING_TIMEOUT_MS))
                .send()
                .await;
            
            while Instant::now() < end_time {
                if c_p.load(Ordering::Relaxed) || s_p.load(Ordering::Relaxed) { break; }
                let start = Instant::now();
                lt.fetch_add(1, Ordering::Relaxed);
                match cl_p
                    .get(&p_url)
                    .header("Origin", &org_p)
                    .header("Referer", &ref_p)
                    .header("Connection", "keep-alive")
                    .timeout(Duration::from_millis(LOAD_PING_TIMEOUT_MS))
                    .send()
                    .await
                {
                    Ok(resp) if resp.status() == 200 => {
                        let d = start.elapsed().as_secs_f64() * 1000.0;
                        let jitter = {
                            let mut lock = ld.lock().unwrap();
                            lock.push(d);
                            lock.jitter_ms()
                        };
                        let _ = tx_p.send(ProgressEvent::LatencyUpdate { 
                            ping: d, 
                            jitter,
                            failed_count: lf.load(Ordering::Relaxed),
                            total_count: lt.load(Ordering::Relaxed),
                        }).await;
                    }
                    _ => {
                        lf.fetch_add(1, Ordering::Relaxed);
                    }
                }
                tokio::time::sleep(Duration::from_millis(ping_refresh_ms.max(MIN_PING_REFRESH_MS))).await;
            }
        });

        let mut samples = Vec::new();
        let mut rolling = RollingRateWindow::new(window_size);
        let (mut lb, mut lt) = (0u64, start_time);

        while Instant::now() < end_time {
            if stop.load(Ordering::Relaxed) { break; }
            tokio::time::sleep(Duration::from_millis((interval * 1000.0) as u64)).await;
            let now = Instant::now();
            let cb = total_bytes.load(Ordering::Relaxed);
            let dt = now.duration_since(lt).as_secs_f64();
            if dt > 0.0 {
                rolling.push(cb.saturating_sub(lb), dt);
                let raw_mbps = rolling.bits_per_sec() / 1_000_000.0;
                let display_mbps = if allow_cheat { if is_dl { speed_adjuster.adjust_download_mbps(raw_mbps) } else { speed_adjuster.adjust_upload_mbps(raw_mbps) } } else { raw_mbps };
                samples.push((now.duration_since(start_time).as_secs_f64(), display_mbps * 1_000_000.0));
                
                let ratio = (now.duration_since(start_time).as_secs_f64() / duration_sec as f64) as f32;
                
                let _ = tx.send(if is_dl { 
                    ProgressEvent::DownloadUpdate { ratio, speed: display_mbps, raw_speed: raw_mbps } 
                } else { 
                    ProgressEvent::UploadUpdate { ratio, speed: display_mbps, raw_speed: raw_mbps } 
                }).await;
                
                lb = cb; lt = now;
            }
        }

        cancel.store(true, Ordering::Relaxed);
        for h in handles { let _ = h.await; }
        let _ = ping_handle.await;

        let failed_count = loaded_failed.load(Ordering::Relaxed);
        let total_count = loaded_total.load(Ordering::Relaxed);
        if samples.is_empty() { 
            return WorkerRunResult::empty(
                total_bytes.load(Ordering::Relaxed),
                failed_count,
                total_count,
            );
        }
        let stats = SampleStats::from_samples(&samples, duration_sec, smoothing_window_sec);
        let ld_final = loaded_delay_stats.lock().unwrap();
        let avg_raw = if allow_cheat { stats.avg_bps / 1_000_000.0 / (if is_dl { 1.14 } else { 1.09 }) } else { stats.avg_bps / 1_000_000.0 };
        WorkerRunResult {
            avg_mbps: stats.avg_bps / 1_000_000.0,
            raw_avg_mbps: avg_raw,
            max_mbps: stats.max_bps / 1_000_000.0,
            ping_ms: ld_final.avg_ms(),
            jitter_ms: ld_final.jitter_ms(),
            bytes: total_bytes.load(Ordering::Relaxed),
            failed_count,
            total_count,
        }
    }

    async fn begin_download_task(&self, api: &SpeedtestApi<'_>, context: &DiscoveryContext, node_id_override: Option<&str>, tx: &mpsc::Sender<ProgressEvent>) -> (String, String, SpeedAdjuster) {
        for _ in 0..BEGIN_TEST_RETRIES {
            if let Some(data) = api
                .begin_test(&BeginTestRequest {
                    dbw: context.dbw,
                    ubw: context.ubw,
                    city: &context.city,
                    user_ip: &context.user_ip,
                    province: &context.province,
                    operator: &context.isp,
                    mode: "Down",
                    node_id: node_id_override.unwrap_or(""),
                    bd_account: &context.account,
                    is_sign_account: "",
                    is_use_plug: 0,
                    network_type: "",
                    task_id: None,
                })
                .await
            {
                if !data.node_ip.is_empty() {
                    let _ = tx
                        .send(ProgressEvent::NodeIpFound {
                            node_id: data.node_id.clone(),
                            node_ip: data.node_ip.clone(),
                        })
                        .await;
                }
                let task_id = data.task_id.clone();
                let node_ip = data.node_ip.clone();
                let adjuster = SpeedAdjuster::from_begin_data(&data);
                return (task_id, node_ip, adjuster);
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
        (String::new(), String::new(), SpeedAdjuster { is_ten_gig: false })
    }

    async fn begin_upload_task(&self, api: &SpeedtestApi<'_>, context: &DiscoveryContext, node_id: &str, down_task_id: &str) -> String {
        api.begin_test(&BeginTestRequest {
            dbw: context.dbw, ubw: context.ubw, city: &context.city, user_ip: &context.user_ip, province: &context.province,
            operator: &context.isp, mode: "Up", node_id, bd_account: &context.account,
            is_sign_account: "", is_use_plug: 0, network_type: "", task_id: Some(down_task_id),
        }).await.map(|d| d.task_id).unwrap_or_else(|| down_task_id.to_string())
    }

    async fn prefetch_node_ip_for(&self, api: &SpeedtestApi<'_>, context: &DiscoveryContext, node_id: &str) -> Option<String> {
        api.begin_test(&BeginTestRequest {
            dbw: context.dbw, ubw: context.ubw, city: &context.city, user_ip: &context.user_ip, province: &context.province,
            operator: &context.isp, mode: "Down", node_id, bd_account: &context.account,
            is_sign_account: "", is_use_plug: 0, network_type: "", task_id: None,
        }).await.and_then(|d| if d.node_ip.is_empty() { None } else { Some(d.node_ip) })
    }

    async fn prefetch_partial_node_ips(&self, api: &SpeedtestApi<'_>, context: &DiscoveryContext, tx: &mpsc::Sender<ProgressEvent>, max: usize) {
        let mut n = 0;
        for node in &context.nodes {
            if n >= max { break; }
            if let Some(ip) = self.prefetch_node_ip_for(api, context, &node.node_id).await {
                let _ = tx.send(ProgressEvent::NodeIpFound { node_id: node.node_id.clone(), node_ip: ip }).await;
                n += 1;
            }
        }
    }
}

#[derive(Clone, Copy)]
struct SpeedAdjuster { is_ten_gig: bool }
impl SpeedAdjuster {
    fn from_begin_data(data: &BeginTestData) -> Self { Self { is_ten_gig: match &data.is_ten_thousand {
        serde_json::Value::Bool(v) => *v,
        serde_json::Value::Number(v) => v.as_i64() == Some(1),
        serde_json::Value::String(v) => v.trim() == "1" || v.eq_ignore_ascii_case("true"),
        _ => false
    }}}
    fn adjust_download_mbps(&self, raw: f64) -> f64 { self.adjust(raw, 1.14) }
    fn adjust_upload_mbps(&self, raw: f64) -> f64 { self.adjust(raw, 1.09) }
    fn adjust(&self, raw: f64, mult: f64) -> f64 {
        if self.is_ten_gig {
            if raw < 8000.0 { raw * 1.125 } else if raw < 9000.0 { raw + 1000.0 } else { raw }
        } else { raw * mult }
    }
}
