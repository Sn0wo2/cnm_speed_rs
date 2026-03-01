use super::types::AppState;
use crate::speedtest::types::ProgressEvent;
use crate::utils::format::format_bytes;
use crate::utils::trend::TrendRenderer;
use std::collections::VecDeque;
use std::time::Instant;

pub fn apply_event(state: &mut AppState, ev: ProgressEvent) {
    let trend_renderer = TrendRenderer::default();
    match ev {
        ProgressEvent::Status(st) => {
            state.status = st.clone();
            push_timeline(&mut state.timeline, st);
        }
        ProgressEvent::ServerSelected {
            base_url,
            province_label,
        } => {
            state.base_url = base_url.clone();
            state.province_label = province_label.clone();
            state.status = format!("Server ready: {}", province_label);
            push_timeline(
                &mut state.timeline,
                format!("Selected server: {} ({})", province_label, base_url),
            );
        }
        ProgressEvent::UserInfo { user, ip, city, bw } => {
            state.user_context.name = user;
            state.user_context.ip = ip;
            state.user_context.city = city;
            state.user_context.bandwidth = bw;
        }
        ProgressEvent::NodesUpdate(nodes) => {
            for new_node in nodes {
                if let Some(existing) = state
                    .nodes
                    .iter_mut()
                    .find(|n| n.node_id == new_node.node_id)
                {
                    // Update metadata but keep IP if we already have it
                    existing.name = new_node.name;
                    existing.status = new_node.status;
                    if existing.node_ip.is_empty() {
                        existing.node_ip = new_node.node_ip;
                    }
                } else {
                    state.nodes.push(new_node);
                }
            }
            if !state.nodes.is_empty() {
                state.selected_idx = state.selected_idx.min(state.nodes.len() - 1);
                state.node = state.nodes[state.selected_idx].name.clone();
            }
        }
        ProgressEvent::DownloadUpdate {
            ratio,
            speed,
            raw_speed,
        } => {
            state.live_stats.dl_ratio = ratio.clamp(0.0, 1.0);
            state.live_stats.dl_speed = speed.max(0.0);
            state.live_stats.dl_raw_speed = raw_speed.max(0.0);

            if state.live_stats.dl_trend_start_ratio.is_none()
                && trend_renderer
                    .should_start_capture(state.live_stats.dl_ratio, state.live_stats.dl_speed)
            {
                state.live_stats.dl_trend_start_ratio = Some(state.live_stats.dl_ratio);
            }

            // Push all samples, the frontend will resample them based on ratio
            if state.live_stats.dl_trend_start_ratio.is_some() {
                push_history(&mut state.dl_hist, state.live_stats.dl_speed, 2000);
            }

            if state.live_stats.dl_ratio >= 1.0 {
                state.live_stats.dl_final = Some(state.live_stats.dl_speed);
                state.live_stats.dl_raw_final = Some(state.live_stats.dl_raw_speed);
            }
        }
        ProgressEvent::UploadUpdate {
            ratio,
            speed,
            raw_speed,
        } => {
            state.live_stats.ul_ratio = ratio.clamp(0.0, 1.0);
            state.live_stats.ul_speed = speed.max(0.0);
            state.live_stats.ul_raw_speed = raw_speed.max(0.0);

            if state.live_stats.ul_trend_start_ratio.is_none()
                && trend_renderer
                    .should_start_capture(state.live_stats.ul_ratio, state.live_stats.ul_speed)
            {
                state.live_stats.ul_trend_start_ratio = Some(state.live_stats.ul_ratio);
            }

            if state.live_stats.ul_trend_start_ratio.is_some() {
                push_history(&mut state.ul_hist, state.live_stats.ul_speed, 2000);
            }

            if state.live_stats.ul_ratio >= 1.0 {
                state.live_stats.ul_final = Some(state.live_stats.ul_speed);
                state.live_stats.ul_raw_final = Some(state.live_stats.ul_raw_speed);
            }
        }
        ProgressEvent::LatencyUpdate {
            ping,
            jitter,
            failed_count,
            total_count,
        } => {
            state.live_stats.ping = ping.max(0.0);
            state.live_stats.jitter = jitter.max(0.0);
            state.live_stats.packet_failed = failed_count;
            state.live_stats.packet_total = total_count;
        }
        ProgressEvent::NodeIpFound { node_id, node_ip } => {
            if let Some(node) = state.nodes.iter_mut().find(|n| n.node_id == node_id) {
                node.node_ip = node_ip.clone();
            }
            if let Some(active) = state.nodes.get(state.selected_idx) {
                if active.node_id == node_id {
                    state.node = active.name.clone();
                }
            }
        }
        ProgressEvent::TestFinished(res) => {
            state.live_stats.dl_final = Some(res.dl_avg);
            state.live_stats.dl_raw_final = Some(res.dl_raw_avg);
            state.live_stats.ul_final = Some(res.ul_avg);
            state.live_stats.ul_raw_final = Some(res.ul_raw_avg);
            state.results = Some(res);
            state.running = false;
            state.status = "Done".into();
            push_timeline(&mut state.timeline, "Test complete".into());
            state.started_at = None;
        }
        ProgressEvent::TestAborted { reason } => {
            state.running = false;
            state.started_at = None;
            state.status = format!("Aborted: {}", reason);
            push_timeline(&mut state.timeline, format!("Test aborted: {}", reason));
        }
    }
}

pub fn select_prev_node(state: &mut AppState) {
    if state.selected_idx > 0 {
        state.selected_idx -= 1;
        if let Some(node) = state.nodes.get(state.selected_idx) {
            state.node = node.name.clone();
        }
    }
}

