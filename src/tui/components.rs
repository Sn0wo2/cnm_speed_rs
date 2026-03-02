use ratatui::prelude::Alignment;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

#[derive(Clone, Copy)]
pub struct Theme {
    pub accent: Color,
    pub highlight: Color,
    pub text: Color,
    pub dim: Color,
    pub border: Color,
    pub success: Color,
    pub error: Color,
    pub bg_card: Color,
}

impl Theme {
    pub fn default() -> Self {
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

pub fn apple_block(title: &'static str, t: Theme) -> Block<'static> {
    Block::default()
        .title(Span::styled(
            title,
            Style::default().fg(t.dim).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::TOP)
        .border_style(Style::default().fg(t.border))
}

pub fn render_apple_button(f: &mut Frame, text: &str, color: Color, area: Rect) {
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

pub fn line_kv(key: &str, value: &str, val_color: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!(" {:<12} ", key),
            Style::default().fg(Color::Rgb(140, 140, 150)),
        ),
        Span::styled(value.to_string(), Style::default().fg(val_color)),
    ])
}

pub fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
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
