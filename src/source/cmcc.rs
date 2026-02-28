use crate::app::types::Args;
use crate::source::cmcc_types::{ApiResponse, PROVINCES};
use crate::source::{SourceSelection, SpeedSource};
use crate::speedtest::types::{ActiveTestHandle, ProgressEvent, RuntimeConfig};
use crate::speedtest::{SpeedTester, SpeedtestEndpoints};
use log::{info, warn};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

const DEFAULT_NODE_PORT: u16 = 18989;
const DETECT_TIMEOUT_MS: u64 = 900;
const DETECT_EARLY_EXIT_MS: u128 = 120;
const DEFAULT_SCHEME: &str = "http";
const DEFAULT_HOST_PREFIX: &str = "speed";
const DEFAULT_DOMAIN: &str = "chinamobile.com";
const DEFAULT_BASE_PROVINCE: &str = "zj";
const DEFAULT_BASE_PORT: u16 = 28190;

const TASK_PAGE_PATH: &str = "/speedtest/task.html";
const GET_USER_IP_PATH: &str = "/speedtest/userCommon/getUserIp";
const QUERY_REGION_DISPATCH_PATH: &str = "/speedtest/queryRegion/dispatch";
const GET_IP_INFO_PATH: &str = "/speedtest/stdispatch/getIpInfo";
const SELECT_NODE_BY_CITY_PATH: &str = "/speedtest/stnode/selectNodeByCity";
const GET_DEFAULT_NODE_PATH: &str = "/speedtest/stdispatch/getDefaltNode";
const BEGIN_TEST_PATH: &str = "/speedtest/stdispatch/beginTest";

const NODE_PING_PATH: &str = "/speed/ping";
const NODE_DOWNLOAD_PATH: &str = "/speed/download";
const NODE_UPLOAD_PATH: &str = "/speed/upload";

pub struct CmccSource;

#[derive(Debug, Clone)]
struct ProbeResult {
    base_url: String,
    label: String,
    latency_ms: u128,
    user_ip: String,
}

impl CmccSource {
    pub fn new() -> Self {
        Self
    }

    pub fn build_base_url_for_province(&self, province_code: &str) -> String {
        format!(
            "{}://{}.{}.{}:{}",
            DEFAULT_SCHEME, DEFAULT_HOST_PREFIX, province_code, DEFAULT_DOMAIN, DEFAULT_BASE_PORT
        )
    }

    pub fn build_fallback_base_url(&self) -> String {
        self.build_base_url_for_province(DEFAULT_BASE_PROVINCE)
    }

    pub fn join_base_url(&self, base_url: &str, path: &str) -> String {
        format!("{}{}", base_url.trim_end_matches('/'), path)
    }

    pub fn build_endpoints(&self) -> SpeedtestEndpoints {
        SpeedtestEndpoints {
            task_page_path: TASK_PAGE_PATH,
            get_user_ip_path: GET_USER_IP_PATH,
            query_region_dispatch_path: QUERY_REGION_DISPATCH_PATH,
            get_ip_info_path: GET_IP_INFO_PATH,
            select_node_by_city_path: SELECT_NODE_BY_CITY_PATH,
            get_default_node_path: GET_DEFAULT_NODE_PATH,
            begin_test_path: BEGIN_TEST_PATH,
            node_ping_path: NODE_PING_PATH,
            node_download_path: NODE_DOWNLOAD_PATH,
            node_upload_path: NODE_UPLOAD_PATH,
        }
    }

    pub fn build_tester(&self, selection: &SourceSelection) -> Arc<SpeedTester> {
        Arc::new(SpeedTester::new(
            selection.base_url.clone(),
            DEFAULT_NODE_PORT,
            self.build_endpoints(),
        ))
    }

    pub fn detect_forced(&self, args: &Args) -> Option<SourceSelection> {
        if let Some(base) = &args.base_url {
            info!("Base URL forced by args: {}", base);
            return Some(SourceSelection {
                base_url: base.clone(),
                label: "Manual".to_string(),
                prefetched_ip: String::new(),
            });
        }
        None
    }

    pub fn detect_by_province(&self, args: &Args) -> Option<SourceSelection> {
        let code = args.province.as_ref()?;
        let url = self.build_base_url_for_province(code);
        let label = PROVINCES
            .iter()
            .find(|p| p.code == code.as_str())
            .map(|p| p.name.to_string())
            .unwrap_or_else(|| code.clone());
        Some(SourceSelection {
            base_url: url,
            label,
            prefetched_ip: String::new(),
        })
    }

