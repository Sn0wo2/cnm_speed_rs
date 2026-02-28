use super::state::start_test;
use super::types::{AppState, ClickAction};
use ratatui::layout::{Margin, Rect};

pub fn handle_click(state: &mut AppState, x: u16, y: u16) -> ClickAction {
    if rect_contains(state.hits.settings_btn, x, y) {
        return ClickAction::ToggleSettings;
    }
    if rect_contains(state.hits.quit_btn, x, y) {
        return ClickAction::Quit;
    }
    if rect_contains(state.hits.start_btn, x, y) {
        if state.running {
            return ClickAction::Start(None);
        }
        if let Some(node_opt) = start_test(state) {
            return ClickAction::Start(node_opt);
        }
        return ClickAction::None;
    }
    if rect_contains(state.hits.nodes_rect, x, y) {
        select_node_by_click(state, y);
    }
    ClickAction::None
}

fn select_node_by_click(state: &mut AppState, y: u16) {
    let inner = state.hits.nodes_rect.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });
    let first_row = inner.y.saturating_add(1);
    if y < first_row {
        return;
    }
    let idx = (y - first_row) as usize;
    let visible_rows = inner.height.saturating_sub(1) as usize;
    if idx < visible_rows && idx < state.nodes.len() {
        state.selected_idx = idx;
        state.node = state.nodes[idx].name.clone();
    }
}

fn rect_contains(r: Rect, x: u16, y: u16) -> bool {
    x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height
}
