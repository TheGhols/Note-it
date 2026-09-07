use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

/// Renders the static placeholder view for Phase 5.0B.
pub fn render(frame: &mut Frame) {
    let area = frame.area();

    // If the terminal window is excessively tiny, render a minimal emergency line
    if area.width < 10 || area.height < 3 {
        let minimal =
            Paragraph::new("Note-it (q/Esc: sair)").style(Style::default().fg(Color::Yellow));
        frame.render_widget(minimal, area);
        return;
    }

    let box_height = 7.min(area.height.saturating_sub(2));
    let box_width = 54.min(area.width.saturating_sub(2));

    let vertical_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length((area.height.saturating_sub(box_height)) / 2),
            Constraint::Length(box_height),
            Constraint::Min(0),
        ])
        .split(area);

    let center_row = vertical_chunks[1];

    let horizontal_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length((area.width.saturating_sub(box_width)) / 2),
            Constraint::Length(box_width),
            Constraint::Min(0),
        ])
        .split(center_row);

    let target_rect = horizontal_chunks[1];

    let lines = vec![
        Line::from(vec![
            Span::styled(
                "NOTE-IT",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" — "),
            Span::styled(
                "TUI Shell",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "Fase 5.0B",
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(": Shell de Terminal & Portão de Fronteira"),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::raw("Pressione "),
            Span::styled(
                "q",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" ou "),
            Span::styled(
                "Esc",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" para sair"),
        ]),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Note-it TUI ")
        .title_alignment(Alignment::Center);

    let paragraph = Paragraph::new(lines)
        .block(block)
        .alignment(Alignment::Center);

    frame.render_widget(paragraph, target_rect);
}