pub fn select_next_node(state: &mut AppState) {
    if !state.nodes.is_empty() && state.selected_idx + 1 < state.nodes.len() {
        state.selected_idx += 1;
        state.node = state.nodes[state.selected_idx].name.clone();
    }
}

pub fn start_test(state: &mut AppState) -> Option<Option<String>> {
    if state.running {
        return None;
    }
    state.running = true;
    state.results = None;
    state.live_stats.dl_final = None;
    state.live_stats.ul_final = None;
    state.live_stats.dl_speed = 0.0;
    state.live_stats.ul_speed = 0.0;
    state.live_stats.dl_ratio = 0.0;
    state.live_stats.ul_ratio = 0.0;
    state.live_stats.dl_trend_start_ratio = None;
    state.live_stats.ul_trend_start_ratio = None;
    state.live_stats.packet_total = 0;
    state.live_stats.packet_failed = 0;
    state.dl_hist.clear();
    state.ul_hist.clear();
    state.started_at = Some(Instant::now());

    let selected_node_id = state
        .nodes
        .get(state.selected_idx)
        .map(|n| n.node_id.clone());
    Some(selected_node_id)
}

pub fn stop_test(state: &mut AppState) {
    state.running = false;
    state.started_at = None;
    state.status = "Stopped".into();
    push_timeline(&mut state.timeline, "Stopped by user".into());
}

pub fn copy_results_to_clipboard(state: &mut AppState) {
    let mut text = String::new();
    text.push_str("--- CNM SPEED TEST FULL REPORT ---\n");
    text.push_str(&format!(
        "Server  : {} ({})\n",
        state.province_label, state.base_url
    ));
    text.push_str(&format!(
        "User    : {}, IP: {}, City: {}\n",
        state.user_context.name, state.user_context.ip, state.user_context.city
    ));
    text.push_str(&format!(
        "Contract: {}, Node: {}\n",
        state.user_context.bandwidth, state.node
    ));
    text.push_str(&format!(
        "Latency : {:.2}ms, Jitter: {:.2}ms\n",
        state.live_stats.ping, state.live_stats.jitter
    ));

    text.push_str("\n[Settings]\n");
    text.push_str(&format!("Concurrency : {}\n", state.settings.concurrency));
    text.push_str(&format!("Duration    : {}s\n", state.settings.duration_sec));
    text.push_str(&format!(
        "Smoothing   : {}s\n",
        state.settings.smoothing_window_sec
    ));
    text.push_str(&format!("Priority    : {:?}\n", state.settings.priority));

    text.push_str("\n[Results]\n");
    if let Some(r) = &state.results {
        text.push_str(&format!(
            "Download: {:.2} Mbps (Avg), {:.2} Mbps (Max)\n",
            r.dl_avg, r.dl_max
        ));
        text.push_str(&format!(
            "Upload  : {:.2} Mbps (Avg), {:.2} Mbps (Max)\n",
            r.ul_avg, r.ul_max
        ));
        text.push_str(&format!(
            "Data    : DL {:.1}MB, UL {:.1}MB\n",
            r.dl_bytes as f64 / 1024.0 / 1024.0,
            r.ul_bytes as f64 / 1024.0 / 1024.0
        ));
    } else {
        text.push_str("Status: No complete results available yet.\n");
    }
    text.push_str("----------------------------------\n");

    copy_to_system_clipboard(state, text, "✓ Full report copied!");
}

pub fn copy_summary_to_clipboard(state: &mut AppState) {
    let mut text = String::new();
    text.push_str("--- SUMMARY LOGS ---\n");

    if let Some(r) = &state.results {
        text.push_str(&format!(
            "LAST RESULT: DL {:.2} / UL {:.2} Mbps / Ping {:.2}ms\n",
            r.dl_avg, r.ul_avg, state.live_stats.ping
        ));
        text.push_str(&format!(
            "Data Used: DL {} / UL {}\n",
            format_bytes(r.dl_bytes),
            format_bytes(r.ul_bytes)
        ));
        text.push_str("\n");
    }

    for msg in &state.timeline {
        text.push_str(&format!("• {}\n", msg));
    }
    text.push_str("--------------------\n");

    copy_to_system_clipboard(state, text, "✓ All logs copied!");
}

fn copy_to_system_clipboard(state: &mut AppState, text: String, success_msg: &str) {
    copy_to_system_clipboard_impl(state, text, success_msg);
}

#[cfg(not(target_os = "android"))]
fn copy_to_system_clipboard_impl(state: &mut AppState, text: String, success_msg: &str) {
    let mut clipboard = match arboard::Clipboard::new() {
        Ok(c) => c,
        Err(_) => {
            push_timeline(
                &mut state.timeline,
                "Error: Could not access clipboard".into(),
            );
            return;
        }
    };

    match clipboard.set_text(text) {
        Ok(_) => push_timeline(&mut state.timeline, success_msg.into()),
        Err(_) => push_timeline(&mut state.timeline, "✗ Failed to copy to clipboard".into()),
    }
}

#[cfg(target_os = "android")]
fn copy_to_system_clipboard_impl(state: &mut AppState, _text: String, _success_msg: &str) {
    push_timeline(
        &mut state.timeline,
        "Clipboard is unavailable on Android target".into(),
    );
}

pub fn push_timeline(lines: &mut VecDeque<String>, s: String) {
    if lines.len() >= 100 {
        lines.pop_front();
    }
    lines.push_back(s);
}

fn push_history(hist: &mut VecDeque<f64>, v: f64, max_len: usize) {
    if hist.len() >= max_len {
        hist.pop_front();
    }
    hist.push_back(v);
}