    pub fn detect_auto(&self, tx: &mpsc::Sender<ProgressEvent>) -> SourceSelection {
        let deadline = Instant::now() + Duration::from_millis(DETECT_TIMEOUT_MS + 500);
        let (rtx, rrx) = mpsc::channel::<ProbeResult>();

        for province in PROVINCES {
            let rtx = rtx.clone();
            let base = self.build_base_url_for_province(province.code);
            let label = province.name.to_string();
            let probe = self.join_base_url(&base, GET_USER_IP_PATH);
            std::thread::spawn(move || {
                let start = Instant::now();
                if let Ok(mut resp) = ureq::get(&probe)
                    .config()
                    .timeout_global(Some(Duration::from_millis(DETECT_TIMEOUT_MS)))
                    .build()
                    .call()
                {
                    if resp.status() == 200 {
                        let latency_ms = start.elapsed().as_millis();
                        if let Ok(json) = resp.body_mut().read_json::<ApiResponse>() {
                            let _ = rtx.send(ProbeResult {
                                base_url: base,
                                label,
                                latency_ms,
                                user_ip: json.data.as_str().unwrap_or("").to_string(),
                            });
                        }
                    }
                }
            });
        }
        drop(rtx);

        let mut best: Option<ProbeResult> = None;
        let mut received = 0usize;
        let _ = tx.send(ProgressEvent::Status(
            "Detecting server... (0 replies)".into(),
        ));

        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match rrx.recv_timeout(remaining.min(Duration::from_millis(250))) {
                Ok(probe) => {
                    received += 1;
                    info!(
                        "Probe ok province={} latency={}ms url={} ip={}",
                        probe.label, probe.latency_ms, probe.base_url, probe.user_ip
                    );
                    if best
                        .as_ref()
                        .map(|b| probe.latency_ms < b.latency_ms)
                        .unwrap_or(true)
                    {
                        best = Some(probe);
                    }
                    if let Some(best_probe) = &best {
                        let _ = tx.send(ProgressEvent::Status(format!(
                            "Detecting server... ({} replies, best: {} {}ms)",
                            received, best_probe.label, best_probe.latency_ms
                        )));
                        if best_probe.latency_ms <= DETECT_EARLY_EXIT_MS {
                            break;
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    let _ = tx.send(ProgressEvent::Status(format!(
                        "Detecting server... ({} replies)",
                        received
                    )));
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        if let Some(best_probe) = best {
            let _ = tx.send(ProgressEvent::Status(format!(
                "Auto-selected {} ({}ms)",
                best_probe.label, best_probe.latency_ms
            )));
            return SourceSelection {
                base_url: best_probe.base_url,
                label: best_probe.label,
                prefetched_ip: best_probe.user_ip,
            };
        }

        warn!("No province server reachable, fallback to zj");
        SourceSelection {
            base_url: self.build_fallback_base_url(),
            label: "Zhejiang (fallback)".to_string(),
            prefetched_ip: String::new(),
        }
    }
}

impl SpeedSource for CmccSource {
    fn detect(
        &self,
        args: &Args,
        tx: &mpsc::Sender<ProgressEvent>,
    ) -> Result<SourceSelection, String> {
        if let Some(selection) = self.detect_forced(args) {
            return Ok(selection);
        }

        let _ = tx.send(ProgressEvent::Status(
            "Detecting fastest province server...".into(),
        ));

        if let Some(selection) = self.detect_by_province(args) {
            return Ok(selection);
        }

        Ok(self.detect_auto(tx))
    }

    fn spawn_test(
        &self,
        selection: &SourceSelection,
        cfg: RuntimeConfig,
        node_id_override: Option<String>,
        tx: mpsc::Sender<ProgressEvent>,
        prefetched_ip: Option<String>,
    ) -> ActiveTestHandle {
        self.build_tester(selection)
            .spawn_test(cfg, node_id_override, tx, prefetched_ip)
    }

    fn run_test(
        &self,
        selection: &SourceSelection,
        cfg: RuntimeConfig,
        node_id_override: Option<String>,
        tx: mpsc::Sender<ProgressEvent>,
        stop: Arc<AtomicBool>,
        prefetched_ip: Option<String>,
    ) {
        self.build_tester(selection)
            .run_test(cfg, node_id_override, tx, stop, prefetched_ip)
    }
}
