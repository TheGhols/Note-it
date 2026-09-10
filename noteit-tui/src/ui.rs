//! Terminal User Interface layout and widgets rendering.
//!
//! Renders a side-by-side view with navigation panels (Recent Notes, Pending Tasks, Trash),
//! quick search results, and a formatted read-only Markdown reader.

use crate::app::{ActivePanel, App, EditorPrompt, Focus};
use crate::draft::Position;
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
            Constraint::Length(
                if app.notice.is_empty()
                    && app.discard_confirmation.is_none()
                    && app.editor_prompt.is_none()
                {
                    1
                } else {
                    4
                },
            ),
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
        match app.focus {
            Focus::Editor => render_editor_pane(frame, app, area),
            Focus::Reader => render_reader_pane(frame, app, area),
            _ => render_list_pane(frame, app, area),
        }
        return;
    }

    // Side-by-side layout: Left (38%), Right (62%)
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(area);

    render_list_pane(frame, app, horizontal[0]);
    if app.focus == Focus::Editor {
        render_editor_pane(frame, app, horizontal[1]);
    } else {
        render_reader_pane(frame, app, horizontal[1]);
    }
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

    let max_items = (area.height as usize / 2).max(1);
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

    let max_items = (area.height as usize / 2).max(1);
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

        // A task's text is a line of the note, so it is read as one: the
        // panel shows `tarefa em negrito`, never `**tarefa em negrito**`.
        let mut spans = vec![
            Span::styled(prefix, Style::default().fg(Color::Yellow)),
            Span::styled(
                "☐ ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ];
        spans.extend(markdown::inline_spans(&task.text, text_style));
        lines.push(Line::from(spans));

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

    let max_items = (area.height as usize / 2).max(1);
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

    let max_items = (area.height as usize / 2).max(1);
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

/// The editing pane: the note's Markdown source, a cursor and a selection.
///
/// This pane deliberately shows *source*. The reader next door is the one
/// rendered projection of a note (Fase 5.0D.1) and it stays the only one;
/// nothing here parses Markdown, so there is no second interpretation to
/// disagree with it. What a person edits is what the file holds.
fn render_editor_pane(frame: &mut Frame, app: &App, area: Rect) {
    let pending = app.pending_text().is_some();
    let title = match &app.current_note {
        Some(note) => format!(
            " Edição: {}{} ",
            noteit_core::search::label_for(&note.content),
            if pending { " ●" } else { "" }
        ),
        None => " Edição ".to_string(),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        // A colour of its own: reading is cyan, editing is amber, and which
        // one has the keyboard is never a guess.
        .border_style(Style::default().fg(Color::Rgb(0xFF, 0xCC, 0x66)))
        .title(title);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(draft) = app.draft.as_ref() else {
        return;
    };
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let height = inner.height as usize;
    let width = inner.width as usize;
    app.editor_viewport.set(height);

    // Only drawing knows the pane's size, so this is where the viewport is
    // moved to contain the cursor — vertically by line and horizontally by
    // character. Both keep their previous position when the cursor is already
    // inside, so the text does not jump under a reader who is only typing.
    let cursor = draft.cursor();
    let mut top = app.editor_scroll.get();
    top = top.min(cursor.line);
    if cursor.line >= top + height {
        top = cursor.line + 1 - height;
    }
    top = top.min(draft.lines().len().saturating_sub(1));
    app.editor_scroll.set(top);

    let mut left = app.editor_column.get();
    left = left.min(cursor.column);
    if cursor.column >= left + width {
        left = cursor.column + 1 - width;
    }
    app.editor_column.set(left);

    let selection = draft.selection();
    let lines: Vec<Line<'static>> = draft
        .lines()
        .iter()
        .enumerate()
        .skip(top)
        .take(height)
        .map(|(row, text)| editor_line(text, row, left, width, cursor, selection))
        .collect();

    frame.render_widget(Paragraph::new(lines), inner);
}

/// Base, selected and cursor styles of the editing pane.
const EDITOR_TEXT: Style = Style::new().fg(Color::White);
const EDITOR_SELECTION: Color = Color::Rgb(0x33, 0x3D, 0x52);

/// One visible line, with its selection painted and its cursor reversed.
///
/// Every character of the note occupies exactly one cell here, which is what
/// keeps the column a cursor reports and the column a reader sees the same
/// number. A control character therefore becomes a visible placeholder rather
/// than disappearing (which would shift the line) or being emitted (which a
/// terminal would obey): a note is data on this screen too.
fn editor_line(
    text: &str,
    row: usize,
    left: usize,
    width: usize,
    cursor: Position,
    selection: Option<(Position, Position)>,
) -> Line<'static> {
    let selected = |column: usize| {
        selection.is_some_and(|(start, end)| {
            let position = Position { line: row, column };
            position >= start && position < end
        })
    };

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut run = String::new();
    let mut run_style: Option<Style> = None;
    let flush = |run: &mut String, style: Option<Style>, spans: &mut Vec<Span<'static>>| {
        if !run.is_empty() {
            spans.push(Span::styled(
                std::mem::take(run),
                style.unwrap_or(EDITOR_TEXT),
            ));
        }
    };

    for (column, character) in (left..).zip(text.chars().skip(left).take(width)) {
        let mut style = EDITOR_TEXT;
        if selected(column) {
            style = style.bg(EDITOR_SELECTION);
        }
        if row == cursor.line && column == cursor.column {
            style = style.add_modifier(Modifier::REVERSED);
        }
        if run_style != Some(style) {
            flush(&mut run, run_style, &mut spans);
            run_style = Some(style);
        }
        run.push(editor_cell(character));
    }
    flush(&mut run, run_style, &mut spans);

    // The caret past the last character is a cell of its own, so an empty line
    // and the end of a line both show where typing would land.
    if row == cursor.line && cursor.column >= text.chars().count() {
        spans.push(Span::styled(
            " ",
            EDITOR_TEXT.add_modifier(Modifier::REVERSED),
        ));
    }

    Line::from(spans)
}

/// The single cell a stored character is drawn as.
fn editor_cell(character: char) -> char {
    match character {
        '\t' => '\u{2192}',
        '\u{0}'..='\u{1F}' | '\u{7F}'..='\u{9F}' => '\u{00B7}',
        other => other,
    }
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

            let mut lines = vec![
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
            ];

            // A note in the trash is still a note, and it is read with the
            // same renderer as any other — a raw dump of the file would spell
            // out the storage the reader is not supposed to see.
            match app
                .current_note
                .as_ref()
                .filter(|note| note.in_trash && note.id == trash.note_id)
            {
                Some(note) => lines.extend(markdown::render_markdown(&note.content)),
                None => lines.push(Line::from(Span::styled(
                    trash.snippet.clone(),
                    Style::default().fg(Color::White),
                ))),
            }

            lines.extend([
                Line::from(""),
                Line::from(Span::styled(
                    "[r] Restaurar a revision exibida",
                    Style::default().fg(Color::DarkGray).italic(),
                )),
            ]);

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
        let mut rendered = markdown::render_with_sources(&doc.content);
        let cursor_line = rendered
            .sources
            .iter()
            .position(|source| *source == app.reader_cursor)
            .unwrap_or(0);
        if is_focused {
            if let Some(line) = rendered.lines.get_mut(cursor_line) {
                line.style = line.style.bg(Color::DarkGray);
            }
        }
        // Start at the selected source line when navigating. Wrapping remains
        // Ratatui's job; a long visual line cannot shift the task's identity.
        let start = if is_focused && cursor_line > 0 {
            lines.clear();
            cursor_line
        } else {
            0
        };
        lines.extend(rendered.lines.into_iter().skip(start));

        let p = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((app.reader_scroll.min(u16::MAX as usize) as u16, 0));

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
    if app.editor_prompt.is_some() || app.discard_confirmation.is_some() || !app.notice.is_empty() {
        let text = match app.editor_prompt {
            Some(EditorPrompt::Pending) => {
                "Alterações não salvas nesta nota. [s] salvar  [d] descartar  [Esc] continuar editando"
            }
            Some(EditorPrompt::Conflict) => {
                "Conflito de revision; nada foi sobrescrito. [p] preservar rascunho  [r] reler a nota  [Esc] manter no editor"
            }
            None if app.discard_confirmation.is_some() => "Mover para a lixeira? [y/N]",
            None => &app.notice,
        };
        frame.render_widget(
            Paragraph::new(text)
                .wrap(Wrap { trim: false })
                .style(Style::default().fg(Color::Yellow)),
            area,
        );
        return;
    }
    let shortcuts = match app.focus {
        Focus::Editor => {
            " [Ctrl+S] Salvar  [Ctrl+Z] Desfazer  [Ctrl+Y] Refazer  [Ctrl+A] Tudo  [Shift+↑↓←→] Selecionar  [Esc] Sair da edição "
        }
        Focus::Search => " [Enter] Abrir Nota  [↑↓] Selecionar  [Esc] Cancelar Busca ",
        Focus::Reader => {
            " [Enter/i] Editar  [↑↓/jk] Cursor  [Space] Tarefa  [e] $EDITOR  [d] Lixeira  [r] Restaurar  [Esc] Voltar  [q] Sair "
        }
        Focus::List => {
            " [Tab/1-3] Painéis  [n] Nova  [r] Restaurar  [/] Buscar  [↑↓/jk] Navegar  [Enter] Ler  [q] Sair "
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
