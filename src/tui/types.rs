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
}

#[derive(Default)]
pub struct UserContext {
    pub name: String,
    pub ip: String,
    pub city: String,
    pub bandwidth: String,
}

#[derive(Default)]
pub struct LiveStats {
    pub ping: f64,
    pub jitter: f64,
    pub dl_speed: f64,
    pub dl_ratio: f32,
    pub ul_speed: f64,
    pub ul_ratio: f32,
    pub dl_final: Option<f64>,
    pub ul_final: Option<f64>,
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

    pub settings: RuntimeConfig,
    pub settings_open: bool,
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
            dl_hist: VecDeque::with_capacity(101),
            ul_hist: VecDeque::with_capacity(101),
            hits: HitBoxes::empty(),
            timeline: VecDeque::with_capacity(32),
            settings: RuntimeConfig::default(),
            settings_open: false,
            settings_focus: SettingsField::Concurrency,
            settings_input: tui_input::Input::default(),
        }
    }
}
