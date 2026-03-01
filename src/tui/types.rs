use crate::speedtest::types::{NodeInfo, RuntimeConfig, TestResult};
use ratatui::layout::Rect;
use std::collections::VecDeque;
use std::time::Instant;

#[derive(Clone, Copy)]
pub struct HitBoxes {
    pub start_btn: Rect,
    pub quit_btn: Rect,
    pub settings_btn: Rect,
    pub nodes_rect: Rect,
}

impl HitBoxes {
    pub fn empty() -> Self {
        Self {
            start_btn: Rect::new(0, 0, 0, 0),
            quit_btn: Rect::new(0, 0, 0, 0),
            settings_btn: Rect::new(0, 0, 0, 0),
            nodes_rect: Rect::new(0, 0, 0, 0),
        }
    }
}

pub enum ClickAction {
    None,
    Quit,
    ToggleSettings,
    Start(Option<String>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SettingsField {
    Concurrency,
    Duration,
    Smoothing,
    SpeedRefresh,
    PingRefresh,
    Priority,
    AllowOfficialCheatCalculation,
    Reload,
}

#[derive(Default, Clone)]
pub struct UserContext {
    pub name: String,
    pub ip: String,
    pub city: String,
    pub bandwidth: String,
}

#[derive(Default, Clone)]
pub struct LiveStats {
    pub ping: f64,
    pub jitter: f64,
    pub packet_total: usize,
    pub packet_failed: usize,
    pub dl_speed: f64,
    pub dl_raw_speed: f64,
    pub dl_ratio: f32,
    pub ul_speed: f64,
    pub ul_raw_speed: f64,
    pub ul_ratio: f32,
    pub dl_trend_start_ratio: Option<f32>,
    pub ul_trend_start_ratio: Option<f32>,
    pub dl_final: Option<f64>,
    pub dl_raw_final: Option<f64>,
    pub ul_final: Option<f64>,
    pub ul_raw_final: Option<f64>,
}

pub struct AppState {
    pub status: String,
    pub user_context: UserContext,
    pub live_stats: LiveStats,

    pub node: String,
    pub nodes: Vec<NodeInfo>,
    pub results: Option<TestResult>,
    pub running: bool,
    pub selected_idx: usize,

    pub base_url: String,
    pub province_label: String,
    pub started_at: Option<Instant>,

    pub dl_hist: VecDeque<f64>,
    pub ul_hist: VecDeque<f64>,
    pub hits: HitBoxes,
    pub timeline: VecDeque<String>,
    pub log_scroll_offset: usize,
    pub log_auto_scroll: bool,

    pub throbber_state: throbber_widgets_tui::ThrobberState,

    pub settings: RuntimeConfig,
    pub settings_open: bool,
    pub settings_dirty: bool,
    pub settings_focus: SettingsField,
    pub settings_input: tui_input::Input,
}

impl AppState {
    pub fn new(base_url: String, province_label: String) -> Self {
        Self {
            status: "Ready".into(),
            user_context: UserContext::default(),
            live_stats: LiveStats::default(),
            node: "-".into(),
            nodes: vec![],
            results: None,
            running: false,
            selected_idx: 0,
            base_url,
            province_label,
            started_at: None,
            dl_hist: VecDeque::with_capacity(1000),
            ul_hist: VecDeque::with_capacity(1000),
            hits: HitBoxes::empty(),
            timeline: VecDeque::with_capacity(512),
            log_scroll_offset: 0,
            log_auto_scroll: true,
            throbber_state: throbber_widgets_tui::ThrobberState::default(),
            settings: RuntimeConfig::default(),
            settings_open: false,
            settings_dirty: false,
            settings_focus: SettingsField::Concurrency,
            settings_input: tui_input::Input::default(),
        }
    }
}
