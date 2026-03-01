use super::types::{AppState, SettingsField};
use crate::speedtest::types::TestPriority;
use crate::utils::format::format_bytes;
use ratatui::prelude::Alignment;
use ratatui::{
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Clear, Paragraph, Row, Table},
    Frame,
};
use std::collections::VecDeque;

#[derive(Clone, Copy)]
struct Theme {
    accent: Color,
    highlight: Color,
    text: Color,
    dim: Color,
    border: Color,
    success: Color,
    error: Color,
    bg_card: Color,
}

impl Theme {
    fn default() -> Self {
        Self {
            accent: Color::Rgb(10, 132, 255),
            highlight: Color::Rgb(255, 255, 255),
            text: Color::Rgb(235, 235, 245),
            dim: Color::Rgb(142, 142, 147),
            border: Color::Rgb(60, 60, 67),
            success: Color::Rgb(48, 209, 88),
            error: Color::Rgb(255, 69, 58),
            bg_card: Color::Rgb(30, 30, 32),
        }
    }
}

pub fn draw(f: &mut Frame, state: &std::sync::Arc<std::sync::Mutex<AppState>>) {
    let mut s = state.lock().unwrap();
    let t = Theme::default();

    let root = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(f.area());

    render_header(f, &s, t, root[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(root[1]);

    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(9), // Info
            Constraint::Length(8), // Performance (Increased to accommodate dual bars)
            Constraint::Length(3), // Actions
            Constraint::Min(0),    // Summary
        ])
        .split(body[0]);

    render_info(f, &s, t, left[0]);
    render_performance(f, &s, t, left[1]);
    render_actions(f, &mut s, t, left[2]);
    render_summary(f, &s, t, left[3]);
    render_nodes_table(f, &mut s, t, body[1]);

    f.render_widget(
        Paragraph::new(Span::styled(
            " ^C Full Copy  ·  ^S Summary  ·  ESC Settings  ·  S Start  ·  Q Quit",
            Style::default().fg(t.dim),
        )),
        root[2],
    );

    if s.settings_open {
        draw_settings_modal(f, &mut s, t);
    }
}

fn render_header(f: &mut Frame, s: &AppState, t: Theme, area: Rect) {
    let base_url_clean = s.base_url.trim_start_matches("http://");
    let header_content = vec![
        Span::styled(
            " CNM SPEED ",
            Style::default()
                .fg(t.highlight)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  {}  ", s.province_label),
            Style::default().fg(t.accent),
        ),
        Span::styled(base_url_clean, Style::default().fg(t.dim)),
    ];
    f.render_widget(Paragraph::new(Line::from(header_content)), area);
}

fn render_info(f: &mut Frame, s: &AppState, t: Theme, area: Rect) {
    let ip_display = if s.user_context.ip.is_empty() || s.user_context.ip == "-" {
        "Detecting..."
    } else {
        &s.user_context.ip
    };
    let info = vec![
        line_kv("Status", &s.status, t.accent),
        line_kv("Account", &s.user_context.name, t.text),
        line_kv("Public IP", ip_display, t.text),
        line_kv(
            "Latency",
            &format!("{:.2} ms", s.live_stats.ping),
            t.success,
        ),
        line_kv(
            "Jitter",
            &format!("{:.2} ms", s.live_stats.jitter),
            t.success,
        ),
    ];
    f.render_widget(
        Paragraph::new(info).block(apple_block(" INFORMATION ", t)),
        area,
    );
}

