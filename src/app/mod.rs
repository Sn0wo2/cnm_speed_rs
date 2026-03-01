use crate::source::SourceRuntime;
use crate::speedtest::types::{ProgressEvent, RuntimeConfig};
use crate::tui;
use anyhow::{Context, Result};
use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind, MouseEvent, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use fs2::FileExt;
use std::{fs::OpenOptions, io, time::Duration};
use tokio::sync::mpsc;
use tracing::error;

pub mod logging;
mod settings;
pub mod types;
use crate::app::logging::LoggerManager;
use crate::app::settings::{route_settings_key, SettingsRouteOutcome};
use types::Args;

pub struct AppRuntime {
    pub state: tui::AppState,
    pub source_runtime: SourceRuntime,
    pub rx: mpsc::Receiver<ProgressEvent>,
    pub _logger: LoggerManager,
}

impl AppRuntime {
    pub fn new(args: &Args) -> (Self, mpsc::Sender<ProgressEvent>) {
        let (tx, rx) = mpsc::channel::<ProgressEvent>(100);
        let logger = LoggerManager::init().expect("initializing loggers");
        let settings = Self::load_settings_from_file().unwrap_or_else(RuntimeConfig::default);
        
        let mut state = tui::AppState::new("(detecting...)".into(), "Auto".into());
        state.status = "Detecting fastest server...".into();
        state.settings = settings;
        if args.duration != 10 { state.settings.duration_sec = args.duration; state.settings_dirty = true; }
        if args.concurrency != 8 { state.settings.concurrency = args.concurrency; state.settings_dirty = true; }

        (Self { state, source_runtime: SourceRuntime::new(tx.clone()), rx, _logger: logger }, tx)
    }

    fn load_settings_from_file() -> Option<RuntimeConfig> {
        std::fs::read_to_string("data/settings.json").ok().and_then(|s| serde_json::from_str(&s).ok())
    }

    pub fn save_settings(&self) {
        if !self.state.settings_dirty { return; }
        let json = serde_json::to_string_pretty(&self.state.settings).unwrap_or_default();
        let _ = std::fs::create_dir_all("data");
        let _ = std::fs::write("data/settings.json", json);
    }

