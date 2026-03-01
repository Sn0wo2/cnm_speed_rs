use super::types::{AppState, SettingsField};
use crate::speedtest::types::TestPriority;
use crossterm::event::{KeyCode, KeyEvent};
use tui_input::backend::crossterm::EventHandler;

pub fn settings_toggle(state: &mut AppState) {
    state.settings_open = !state.settings_open;
    if state.settings_open {
        sync_input_to_field(state);
    }
}

pub fn settings_next_field(state: &mut AppState) {
    settings_apply_input(state);
    state.settings_focus = match state.settings_focus {
        SettingsField::Concurrency => SettingsField::Duration,
        SettingsField::Duration => SettingsField::Smoothing,
        SettingsField::Smoothing => SettingsField::SpeedRefresh,
        SettingsField::SpeedRefresh => SettingsField::PingRefresh,
        SettingsField::PingRefresh => SettingsField::Priority,
        SettingsField::Priority => SettingsField::AllowOfficialCheatCalculation,
        SettingsField::AllowOfficialCheatCalculation => SettingsField::Concurrency,
    };
    sync_input_to_field(state);
}

pub fn settings_prev_field(state: &mut AppState) {
    settings_apply_input(state);
    state.settings_focus = match state.settings_focus {
        SettingsField::Concurrency => SettingsField::AllowOfficialCheatCalculation,
        SettingsField::Duration => SettingsField::Concurrency,
        SettingsField::Smoothing => SettingsField::Duration,
        SettingsField::SpeedRefresh => SettingsField::Smoothing,
        SettingsField::PingRefresh => SettingsField::SpeedRefresh,
        SettingsField::Priority => SettingsField::PingRefresh,
        SettingsField::AllowOfficialCheatCalculation => SettingsField::Priority,
    };

    sync_input_to_field(state);
}

fn sync_input_to_field(state: &mut AppState) {
    let val = match state.settings_focus {
        SettingsField::Concurrency => state.settings.concurrency.to_string(),
        SettingsField::Duration => state.settings.duration_sec.to_string(),
        SettingsField::Smoothing => format!("{:.1}", state.settings.smoothing_window_sec),
        SettingsField::SpeedRefresh => state.settings.speed_refresh_ms.to_string(),
        SettingsField::PingRefresh => state.settings.ping_refresh_ms.to_string(),
        SettingsField::Priority => String::new(),
        SettingsField::AllowOfficialCheatCalculation => String::new(),
    };
    state.settings_input = tui_input::Input::new(val);
}

pub fn settings_adjust(state: &mut AppState, delta: i32) {
    match state.settings_focus {
        SettingsField::Concurrency => {
            state.settings.concurrency =
                (state.settings.concurrency as i32 + delta).clamp(1, 64) as usize;
            sync_input_to_field(state);
        }
        SettingsField::Duration => {
            state.settings.duration_sec =
                (state.settings.duration_sec as i32 + delta).clamp(3, 120) as u64;
            sync_input_to_field(state);
        }
        SettingsField::Smoothing => {
            state.settings.smoothing_window_sec =
                (state.settings.smoothing_window_sec + delta as f64 * 0.1).clamp(0.2, 5.0);
            sync_input_to_field(state);
        }
        SettingsField::SpeedRefresh => {
            state.settings.speed_refresh_ms = (state.settings.speed_refresh_ms as i32
                + if delta > 0 { 20 } else { -20 })
            .clamp(50, 1000) as u64;
            sync_input_to_field(state);
        }
        SettingsField::PingRefresh => {
            state.settings.ping_refresh_ms = (state.settings.ping_refresh_ms as i32
                + if delta > 0 { 20 } else { -20 })
            .clamp(50, 2000) as u64;
            sync_input_to_field(state);
        }
        SettingsField::Priority => {
            state.settings.priority = if state.settings.priority == TestPriority::DownloadFirst {
                TestPriority::UploadFirst
            } else {
                TestPriority::DownloadFirst
            };
        }
        SettingsField::AllowOfficialCheatCalculation => {
            state.settings.allow_official_cheat_calculation =
                !state.settings.allow_official_cheat_calculation;
        }
    }
}

pub fn settings_handle_key(state: &mut AppState, key: KeyEvent) {
    if !state.settings_open {
        return;
    }

    // Handle Ctrl+K
    if key
        .modifiers
        .contains(crossterm::event::KeyModifiers::CONTROL)
    {
        if let KeyCode::Char('k') = key.code {
            state.settings_input = tui_input::Input::default();
            return;
        }
    }

    // Block character input for toggle-only fields
    if matches!(
        state.settings_focus,
        SettingsField::Priority | SettingsField::AllowOfficialCheatCalculation
    ) {
        if let KeyCode::Left | KeyCode::Right = key.code {
            settings_adjust(state, 1); // Toggle
        }
        return;
    }

    let is_nav = matches!(
        key.code,
        KeyCode::Left
            | KeyCode::Right
            | KeyCode::Home
            | KeyCode::End
            | KeyCode::Backspace
            | KeyCode::Delete
    );
    let is_char = match key.code {
        KeyCode::Char(c) => c.is_ascii_digit() || c == '.',
        _ => false,
    };

    if is_nav || is_char {
        state
            .settings_input
            .handle_event(&crossterm::event::Event::Key(key));
        settings_apply_input_live(state);
    }
}

pub fn settings_apply_input(state: &mut AppState) {
    let val = state.settings_input.value();
    if val.is_empty() {
        return;
    }
    match state.settings_focus {
        SettingsField::Concurrency => {
            if let Ok(v) = val.parse() {
                state.settings.concurrency = v;
            }
        }
        SettingsField::Duration => {
            if let Ok(v) = val.parse() {
                state.settings.duration_sec = v;
            }
        }
        SettingsField::Smoothing => {
            if let Ok(v) = val.parse() {
                state.settings.smoothing_window_sec = v;
            }
        }
        SettingsField::SpeedRefresh => {
            if let Ok(v) = val.parse() {
                state.settings.speed_refresh_ms = v;
            }
        }
        SettingsField::PingRefresh => {
            if let Ok(v) = val.parse() {
                state.settings.ping_refresh_ms = v;
            }
        }
        SettingsField::Priority => {}
        SettingsField::AllowOfficialCheatCalculation => {}
    }
}

fn settings_apply_input_live(state: &mut AppState) {
    let val = state.settings_input.value();
    if val.is_empty() {
        return;
    }
    match state.settings_focus {
        SettingsField::Concurrency => {
            if let Ok(v) = val.parse::<usize>() {
                state.settings.concurrency = v.clamp(1, 64);
            }
        }
        SettingsField::Duration => {
            if let Ok(v) = val.parse::<u64>() {
                state.settings.duration_sec = v.clamp(3, 120);
            }
        }
        SettingsField::Smoothing => {
            if let Ok(v) = val.parse::<f64>() {
                state.settings.smoothing_window_sec = v.clamp(0.2, 5.0);
            }
        }
        SettingsField::SpeedRefresh => {
            if let Ok(v) = val.parse::<u64>() {
                state.settings.speed_refresh_ms = v.clamp(50, 1000);
            }
        }
        SettingsField::PingRefresh => {
            if let Ok(v) = val.parse::<u64>() {
                state.settings.ping_refresh_ms = v.clamp(50, 2000);
            }
        }
        SettingsField::Priority => {}
        SettingsField::AllowOfficialCheatCalculation => {}
    }
}