fn render_performance(f: &mut Frame, s: &AppState, t: Theme, area: Rect) {
    let (mode, speed, raw_speed, hist, ratio) = if s.running {
        if s.live_stats.dl_ratio > 0.0 && s.live_stats.dl_ratio < 1.0 {
            (
                "Downloading",
                s.live_stats.dl_speed,
                s.live_stats.dl_raw_speed,
                &s.dl_hist,
                s.live_stats.dl_ratio,
            )
        } else {
            (
                "Uploading",
                s.live_stats.ul_speed,
                s.live_stats.ul_raw_speed,
                &s.ul_hist,
                s.live_stats.ul_ratio,
            )
        }
    } else if s.results.is_some() {
        if let Some(ul) = s.live_stats.ul_final {
            (
                "Uploading",
                ul,
                s.live_stats.ul_raw_final.unwrap_or(ul),
                &s.ul_hist,
                1.0,
            )
        } else if let Some(dl) = s.live_stats.dl_final {
            (
                "Downloading",
                dl,
                s.live_stats.dl_raw_final.unwrap_or(dl),
                &s.dl_hist,
                1.0,
            )
        } else {
            ("Finished", 0.0, 0.0, &s.dl_hist, 1.0)
        }
    } else {
        ("Idle State", 0.0, 0.0, &s.dl_hist, 0.0)
    };

    let block = apple_block(" PERFORMANCE ", t);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let is_cheating = s.settings.allow_official_cheat_calculation;

    let mut constraints = vec![
        Constraint::Length(1), // Title/Speed
        Constraint::Length(1), // Meter 1
        Constraint::Length(1), // Meter 2 (if cheating)
        Constraint::Length(1), // Trend
    ];
    if !is_cheating {
        constraints.remove(2);
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("{:<12}", mode),
                Style::default()
                    .fg(t.highlight)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" {:.2} Mbps", speed),
                Style::default()
                    .fg(if is_cheating { t.accent } else { t.success })
                    .add_modifier(Modifier::BOLD),
            ),
            if is_cheating {
                Span::styled(
                    format!(" (Truth: {:.2})", raw_speed),
                    Style::default().fg(t.success),
                )
            } else {
                Span::raw("")
            },
        ])),
        rows[0],
    );

    let chart_width = inner.width.saturating_sub(10) as usize;

    // Official / Normal Meter
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                if is_cheating { "Official" } else { "Speed   " },
                Style::default().fg(t.dim),
            ),
            Span::styled(
                speed_meter(speed, chart_width),
                Style::default().fg(if is_cheating { t.accent } else { t.success }),
            ),
        ])),
        rows[1],
    );

    if is_cheating {
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("Truth   ", Style::default().fg(t.success)),
                Span::styled(
                    speed_meter(raw_speed, chart_width),
                    Style::default().fg(t.success),
                ),
            ])),
            rows[2],
        );

        let trend_idx = 3;
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("Trend   ", Style::default().fg(t.dim)),
                Span::styled(
                    mini_chart_rtl(hist, chart_width, ratio as f64),
                    Style::default().fg(t.accent),
                ),
            ])),
            rows[trend_idx],
        );
    } else {
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("Trend   ", Style::default().fg(t.dim)),
                Span::styled(
                    mini_chart_rtl(hist, chart_width, ratio as f64),
                    Style::default().fg(t.accent),
                ),
            ])),
            rows[2],
        );
    }
}

fn render_actions(f: &mut Frame, s: &mut AppState, t: Theme, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    s.hits.start_btn = cols[0];
    s.hits.quit_btn = cols[1];

    render_apple_button(
        f,
        if s.running { "STOP" } else { "START" },
        if s.running { t.error } else { t.success },
        cols[0],
    );
    render_apple_button(f, "QUIT", t.dim, cols[1]);
}