    pub async fn run_loop(&mut self, terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<io::Stdout>>) {
        let mut last_area = terminal.size().ok();
        'app: loop {
            // 1. Process all pending events from backend
            while let Ok(ev) = self.rx.try_recv() {
                tui::apply_event(&mut self.state, ev);
            }

            // 2. Render UI
            self.state.throbber_state.calc_next();
            if let Err(e) = terminal.draw(|f| tui::draw(f, &mut self.state)) {
                error!("Draw error: {}", e); break 'app;
            }

            // 3. Handle input
            if !event::poll(Duration::from_millis(16)).unwrap_or(false) {
                tokio::task::yield_now().await; continue;
            }

            match event::read() {
                Ok(Event::Key(k)) if k.kind == KeyEventKind::Press => if self.handle_key(k).await { break 'app; },
                Ok(Event::Mouse(m)) => if self.handle_mouse(m).await { break 'app; },
                Ok(Event::Resize(_, _)) => {
                    if let Ok(new_area) = terminal.size() {
                        if Some(new_area) != last_area {
                            self.state.settings_open = false;
                            last_area = Some(new_area);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    pub async fn handle_key(&mut self, key: KeyEvent) -> bool {
        if self.state.settings_open {
            return self.handle_settings_key(key).await;
        }

        let ctrl = key.modifiers.contains(event::KeyModifiers::CONTROL);

        match key.code {
            KeyCode::Esc => { tui::settings_toggle(&mut self.state); }
            KeyCode::Char('q') => return true,
            KeyCode::Char('c') if ctrl => { tui::copy_results_to_clipboard(&mut self.state); }
            KeyCode::Char('s') if ctrl => { tui::copy_summary_to_clipboard(&mut self.state); }
            KeyCode::Up => { tui::select_prev_node(&mut self.state); }
            KeyCode::Down => { tui::select_next_node(&mut self.state); }
            KeyCode::Enter | KeyCode::Char('s') => {
                if self.state.running {
                    tui::stop_test(&mut self.state);
                    self.source_runtime.stop_test();
                } else {
                    if !self.source_runtime.is_ready() {
                        tui::push_timeline(
                            &mut self.state.timeline,
                            "Server not ready, wait for detection...".into(),
                        );
                        self.state.status = "Detecting fastest server...".into();
                        return false;
                    }
                    let cfg = self.state.settings.clone();
                    let node = tui::start_test(&mut self.state).unwrap_or(None);
                    self.source_runtime.spawn_test(cfg, node);
                }
            }
            KeyCode::Char('c') => { tui::copy_results_to_clipboard(&mut self.state); }
            _ => {}
        }
        false
    }

    async fn handle_settings_key(&mut self, key: KeyEvent) -> bool {
        if matches!(
            route_settings_key(&mut self.state, key),
            SettingsRouteOutcome::ReloadRequested
        ) {
            if let Some(settings) = Self::load_settings_from_file() {
                self.state.settings = settings; self.state.settings_dirty = false;
                tui::settings_sync_input(&mut self.state);
                tui::push_timeline(&mut self.state.timeline, "Settings reloaded from disk".into());
            }
        }
        false
    }

    pub async fn handle_mouse(&mut self, mouse: MouseEvent) -> bool {
        match mouse.kind {
            MouseEventKind::Down(_) => {
                let was_running = self.state.running;
                let action = tui::handle_click(&mut self.state, mouse.column, mouse.row);
                let cfg = self.state.settings.clone();

                match action {
                    tui::ClickAction::Quit => return true,
                    tui::ClickAction::ToggleSettings => { tui::settings_toggle(&mut self.state); }
                    tui::ClickAction::Start(node) => {
                        if was_running {
                            tui::stop_test(&mut self.state);
                            self.source_runtime.stop_test();
                        } else {
                            if !self.source_runtime.is_ready() {
                                tui::push_timeline(
                                    &mut self.state.timeline,
                                    "Server not ready, wait for detection...".into(),
                                );
                                self.state.status = "Detecting fastest server...".into();
                                return false;
                            }
                            let selected_node = if node.is_some() {
                                node
                            } else {
                                tui::start_test(&mut self.state).unwrap_or(None)
                            };
                            self.source_runtime.spawn_test(cfg, selected_node);
                        }
                    }
                    _ => {}
                }
            }
            MouseEventKind::ScrollUp => {
                self.state.log_auto_scroll = false;
                self.state.log_scroll_offset = self.state.log_scroll_offset.saturating_sub(2);
            }
            MouseEventKind::ScrollDown => {
                self.state.log_scroll_offset += 2;
                // Auto-scroll will be re-enabled if render detects we're at or beyond max_offset
                // But for immediate feedback, we can check if we want to snap back
            }
            _ => {}
        }
        false
    }
}

pub async fn run() -> Result<()> {
    let _ = std::fs::create_dir_all("data");
    let lock_file = OpenOptions::new().read(true).write(true).create(true).open("data/.runtime.lock").context("failed to open lock file")?;
    if lock_file.try_lock_exclusive().is_err() {
        eprintln!("\n  \x1b[31m\x1b[1m[!]\x1b[0m \x1b[1manother instance is already running\x1b[0m\n      \x1b[2mhelp: concurrent instances are restricted to prevent corruption\x1b[0m\n");
        return Ok(());
    }
    #[cfg(windows)] {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("attrib").arg("+h").arg("data/.runtime.lock").creation_flags(0x08000000).spawn();
    }

    let args = Args::parse();
    let (mut runtime, _) = AppRuntime::new(&args);
    runtime.source_runtime.bootstrap_detection(args.clone());

    enable_raw_mode().context("failed to enable raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let mut terminal = tui::terminal(tui::backend(stdout))?;

    runtime.run_loop(&mut terminal).await;
    runtime.save_settings();

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    Ok(())
}
