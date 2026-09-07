//! Terminal User Interface layout and widgets rendering.
//!
//! Renders a side-by-side view with navigation panels (Recent Notes, Pending Tasks, Trash),
//! quick search results, and a formatted read-only Markdown reader.

use crate::app::{ActivePanel, App, Focus};
use crate::markdown;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
    Frame,
};

/// Main render entry point.
pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Guard for degenerate terminal dimensions
    if area.width < 35 || area.height < 6 {
        let minimal =
            Paragraph::new("Note-it (muito pequeno)").style(Style::default().fg(Color::Yellow));
        frame.render_widget(minimal, area);
        return;
    }

    // Top-level vertical layout: Header (3), Main Body (Min), Footer (1)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    render_header(frame, app, chunks[0]);
    render_body(frame, app, chunks[1]);
    render_footer(frame, app, chunks[2]);
}

fn render_header(frame: &mut Frame, app: &App, area: Rect) {
    if app.focus == Focus::Search {
        // Search bar mode
        let search_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Yellow))
            .title(" 🔍 Busca Rápida no Core ")
            .title_alignment(Alignment::Left);

        let query_display = format!("{}▋", app.search_query);
        let content = Line::from(vec![
            Span::styled(
                "Consulta: ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                query_display,
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "  (Enter: abrir, Esc: cancelar, ↑↓: selecionar)",
                Style::default().fg(Color::DarkGray),
            ),
        ]);

        let p = Paragraph::new(content).block(search_block);
        frame.render_widget(p, area);
        return;
    }

    // Normal navigation tab bar
    let header_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" NOTE-IT — Interface de Terminal (Fase 5.0B / 5.0C) ")
        .title_alignment(Alignment::Center);

    let tab_style = |panel: ActivePanel| {
        if app.panel == panel {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        }
    };

    let indicator = |panel: ActivePanel| {
        if app.panel == panel {
            "●"
        } else {
            "○"
        }
    };

    let tabs_line = Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!(
                "{} [1] Notas Recentes ({})",
                indicator(ActivePanel::RecentNotes),
                app.recent_notes.len()
            ),
            tab_style(ActivePanel::RecentNotes),
        ),
        Span::styled("   │   ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!(
                "{} [2] Tarefas Pendentes ({})",
                indicator(ActivePanel::PendingTasks),
                app.pending_tasks.len()
            ),
            tab_style(ActivePanel::PendingTasks),
        ),
        Span::styled("   │   ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!(
                "{} [3] Lixeira ({})",
                indicator(ActivePanel::Trash),
                app.trash_items.len()
            ),
            tab_style(ActivePanel::Trash),
        ),
        Span::styled("   │   ", Style::default().fg(Color::DarkGray)),
        Span::styled("[/] Buscar", Style::default().fg(Color::Cyan)),
    ]);

    let p = Paragraph::new(tabs_line)
        .block(header_block)
        .alignment(Alignment::Center);
    frame.render_widget(p, area);
}

fn render_body(frame: &mut Frame, app: &App, area: Rect) {
    // If area width is constrained, show single pane
    if area.width < 60 {
        if app.focus == Focus::Reader {
            render_reader_pane(frame, app, area);
        } else {
            render_list_pane(frame, app, area);
        }
        return;
    }

    // Side-by-side layout: Left (38%), Right (62%)
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(area);

    render_list_pane(frame, app, horizontal[0]);
    render_reader_pane(frame, app, horizontal[1]);
}

