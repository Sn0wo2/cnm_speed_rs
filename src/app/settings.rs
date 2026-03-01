use crate::tui;
use crossterm::event::{self, KeyCode, KeyEvent};

pub enum SettingsRouteOutcome {
    None,
    ReloadRequested,
}

pub fn route_settings_key(state: &mut tui::AppState, key: KeyEvent) -> SettingsRouteOutcome {
    let ctrl = key.modifiers.contains(event::KeyModifiers::CONTROL);

    match key.code {
        KeyCode::Esc => {
            tui::settings_apply_input(state);
            tui::settings_toggle(state);
        }
        KeyCode::Up => tui::settings_prev_field(state),
        KeyCode::Down | KeyCode::Tab => tui::settings_next_field(state),
        KeyCode::BackTab => tui::settings_prev_field(state),
        KeyCode::Left => tui::settings_adjust(state, -1),
        KeyCode::Right => tui::settings_adjust(state, 1),
        KeyCode::Char('c') if ctrl => tui::copy_results_to_clipboard(state),
        KeyCode::Char('s') if ctrl => tui::copy_summary_to_clipboard(state),
        KeyCode::Enter => match state.settings_focus {
            tui::SettingsField::Reload => return SettingsRouteOutcome::ReloadRequested,
            tui::SettingsField::Priority | tui::SettingsField::AllowOfficialCheatCalculation => {
                tui::settings_adjust(state, 1)
            }
            _ => tui::settings_apply_input(state),
        },
        _ => tui::settings_handle_key(state, key),
    }

    SettingsRouteOutcome::None
}