fn render_summary(f: &mut Frame, s: &AppState, t: Theme, area: Rect) {
    let block = apple_block(" SUMMARY ", t);
    let inner = block.inner(area);
    let available_height = inner.height as usize;

    let mut lines = Vec::new();
    let mut used = 0;

    if let Some(r) = &s.results {
        let is_cheating = s.settings.allow_official_cheat_calculation;
        lines.push(Line::from(vec![
            Span::styled(
                " LAST RESULT ",
                Style::default()
                    .fg(t.bg_card)
                    .bg(if is_cheating { t.accent } else { t.success })
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                if is_cheating {
                    format!(" DL {:.1} / UL {:.1} Mbps (Official)", r.dl_avg, r.ul_avg)
                } else {
                    format!(" DL {:.1} / UL {:.1} Mbps", r.dl_avg, r.ul_avg)
                },
                Style::default().fg(t.highlight),
            ),
        ]));

        if is_cheating {
            lines.push(Line::from(vec![
                Span::styled(
                    "   TRUTH     ",
                    Style::default().fg(t.success).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" DL {:.1} / UL {:.1} Mbps", r.dl_raw_avg, r.ul_raw_avg),
                    Style::default().fg(t.success),
                ),
            ]));
            used += 1;
        }

        lines.push(Line::from(""));
        lines.push(line_kv(
            "Data Used",
            &format!(
                "DL {} / UL {}",
                format_bytes(r.dl_bytes),
                format_bytes(r.ul_bytes)
            ),
            t.dim,
        ));
        used += 3;
    }

    let remaining = available_height.saturating_sub(used);
    for msg in s.timeline.iter().rev().take(remaining).rev() {
        lines.push(Line::from(vec![
            Span::styled(" • ", Style::default().fg(t.border)),
            Span::styled(msg, Style::default().fg(t.dim)),
        ]));
    }
    f.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_nodes_table(f: &mut Frame, s: &mut AppState, t: Theme, area: Rect) {
    let block = apple_block(" SERVERS ", t);
    s.hits.nodes_rect = area;
    let rows: Vec<Row> = s
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let is_sel = i == s.selected_idx;
            let style = if is_sel {
                Style::default().fg(t.accent).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(t.text)
            };
            let ip_text = if n.node_ip.is_empty() {
                if is_sel {
                    "Checking..."
                } else {
                    "-"
                }
            } else {
                &n.node_ip
            };
            Row::new(vec![
                Cell::from(if is_sel { "●" } else { " " }),
                Cell::from(n.name.clone()),
                Cell::from(ip_text.to_string()),
                Cell::from(if n.status == 1 { "Online" } else { "Offline" }),
            ])
            .style(style)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(2),
            Constraint::Min(20),
            Constraint::Length(16),
            Constraint::Length(10),
        ],
    )
    .header(
        Row::new(vec!["", "Node Name", "IP Address", "Status"]).style(Style::default().fg(t.dim)),
    )
    .block(block);
    f.render_widget(table, area);
}

fn draw_settings_modal(f: &mut Frame, s: &mut AppState, t: Theme) {
    let area = centered_rect(48, 48, f.area());
    f.render_widget(Clear, area);
    let block = Block::default()
        .title(Span::styled(
            " Settings ",
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(t.accent))
        .style(Style::default().bg(t.bg_card));
    let inner = block.inner(area).inner(Margin {
        horizontal: 2,
        vertical: 1,
    });
    f.render_widget(block, area);
    let prio = if s.settings.priority == TestPriority::DownloadFirst {
        "Download First"
    } else {
        "Upload First"
    };
    let allow_cheat = if s.settings.allow_official_cheat_calculation {
        "ON"
    } else {
        "OFF"
    };
    let mut rows = Vec::new();
    let fields = [
        ("Concurrency", SettingsField::Concurrency),
        ("Duration", SettingsField::Duration),
        ("Smoothing", SettingsField::Smoothing),
        ("Speed Refresh", SettingsField::SpeedRefresh),
        ("Ping Refresh", SettingsField::PingRefresh),
        ("Priority Mode", SettingsField::Priority),
        (
            "Official Cheat Calculation",
            SettingsField::AllowOfficialCheatCalculation,
        ),
        ("Reload from Disk", SettingsField::Reload),
    ];

    for (label, field) in fields {
        let selected = s.settings_focus == field;
        let label_style = if selected {
            Style::default().fg(t.accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(t.dim)
        };
        let val_bg = if selected {
            Color::Rgb(60, 60, 80)
        } else {
            Color::Reset
        };
        let val_fg = if selected { t.highlight } else { t.text };
        let display_val = setting_display_value(s, field, selected, prio, allow_cheat);
        rows.push(Line::from(vec![
            Span::styled(format!(" {:<20} ", label), label_style),
            Span::styled(
                format!(" {} ", display_val),
                Style::default().fg(val_fg).bg(val_bg),
            ),
        ]));
    }
    f.render_widget(Paragraph::new(rows), inner);
    if s.settings_open {
        let focus_idx = s.settings_focus as u16;
        if focus_idx < 5 {
            f.set_cursor_position((
                inner.x + 23 + s.settings_input.visual_cursor() as u16,
                inner.y + focus_idx,
            ));
        }
    }
}

fn apple_block(title: &'static str, t: Theme) -> Block<'static> {
    Block::default()
        .title(Span::styled(
            title,
            Style::default().fg(t.dim).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::TOP)
        .border_style(Style::default().fg(t.border))
}

fn render_apple_button(f: &mut Frame, text: &str, color: Color, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(color));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(1),
            Constraint::Fill(1),
        ])
        .split(inner);

    f.render_widget(
        Paragraph::new(Span::styled(
            text,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        layout[1],
    );
}

fn line_kv(key: &str, value: &str, val_color: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!(" {:<12} ", key),
            Style::default().fg(Color::Rgb(140, 140, 150)),
        ),
        Span::styled(value.to_string(), Style::default().fg(val_color)),
    ])
}

