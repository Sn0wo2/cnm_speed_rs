pub mod components;
mod io;
mod mouse;
mod render;
mod settings;
mod state;
mod types;

pub use io::{backend, terminal};
pub use mouse::handle_click;
pub use render::draw;
pub use settings::{
    settings_adjust, settings_apply_input, settings_handle_key, settings_next_field,
    settings_prev_field, settings_sync_input, settings_toggle,
};
pub use state::{
    apply_event, copy_results_to_clipboard, copy_summary_to_clipboard, push_timeline,
    select_next_node, select_prev_node, start_test, stop_test,
};
pub use types::{AppState, ClickAction, SettingsField};
