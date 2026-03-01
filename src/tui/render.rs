use super::components::{apple_block, centered_rect, line_kv, render_apple_button, Theme};
use super::types::{AppState, SettingsField};
use crate::speedtest::types::TestPriority;
use crate::utils::format::format_bytes;
use crate::utils::trend::TrendRenderer;
use ratatui::{
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row,
        Scrollbar, ScrollbarOrientation, ScrollbarState, Table,
    },
    Frame,
};
use throbber_widgets_tui::Throbber;

pub fn draw(f: &mut Frame, s: &mut AppState) {
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

    render_header(f, s, t, root[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(root[1]);

    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(12), // Info
            Constraint::Length(8),  // Performance
            Constraint::Length(3),  // Actions
            Constraint::Min(0),     // Summary
        ])
        .split(body[0]);

    render_info(f, s, t, left[0]);
    render_performance(f, s, t, left[1]);
    render_actions(f, s, t, left[2]);
    render_summary(f, s, t, left[3]);
    render_nodes_table(f, s, t, body[1]);

    f.render_widget(
        Paragraph::new(Span::styled(
            " ^C Full Copy  ·  ^S Summary  ·  ESC Settings  ·  S Start  ·  Q Quit",
            Style::default().fg(t.dim),
        )),
        root[2],
    );

    if s.settings_open {
        draw_settings_modal(f, s, t);
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

    let latency_label = if s.running {
        if s.live_stats.dl_ratio > 0.0 && s.live_stats.dl_ratio < 1.0 {
            "Latency(DL)"
        } else if s.live_stats.ul_ratio > 0.0 && s.live_stats.ul_ratio < 1.0 {
            "Latency(UL)"
        } else {
            "Latency"
        }
    } else {
        "Latency"
    };

    let packet_rate = if s.live_stats.packet_total > 0 {
        (s.live_stats.packet_failed as f64 / s.live_stats.packet_total as f64) * 100.0
    } else {
        0.0
    };

    let info = vec![
        line_kv("Status", &s.status, t.accent),
        line_kv("Account", &s.user_context.name, t.text),
        line_kv("Public IP", ip_display, t.text),
        line_kv(
            latency_label,
            &format!("{:.2} ms", s.live_stats.ping),
            t.success,
        ),
        line_kv(
            "Jitter",
            &format!("{:.2} ms", s.live_stats.jitter),
            t.success,
        ),
        line_kv(
            "Packet Count",
            &s.live_stats.packet_total.to_string(),
            t.text,
        ),
        line_kv(
            "Packet Failed",
            &s.live_stats.packet_failed.to_string(),
            if s.live_stats.packet_failed > 0 {
                t.error
            } else {
                t.success
            },
        ),
        line_kv("Packet Rate", &format!("{:.1}%", packet_rate), t.text),
    ];
    f.render_widget(
        Paragraph::new(info).block(apple_block(" INFORMATION ", t)),
        area,
    );
}

fn render_performance(f: &mut Frame, s: &mut AppState, t: Theme, area: Rect) {
    let trend_renderer = TrendRenderer::default();
    let (mode, speed, raw_speed, hist, ratio) = if s.running {
        let dl_started = s.live_stats.dl_ratio > 0.0;
        let ul_started = s.live_stats.ul_ratio > 0.0;
        let dl_done = s.live_stats.dl_ratio >= 1.0;
        let ul_done = s.live_stats.ul_ratio >= 1.0;

        if dl_started && !dl_done {
            (
                "Downloading",
                s.live_stats.dl_speed,
                s.live_stats.dl_raw_speed,
                &s.dl_hist,
                s.live_stats.dl_ratio,
            )
        } else if ul_started && !ul_done {
            (
                "Uploading",
                s.live_stats.ul_speed,
                s.live_stats.ul_raw_speed,
                &s.ul_hist,
                s.live_stats.ul_ratio,
            )
        } else if ul_done {
            (
                "Uploading",
                s.live_stats.ul_final.unwrap_or(s.live_stats.ul_speed),
                s.live_stats
                    .ul_raw_final
                    .unwrap_or(s.live_stats.ul_raw_speed),
                &s.ul_hist,
                1.0,
            )
        } else if dl_done {
            (
                "Downloading",
                s.live_stats.dl_final.unwrap_or(s.live_stats.dl_speed),
                s.live_stats
                    .dl_raw_final
                    .unwrap_or(s.live_stats.dl_raw_speed),
                &s.dl_hist,
                1.0,
            )
        } else if s.live_stats.ping > 0.0 {
            ("Testing Ping", 0.0, 0.0, &s.dl_hist, 0.0)
        } else {
            ("Idle State", 0.0, 0.0, &s.dl_hist, 0.0)
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
        Constraint::Length(2), // Title/Speed (give it 2 lines to separate it from the bars)
        Constraint::Length(1), // Meter 1
    ];

    if is_cheating {
        constraints.push(Constraint::Length(1)); // Meter 2
    }

    let base_rows = if is_cheating { 4 } else { 3 };
    let trend_rows = inner.height.saturating_sub(base_rows).clamp(1, 3);
    constraints.push(Constraint::Length(trend_rows)); // Trend auto 1..3 rows
    constraints.push(Constraint::Min(0)); // Filler space at the bottom to push everything up neatly

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    if rows.is_empty() || rows[0].height == 0 {
        return; // Prevent rendering panics if terminal is too small
    }

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

    if s.running && speed <= 0.0 && ratio == 0.0 && s.live_stats.ping > 0.0 {
        let throbber = Throbber::default()
            .style(Style::default().fg(t.accent))
            .throbber_set(throbber_widgets_tui::BRAILLE_SIX)
            .use_type(throbber_widgets_tui::WhichUse::Spin);

        // Make sure we have space for the throbber to prevent bounds panic
        if rows[0].width > 4 {
            f.render_stateful_widget(
                throbber,
                rows[0].inner(Margin {
                    horizontal: 2,
                    vertical: 0,
                }),
                &mut s.throbber_state,
            );
        }
    }

    let max_speed = 2000.0; // 2Gbps as max visual scale
    let ratio_official = (speed / max_speed).clamp(0.0, 1.0);
    let ratio_truth = (raw_speed / max_speed).clamp(0.0, 1.0);

    // Official / Normal Meter
    let meter_color = if is_cheating { t.accent } else { t.success };
    let bar_width = rows[1].width.saturating_sub(8) as usize; // Dynamically match available width

    // We construct a custom text meter that grows from left to right for current speed
    let blocks = (ratio_official * bar_width as f64).round() as usize;
    let blocks = blocks.clamp(0, bar_width);
    let gauge_str = format!("{}{}", "█".repeat(blocks), "░".repeat(bar_width - blocks));

    let official_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(8), Constraint::Min(0)])
        .split(rows[1]);

    f.render_widget(
        Paragraph::new(Span::styled(
            if is_cheating { "Official" } else { "Speed   " },
            Style::default().fg(t.dim),
        )),
        official_layout[0],
    );
    f.render_widget(
        Paragraph::new(Span::styled(gauge_str, Style::default().fg(meter_color))),
        official_layout[1],
    );

    let trend_start_ratio = if mode == "Downloading" {
        s.live_stats.dl_trend_start_ratio
    } else if mode == "Uploading" {
        s.live_stats.ul_trend_start_ratio
    } else {
        None
    };

    if is_cheating {
        let truth_blocks = (ratio_truth * bar_width as f64).round() as usize;
        let truth_blocks = truth_blocks.clamp(0, bar_width);
        let truth_gauge_str = format!(
            "{}{}",
            "█".repeat(truth_blocks),
            "░".repeat(bar_width - truth_blocks)
        );

        let truth_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(8), Constraint::Min(0)])
            .split(rows[2]);

        f.render_widget(
            Paragraph::new(Span::styled("Truth   ", Style::default().fg(t.dim))),
            truth_layout[0],
        );
        f.render_widget(
            Paragraph::new(Span::styled(
                truth_gauge_str,
                Style::default().fg(t.success),
            )),
            truth_layout[1],
        );

        let trend_idx = 3;

        let trend_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(8), Constraint::Min(0)])
            .split(rows[trend_idx]);
        let trend_width = trend_layout[1].width as usize;
        let trend_lines = trend_renderer.render_rtl_lines(
            hist,
            trend_width,
            ratio,
            trend_start_ratio,
            trend_layout[1].height as usize,
        );

        f.render_widget(
            Paragraph::new(Span::styled("Trend   ", Style::default().fg(t.dim))),
            trend_layout[0],
        );
        f.render_widget(
            Paragraph::new(
                trend_lines
                    .into_iter()
                    .map(Line::from)
                    .collect::<Vec<Line>>(),
            )
            .style(Style::default().fg(t.accent)),
            trend_layout[1],
        );
    } else {
        let trend_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(8), Constraint::Min(0)])
            .split(rows[2]);
        let trend_width = trend_layout[1].width as usize;
        let trend_lines = trend_renderer.render_rtl_lines(
            hist,
            trend_width,
            ratio,
            trend_start_ratio,
            trend_layout[1].height as usize,
        );

        f.render_widget(
            Paragraph::new(Span::styled("Trend   ", Style::default().fg(t.dim))),
            trend_layout[0],
        );
        f.render_widget(
            Paragraph::new(
                trend_lines
                    .into_iter()
                    .map(Line::from)
                    .collect::<Vec<Line>>(),
            )
            .style(Style::default().fg(t.accent)),
            trend_layout[1],
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

fn render_summary(f: &mut Frame, s: &mut AppState, t: Theme, area: Rect) {
    let block = apple_block(" SUMMARY ", t);
    let inner = block.inner(area);

    let mut items = Vec::new();

    if let Some(r) = &s.results {
        let is_cheating = s.settings.allow_official_cheat_calculation;
        items.push(ListItem::new(Line::from(vec![
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
        ])));

        if is_cheating {
            items.push(ListItem::new(Line::from(vec![
                Span::styled(
                    "   TRUTH     ",
                    Style::default().fg(t.success).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" DL {:.1} / UL {:.1} Mbps", r.dl_raw_avg, r.ul_raw_avg),
                    Style::default().fg(t.success),
                ),
            ])));
        }

        items.push(ListItem::new(Line::from("")));
        items.push(ListItem::new(Line::from(vec![
            Span::styled(
                " Latency ",
                Style::default()
                    .fg(t.bg_card)
                    .bg(t.dim)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    " Idle: {:.1}ms · DL: {:.1}ms · UL: {:.1}ms",
                    r.ping_idle, r.ping_dl, r.ping_ul
                ),
                Style::default().fg(t.success),
            ),
        ])));

        // format ping loss rates safely
        let idle_loss = if r.ping_idle_total > 0 {
            (r.ping_idle_failed as f64 / r.ping_idle_total as f64) * 100.0
        } else {
            0.0
        };
        let dl_loss = if r.ping_dl_total > 0 {
            (r.ping_dl_failed as f64 / r.ping_dl_total as f64) * 100.0
        } else {
            0.0
        };
        let ul_loss = if r.ping_ul_total > 0 {
            (r.ping_ul_failed as f64 / r.ping_ul_total as f64) * 100.0
        } else {
            0.0
        };

        items.push(ListItem::new(Line::from(vec![
            Span::styled(
                " Pkg Loss",
                Style::default()
                    .fg(t.bg_card)
                    .bg(t.dim)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    " Idle: {:.1}% · DL: {:.1}% · UL: {:.1}%",
                    idle_loss, dl_loss, ul_loss
                ),
                Style::default().fg(if idle_loss > 0.0 || dl_loss > 0.0 || ul_loss > 0.0 {
                    t.error
                } else {
                    t.success
                }),
            ),
        ])));

        items.push(ListItem::new(Line::from(vec![
            Span::styled(
                " Packet Cnt",
                Style::default()
                    .fg(t.bg_card)
                    .bg(t.dim)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    " Idle: {}/{} · DL: {}/{} · UL: {}/{}",
                    r.ping_idle_total.saturating_sub(r.ping_idle_failed),
                    r.ping_idle_total,
                    r.ping_dl_total.saturating_sub(r.ping_dl_failed),
                    r.ping_dl_total,
                    r.ping_ul_total.saturating_sub(r.ping_ul_failed),
                    r.ping_ul_total
                ),
                Style::default().fg(t.text),
            ),
        ])));

        items.push(ListItem::new(Line::from(vec![
            Span::styled(
                " Packet Fail",
                Style::default()
                    .fg(t.bg_card)
                    .bg(t.dim)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    " Idle: {} · DL: {} · UL: {}",
                    r.ping_idle_failed, r.ping_dl_failed, r.ping_ul_failed
                ),
                Style::default().fg(
                    if r.ping_idle_failed + r.ping_dl_failed + r.ping_ul_failed > 0 {
                        t.error
                    } else {
                        t.success
                    },
                ),
            ),
        ])));

        items.push(ListItem::new(Line::from(vec![
            Span::styled(
                " Packet Rate",
                Style::default()
                    .fg(t.bg_card)
                    .bg(t.dim)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    " Idle: {:.1}% · DL: {:.1}% · UL: {:.1}%",
                    idle_loss, dl_loss, ul_loss
                ),
                Style::default().fg(if idle_loss > 0.0 || dl_loss > 0.0 || ul_loss > 0.0 {
                    t.error
                } else {
                    t.success
                }),
            ),
        ])));

        items.push(ListItem::new(line_kv(
            "Data Used",
            &format!(
                "DL {} / UL {}",
                format_bytes(r.dl_bytes),
                format_bytes(r.ul_bytes)
            ),
            t.dim,
        )));
        items.push(ListItem::new(Line::from("")));
    }

    for msg in &s.timeline {
        items.push(ListItem::new(Line::from(vec![
            Span::styled(" • ", Style::default().fg(t.border)),
            Span::styled(msg, Style::default().fg(t.dim)),
        ])));
    }

    let line_count = items.len();
    let displayable_height = inner.height as usize;
    let max_offset = line_count.saturating_sub(displayable_height);

    if s.log_auto_scroll {
        s.log_scroll_offset = max_offset;
    } else {
        s.log_scroll_offset = s.log_scroll_offset.min(max_offset);
        if s.log_scroll_offset >= max_offset {
            s.log_auto_scroll = true;
        }
    }

    let mut list_state = ListState::default().with_offset(s.log_scroll_offset);

    let list = List::new(items).block(block);
    f.render_stateful_widget(list, area, &mut list_state);

    if line_count > displayable_height {
        let mut scrollbar_state = ScrollbarState::new(max_offset).position(s.log_scroll_offset);
        let scrollbar = Scrollbar::default()
            .orientation(ScrollbarOrientation::VerticalRight)
            .style(Style::default().fg(t.accent))
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));
        f.render_stateful_widget(
            scrollbar,
            area.inner(Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut scrollbar_state,
        );
    }
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