fn render_list_pane(frame: &mut Frame, app: &App, area: Rect) {
    let is_focused = app.focus == Focus::List || app.focus == Focus::Search;
    let border_color = if is_focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    let title = if app.focus == Focus::Search {
        format!(" Resultados da Busca ({}) ", app.search_results.len())
    } else {
        match app.panel {
            ActivePanel::RecentNotes => {
                format!(" {} ({}) ", app.panel.title(), app.recent_notes.len())
            }
            ActivePanel::PendingTasks => {
                format!(" {} ({}) ", app.panel.title(), app.pending_tasks.len())
            }
            ActivePanel::Trash => format!(" {} ({}) ", app.panel.title(), app.trash_items.len()),
        }
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title(title);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    if app.focus == Focus::Search {
        render_search_list(frame, app, inner);
    } else {
        match app.panel {
            ActivePanel::RecentNotes => render_recent_notes_list(frame, app, inner),
            ActivePanel::PendingTasks => render_pending_tasks_list(frame, app, inner),
            ActivePanel::Trash => render_trash_list(frame, app, inner),
        }
    }
}

fn render_recent_notes_list(frame: &mut Frame, app: &App, area: Rect) {
    if app.recent_notes.is_empty() {
        let empty = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "Nenhuma nota encontrada.",
                Style::default().fg(Color::DarkGray).italic(),
            )),
        ])
        .alignment(Alignment::Center);
        frame.render_widget(empty, area);
        return;
    }

    let max_items = area.height as usize / 2;
    let window_start = if app.recent_selected >= max_items {
        app.recent_selected.saturating_sub(max_items - 1)
    } else {
        0
    };

    let mut lines = Vec::new();
    for (idx, note) in app
        .recent_notes
        .iter()
        .enumerate()
        .skip(window_start)
        .take(max_items)
    {
        let selected = idx == app.recent_selected;
        let prefix = if selected { "▶ " } else { "  " };
        let title_style = if selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        let date_str = note
            .updated_at
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "sem data".to_string());

        let tags_str = if note.tags.is_empty() {
            String::new()
        } else {
            format!(" • #{}", note.tags.join(" #"))
        };

        lines.push(Line::from(vec![
            Span::styled(prefix, Style::default().fg(Color::Yellow)),
            Span::styled(note.label.clone(), title_style),
        ]));

        lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(
                format!("{date_str}{tags_str}"),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }

    let p = Paragraph::new(lines);
    frame.render_widget(p, area);
}

fn render_pending_tasks_list(frame: &mut Frame, app: &App, area: Rect) {
    if app.pending_tasks.is_empty() {
        let empty = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "Nenhuma tarefa pendente.",
                Style::default().fg(Color::DarkGray).italic(),
            )),
        ])
        .alignment(Alignment::Center);
        frame.render_widget(empty, area);
        return;
    }

    let max_items = area.height as usize / 2;
    let window_start = if app.tasks_selected >= max_items {
        app.tasks_selected.saturating_sub(max_items - 1)
    } else {
        0
    };

    let mut lines = Vec::new();
    for (idx, task) in app
        .pending_tasks
        .iter()
        .enumerate()
        .skip(window_start)
        .take(max_items)
    {
        let selected = idx == app.tasks_selected;
        let prefix = if selected { "▶ " } else { "  " };
        let text_style = if selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        lines.push(Line::from(vec![
            Span::styled(prefix, Style::default().fg(Color::Yellow)),
            Span::styled(
                "☐ ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(task.text.clone(), text_style),
        ]));

        lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(
                format!("[{}]", task.note_label),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }

    let p = Paragraph::new(lines);
    frame.render_widget(p, area);
}

fn render_trash_list(frame: &mut Frame, app: &App, area: Rect) {
    if app.trash_items.is_empty() {
        let empty = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "Lixeira vazia.",
                Style::default().fg(Color::DarkGray).italic(),
            )),
        ])
        .alignment(Alignment::Center);
        frame.render_widget(empty, area);
        return;
    }

    let max_items = area.height as usize / 2;
    let window_start = if app.trash_selected >= max_items {
        app.trash_selected.saturating_sub(max_items - 1)
    } else {
        0
    };

    let mut lines = Vec::new();
    for (idx, trash) in app
        .trash_items
        .iter()
        .enumerate()
        .skip(window_start)
        .take(max_items)
    {
        let selected = idx == app.trash_selected;
        let prefix = if selected { "▶ " } else { "  " };
        let title_style = if selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        let date_str = trash
            .deleted_at
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "sem data".to_string());

        lines.push(Line::from(vec![
            Span::styled(prefix, Style::default().fg(Color::Yellow)),
            Span::styled("🗑️ ", Style::default()),
            Span::styled(trash.label.clone(), title_style),
        ]));

        lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(
                format!("Excluída em {date_str}"),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }

    let p = Paragraph::new(lines);
    frame.render_widget(p, area);
}

fn render_search_list(frame: &mut Frame, app: &App, area: Rect) {
    if app.search_results.is_empty() {
        let empty = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                if app.search_query.trim().is_empty() {
                    "Digite termos para buscar notas..."
                } else {
                    "Nenhuma correspondência encontrada no Core."
                },
                Style::default().fg(Color::DarkGray).italic(),
            )),
        ])
        .alignment(Alignment::Center);
        frame.render_widget(empty, area);
        return;
    }

    let max_items = area.height as usize / 2;
    let window_start = if app.search_selected >= max_items {
        app.search_selected.saturating_sub(max_items - 1)
    } else {
        0
    };

    let mut lines = Vec::new();
    for (idx, res) in app
        .search_results
        .iter()
        .enumerate()
        .skip(window_start)
        .take(max_items)
    {
        let selected = idx == app.search_selected;
        let prefix = if selected { "▶ " } else { "  " };
        let title_style = if selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        lines.push(Line::from(vec![
            Span::styled(prefix, Style::default().fg(Color::Yellow)),
            Span::styled(res.label.clone(), title_style),
        ]));

        let count_str = if res.match_count > 0 {
            format!("{} correspondência(s) • ", res.match_count)
        } else {
            String::new()
        };

        lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(
                format!("{count_str}{}", res.snippet),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }

    let p = Paragraph::new(lines);
    frame.render_widget(p, area);
}

