use crate::source::SourceRuntime;
use crate::speedtest::types::{ProgressEvent, RuntimeConfig};
use crate::tui;
use clap::Parser;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use log::{error, info, warn};
use std::sync::mpsc;
use std::{
    io,
    sync::{Arc, Mutex},
    time::Duration,
};

pub mod logging;
pub mod types;

use logging::init_logging;
use types::Args;

type SharedState = Arc<Mutex<tui::AppState>>;

pub struct AppRuntime {
    pub state: SharedState,
    pub source_runtime: SourceRuntime,
    pub rx: mpsc::Receiver<ProgressEvent>,
}

impl AppRuntime {
    pub fn new(args: &Args) -> Self {
        let (tx, rx) = mpsc::channel::<ProgressEvent>();
        let state = Arc::new(Mutex::new(tui::AppState::new(
            "(detecting...)".into(),
            "Auto".into(),
        )));
        {
            let mut s = state.lock().unwrap();
            s.status = "Detecting fastest server...".into();
            s.settings.duration_sec = args.duration;
            s.settings.concurrency = args.concurrency;
        }

        Self {
            state,
            source_runtime: SourceRuntime::new(tx.clone()),
            rx,
        }
    }

    pub fn bootstrap_detection(&self, args: Args) {
        self.source_runtime.bootstrap_detection(args);
    }

    pub fn run_loop(
        &self,
        terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<io::Stdout>>,
    ) {
        'app: loop {
            while let Ok(ev) = self.rx.try_recv() {
                let mut s = self.state.lock().unwrap();
                tui::apply_event(&mut s, ev);
            }

            if let Err(e) = terminal.draw(|f| tui::draw(f, &self.state)) {
                error!("Draw error: {}", e);
                break 'app;
            }

            if !event::poll(Duration::from_millis(50)).unwrap_or(false) {
                continue;
            }

            match event::read() {
                Ok(Event::Key(k)) if k.kind == KeyEventKind::Press => {
                    if self.handle_key(k) {
                        break 'app;
                    }
                }
                Ok(Event::Mouse(m)) => {
                    if self.handle_mouse(m) {
                        break 'app;
                    }
                }
                Ok(_) => {}
                Err(e) => warn!("Event read error: {}", e),
            }
        }
    }

    pub fn handle_key(&self, key: KeyEvent) -> bool {
        if self.handle_settings_key(key) {
            return false;
        }

        match key.code {
            KeyCode::Esc => {
                let mut s = self.state.lock().unwrap();
                tui::settings_toggle(&mut s);
            }
            KeyCode::Char('q') => return true,
            KeyCode::Char('c') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                let mut s = self.state.lock().unwrap();
                tui::copy_results_to_clipboard(&mut s);
            }
            KeyCode::Char('s') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                let mut s = self.state.lock().unwrap();
                tui::copy_summary_to_clipboard(&mut s);
            }
            KeyCode::Up => {
                let mut s = self.state.lock().unwrap();
                tui::select_prev_node(&mut s);
            }
            KeyCode::Down => {
                let mut s = self.state.lock().unwrap();
                tui::select_next_node(&mut s);
            }
            KeyCode::Enter | KeyCode::Char('s') => self.toggle_test(None),
            KeyCode::Char('c') => {
                let mut s = self.state.lock().unwrap();
                tui::copy_results_to_clipboard(&mut s);
            }
            _ => {}
        }

        false
    }

    pub fn handle_settings_key(&self, key: KeyEvent) -> bool {
        let mut s = self.state.lock().unwrap();
        if !s.settings_open {
            return false;
        }

        match key.code {
            KeyCode::Esc => {
                tui::settings_apply_input(&mut s);
                tui::settings_toggle(&mut s);
            }
            KeyCode::Up => tui::settings_prev_field(&mut s),
            KeyCode::Down | KeyCode::Tab => tui::settings_next_field(&mut s),
            KeyCode::BackTab => tui::settings_prev_field(&mut s),
            KeyCode::Char('c') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                tui::copy_results_to_clipboard(&mut s);
            }
            KeyCode::Char('s') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                tui::copy_summary_to_clipboard(&mut s);
            }
            KeyCode::Enter => tui::settings_apply_input(&mut s),
            _ => tui::settings_handle_key(&mut s, key),
        }

        true
    }

    pub fn handle_mouse(&self, mouse: MouseEvent) -> bool {
        if !matches!(mouse.kind, MouseEventKind::Down(_)) {
            return false;
        }

        let mut s = self.state.lock().unwrap();
        let was_running = s.running;
        let click = tui::handle_click(&mut s, mouse.column, mouse.row);
        let runtime_cfg = s.settings.clone();
        drop(s);

        match click {
            tui::ClickAction::None => false,
            tui::ClickAction::Quit => true,
            tui::ClickAction::ToggleSettings => {
                let mut s = self.state.lock().unwrap();
                tui::settings_toggle(&mut s);
                false
            }
            tui::ClickAction::Start(node_opt) => {
                if was_running {
                    self.stop_test();
                    return false;
                }
                self.spawn_test(runtime_cfg, node_opt);
                false
            }
        }
    }

    pub fn toggle_test(&self, selected_node: Option<Option<String>>) {
        let runtime_cfg = {
            let s = self.state.lock().unwrap();
            s.settings.clone()
        };
        self.toggle_test_with(runtime_cfg, selected_node.unwrap_or(None));
    }

    pub fn toggle_test_with(&self, runtime_cfg: RuntimeConfig, selected_node: Option<String>) {
        let mut s = self.state.lock().unwrap();
        if s.running {
            drop(s);
            self.stop_test();
            return;
        }

        let node = selected_node.or_else(|| tui::start_test(&mut s).unwrap_or(None));
        drop(s);

        self.source_runtime.spawn_test(runtime_cfg, node);
    }

    pub fn spawn_test(&self, runtime_cfg: RuntimeConfig, node: Option<String>) {
        self.source_runtime.spawn_test(runtime_cfg, node);
    }

    pub fn stop_test(&self) {
        let mut s = self.state.lock().unwrap();
        tui::stop_test(&mut s);
        drop(s);
        self.source_runtime.stop_test();
    }
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    init_logging();
    info!("Boot args: {:?}", args);
    info!("CWD: {:?}", std::env::current_dir());

    let runtime = AppRuntime::new(&args);
    runtime.bootstrap_detection(args.clone());

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = tui::backend(stdout);
    let mut terminal = tui::terminal(backend)?;
    info!("Entered TUI mode");

    let loop_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime.run_loop(&mut terminal)
    }));

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(panic) = loop_result {
        error!("Panic in event loop: {:?}", panic);
        return Err("TUI panicked; see runtime.log".into());
    }

    Ok(())
}