fn draw_settings_modal(f: &mut Frame, s: &AppState, t: Theme) {
    if f.area().width < 50 || f.area().height < 14 {
        let area = centered_rect(70, 40, f.area());
        f.render_widget(Clear, area);
        f.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    " Settings ",
                    Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Terminal too small for settings panel",
                    Style::default().fg(t.text),
                )),
                Line::from(Span::styled(
                    "Resize window and try again",
                    Style::default().fg(t.dim),
                )),
            ])
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(t.accent))
                    .style(Style::default().bg(t.bg_card)),
            ),
            area,
        );
        return;
    }

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

    let prio = match s.settings.priority {
        TestPriority::DownloadFirst => "Download First",
        TestPriority::UploadFirst => "Upload First",
        TestPriority::DownloadOnly => "Download Only",
        TestPriority::UploadOnly => "Upload Only",
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
            "Official Cheat Calc",
            SettingsField::AllowOfficialCheatCalculation,
        ),
        ("Reload from Disk", SettingsField::Reload),
    ];

    for (_idx, (label, field)) in fields.iter().enumerate() {
        let selected = s.settings_focus == *field;
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
        let display_val = setting_display_value(s, *field, selected, prio, allow_cheat);
        rows.push(Line::from(vec![
            Span::styled(format!(" {:<20} ", label), label_style),
            Span::styled(
                format!(" {} ", display_val),
                Style::default().fg(val_fg).bg(val_bg),
            ),
        ]));
    }
    f.render_widget(Paragraph::new(rows), inner);

    let _ = s;
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
                s.settings.smoothing_window_sec.to_string()
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