fn render_reader_pane(frame: &mut Frame, app: &App, area: Rect) {
    let is_focused = app.focus == Focus::Reader;
    let border_color = if is_focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    if app.panel == ActivePanel::Trash && app.focus != Focus::Search {
        // Trash note preview
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border_color))
            .title(" Visualização de Nota na Lixeira ");

        let inner = block.inner(area);
        frame.render_widget(block, area);

        if let Some(trash) = app.trash_items.get(app.trash_selected) {
            let date_str = trash
                .deleted_at
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                .unwrap_or_else(|| "desconhecida".to_string());

            let lines = vec![
                Line::from(vec![
                    Span::styled(
                        "🗑️ NOTA NA LIXEIRA: ",
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        trash.label.clone(),
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(vec![Span::styled(
                    format!("ID: {}  │  Excluída em: {}", trash.note_id, date_str),
                    Style::default().fg(Color::DarkGray),
                )]),
                Line::from(Span::styled(
                    "─────────────────────────────────────────────────────────────────",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Abertura da nota:",
                    Style::default().fg(Color::Yellow),
                )),
                Line::from(Span::styled(
                    trash.snippet.clone(),
                    Style::default().fg(Color::White),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "(As notas na lixeira são somente-leitura nesta fase; restauração em 5.0D)",
                    Style::default().fg(Color::DarkGray).italic(),
                )),
            ];

            let p = Paragraph::new(lines).wrap(Wrap { trim: false });
            frame.render_widget(p, inner);
        } else {
            let empty = Paragraph::new("Nenhuma nota selecionada na lixeira.")
                .style(Style::default().fg(Color::DarkGray).italic());
            frame.render_widget(empty, inner);
        }
        return;
    }

    let note_title = if let Some(doc) = &app.current_note {
        format!(
            " Leitura: {} ",
            noteit_core::search::label_for(&doc.content)
        )
    } else {
        " Leitura de Nota ".to_string()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title(note_title);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if let Some(doc) = &app.current_note {
        let mut lines = Vec::new();

        // Metadata Header
        let date_str = doc
            .metadata
            .updated_at
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "sem data".to_string());

        let tags_str = if doc.user_metadata.tags.is_empty() {
            "nenhuma".to_string()
        } else {
            doc.user_metadata.tags.as_slice().join(", ")
        };

        lines.push(Line::from(vec![Span::styled(
            format!(
                "ID: {}  │  Atualizada: {}  │  Tags: {}",
                doc.metadata.id, date_str, tags_str
            ),
            Style::default().fg(Color::DarkGray),
        )]));
        lines.push(Line::from(Span::styled(
            "─────────────────────────────────────────────────────────────────",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(""));

        // Render Markdown content
        let md_lines = markdown::render_markdown(&doc.content);
        lines.extend(md_lines);

        let p = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((app.reader_scroll as u16, 0));

        frame.render_widget(p, inner);
    } else {
        let empty = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "Selecione uma nota no painel lateral para visualizar.",
                Style::default().fg(Color::DarkGray).italic(),
            )),
        ])
        .alignment(Alignment::Center);
        frame.render_widget(empty, inner);
    }
}

fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    let shortcuts = match app.focus {
        Focus::Search => " [Enter] Abrir Nota  [↑↓] Selecionar  [Esc] Cancelar Busca ",
        Focus::Reader => {
            " [↑↓/jk] Rolar  [PgUp/PgDn] Pág  [Esc/h] Voltar ao Painel  [/] Buscar  [q] Sair "
        }
        Focus::List => {
            " [Tab/1-3] Alternar Painéis  [/] Buscar  [↑↓/jk] Navegar  [Enter/l] Ler  [q/Esc] Sair "
        }
    };

    let line = Line::from(vec![
        Span::styled(
            " Note-it TUI ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(shortcuts, Style::default().fg(Color::White)),
    ]);

    let p = Paragraph::new(line).alignment(Alignment::Left);
    frame.render_widget(p, area);
}