fn speed_meter(speed: f64, width: usize) -> String {
    let blocks = (speed / 25.0).clamp(0.0, width as f64) as usize;
    format!("{}{}", "█".repeat(blocks), "░".repeat(width - blocks))
}

fn mini_chart_rtl(hist: &VecDeque<f64>, width: usize, ratio: f64) -> String {
    if hist.is_empty() {
        return "░".repeat(width);
    }
    let chars = [" ", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
    let max_v = hist.iter().cloned().fold(0.1, f64::max);
    let min_v = hist.iter().cloned().fold(max_v, f64::min);
    let range = (max_v - min_v).max(0.1);

    let active_slots = (width as f64 * ratio.clamp(0.0, 1.0)).floor() as usize;
    let active_slots = active_slots.max(1).min(width);
    let empty_slots = width.saturating_sub(active_slots);

    let mut data = Vec::with_capacity(active_slots);
    if hist.len() >= active_slots {
        for &v in hist.iter().rev().take(active_slots).rev() {
            let norm = ((v - min_v) / range).powf(0.5);
            data.push(chars[(norm * 7.0).round() as usize]);
        }
    } else {
        for i in 0..active_slots {
            let hist_idx = (i * hist.len()) / active_slots;
            let v = hist[hist_idx.min(hist.len() - 1)];
            let norm = ((v - min_v) / range).powf(0.5);
            data.push(chars[(norm * 7.0).round() as usize]);
        }
    }

    let mut res = "░".repeat(empty_slots);
    res.push_str(&data.iter().cloned().collect::<String>());
    res.chars().take(width).collect()
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn setting_display_value(
    s: &AppState,
    field: SettingsField,
    selected: bool,
    prio: &str,
    allow_cheat: &str,
) -> String {
    match field {
        SettingsField::Concurrency => {
            if selected {
                s.settings_input.value().to_string()
            } else {
                s.settings.concurrency.to_string()
            }
        }
        SettingsField::Duration => {
            let raw = if selected {
                s.settings_input.value().to_string()
            } else {
                s.settings.duration_sec.to_string()
            };
            format!("{}s", raw)
        }
        SettingsField::Smoothing => {
            let raw = if selected {
                s.settings_input.value().to_string()
            } else {
                format!("{}", s.settings.smoothing_window_sec)
            };
            format!("{}s", raw)
        }
        SettingsField::SpeedRefresh => {
            let raw = if selected {
                s.settings_input.value().to_string()
            } else {
                s.settings.speed_refresh_ms.to_string()
            };
            format!("{}ms", raw)
        }
        SettingsField::PingRefresh => {
            let raw = if selected {
                s.settings_input.value().to_string()
            } else {
                s.settings.ping_refresh_ms.to_string()
            };
            format!("{}ms", raw)
        }
        SettingsField::Priority => prio.to_string(),
        SettingsField::AllowOfficialCheatCalculation => allow_cheat.to_string(),
        SettingsField::Reload => "Press Enter".into(),
    }
}
