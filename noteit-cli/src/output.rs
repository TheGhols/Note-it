use noteit_core::chrono::{DateTime, Utc};
use noteit_core::{
    MetadataCatalog, NoteDocument, NoteSummary, ReadWarning, SearchResult, StorePaths, TaskEntry,
    TaskStateFilter, TrashEntry, Uuid,
};
use std::collections::BTreeMap;
use std::io::IsTerminal;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputContext {
    pub color_enabled: bool,
}

impl OutputContext {
    /// Detects whether stdout should receive ANSI styling based on TTY and NO_COLOR.
    pub fn for_stdout() -> Self {
        let is_tty = std::io::stdout().is_terminal();
        let no_color = std::env::var_os("NO_COLOR").is_some();
        let term_dumb = std::env::var("TERM").map(|t| t == "dumb").unwrap_or(false);
        Self {
            color_enabled: is_tty && !no_color && !term_dumb,
        }
    }

    /// Explicitly creates a plain (non-ANSI) output context.
    pub fn plain() -> Self {
        Self {
            color_enabled: false,
        }
    }

    /// Explicitly creates a styled (ANSI enabled) output context.
    pub fn styled() -> Self {
        Self {
            color_enabled: true,
        }
    }

    pub fn bold(&self, text: &str) -> String {
        if self.color_enabled {
            format!("\x1b[1m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    pub fn dim(&self, text: &str) -> String {
        if self.color_enabled {
            format!("\x1b[2m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    pub fn green(&self, text: &str) -> String {
        if self.color_enabled {
            format!("\x1b[32m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    pub fn yellow(&self, text: &str) -> String {
        if self.color_enabled {
            format!("\x1b[33m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    pub fn cyan(&self, text: &str) -> String {
        if self.color_enabled {
            format!("\x1b[36m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    pub fn magenta(&self, text: &str) -> String {
        if self.color_enabled {
            format!("\x1b[35m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }
}

/// Neutralizes ANSI escape sequences and dangerous terminal control characters
/// from untrusted inputs (note contents, CLI queries, arguments, paths) while
/// preserving Unicode, tabs, and newlines.
pub fn sanitize_for_terminal(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let ch = chars[i];

        // Neutralize ESC sequences (CSI, OSC, etc.)
        if ch == '\x1b' {
            i += 1;
            if i < len && chars[i] == '[' {
                // CSI sequence: consume parameters up to terminating character (0x40..=0x7E)
                i += 1;
                while i < len {
                    let c = chars[i];
                    if ('\x40'..='\x7e').contains(&c) {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            } else if i < len && chars[i] == ']' {
                // OSC sequence: consume until BEL (\x07) or ST (\x1b\)
                i += 1;
                while i < len {
                    if chars[i] == '\x07' {
                        i += 1;
                        break;
                    }
                    if chars[i] == '\x1b' && i + 1 < len && chars[i + 1] == '\\' {
                        i += 2;
                        break;
                    }
                    i += 1;
                }
            }
            continue;
        }

        // Neutralize dangerous control characters
        match ch {
            // Carriage return: normalize \r\n to \n; standalone \r becomes space
            '\r' => {
                if i + 1 < len && chars[i + 1] == '\n' {
                    out.push('\n');
                    i += 2;
                    continue;
                } else {
                    out.push(' ');
                    i += 1;
                    continue;
                }
            }
            // Allow safe whitespace and newlines
            '\n' | '\t' => {
                out.push(ch);
                i += 1;
            }
            // Strip ASCII control characters and DEL
            '\x00'..='\x08' | '\x0b' | '\x0c' | '\x0e'..='\x1f' | '\x7f' => {
                i += 1;
            }
            // Everything else (Unicode, letters, punctuation, emojis) is preserved
            _ => {
                out.push(ch);
                i += 1;
            }
        }
    }

    out
}

/// Formats a UTC timestamp in the machine's local timezone matching the GUI contract (dd/MM/yyyy HH:mm).
pub fn format_datetime_local(dt: Option<DateTime<Utc>>) -> String {
    format_datetime_with_tz(dt, &noteit_core::chrono::Local)
}

/// Formats a UTC timestamp with a specified timezone.
pub fn format_datetime_with_tz<Tz: noteit_core::chrono::TimeZone>(
    dt: Option<DateTime<Utc>>,
    tz: &Tz,
) -> String
where
    Tz::Offset: std::fmt::Display,
{
    match dt {
        Some(d) => d.with_timezone(tz).format("%d/%m/%Y %H:%M").to_string(),
        None => "desconhecida".to_string(),
    }
}

fn id_prefix(id: &Uuid) -> String {
    let simple = id.as_simple().to_string();
    simple[..8].to_string()
}

pub fn render_welcome(ctx: &OutputContext) -> String {
    let title = ctx.bold("Note-it");
    let subtitle = ctx.dim("Suas notas, também pelo terminal.");
    let hint = format!("Use `{}` para começar.", ctx.bold("noteit ajuda"));

    format!(
        "{title}\n\n{subtitle}\n\n  listar       Listar notas vivas\n  ler          Ler uma nota\n  buscar       Buscar notas\n  tags         Catálogo de tags\n  propriedades Catálogo de propriedades\n  tarefas      Listar tarefas\n  lixeira      Listar notas na lixeira\n  status       Verificar a instalação\n  ajuda        Mostrar ajuda dos comandos\n  versao       Mostrar versão\n\n{hint}\n"
    )
}

pub fn render_help(ctx: &OutputContext) -> String {
    let title = ctx.bold("Note-it CLI");
    let section_usage = ctx.bold("Uso:");
    let section_commands = ctx.bold("Comandos:");
    let section_options = ctx.bold("Opções comuns:");
    let section_aliases = ctx.bold("Aliases internacionais:");

    format!(
        "{title}\n\n{section_usage}\n  noteit <comando> [opções]\n\n{section_commands}\n  listar       Listar notas vivas em ordem de atualização\n  ler <ID>     Ler uma nota pelo UUID ou prefixo de 8 caracteres\n  buscar <Q>   Buscar notas pelo conteúdo de texto\n  tags         Listar tags e contagem de notas\n  propriedades Listar propriedades e contagem de notas\n  tarefas      Listar tarefas (pendentes por padrão)\n  lixeira      Listar notas excluídas recuperáveis\n  status       Verificar o ambiente e store do Note-it\n  ajuda        Mostrar esta ajuda\n  versao       Mostrar a versão\n\n{section_options}\n  --limite, --limit N              Limitar quantidade de resultados (1-100)\n  --tag <TAG>                      Filtrar por tag (repetível com AND)\n  --propriedade, --property K=V    Filtrar por propriedade (repetível com AND)\n  --estado, --state <E>            Estado das tarefas: pendentes, concluidas, todas\n\n{section_aliases}\n  list, read, search, properties, tasks, trash, help, version\n"
    )
}

pub fn render_version(_ctx: &OutputContext) -> String {
    format!("Note-it {}\n", env!("CARGO_PKG_VERSION"))
}

pub fn render_status(ctx: &OutputContext, paths: &StorePaths) -> String {
    let version_line = ctx.bold(&format!("Note-it {}", env!("CARGO_PKG_VERSION")));
    let cli_status = ctx.green("pronta");
    let core_status = ctx.green("disponível");
    let store_status = if paths.store_exists() {
        ctx.green("encontrado")
    } else {
        ctx.yellow("ainda não criado")
    };

    let data_path = sanitize_for_terminal(&paths.data_dir.display().to_string());
    let config_path = sanitize_for_terminal(&paths.config_dir.display().to_string());
    let state_path = sanitize_for_terminal(&paths.state_dir.display().to_string());

    format!(
        "{version_line}\n\nCLI       {cli_status}\nCore      {core_status}\nStore     {store_status}\nDados     {data_path}\nConfig    {config_path}\nEstado    {state_path}\n"
    )
}

pub fn render_notes_list(ctx: &OutputContext, summaries: &[NoteSummary]) -> String {
    if summaries.is_empty() {
        return "Nenhuma nota encontrada.\n".to_string();
    }

    let mut out = String::new();
    out.push_str(&ctx.bold("Notas"));
    out.push_str("\n\n");

    for summary in summaries {
        let prefix = ctx.dim(&id_prefix(&summary.id));
        let label = ctx.bold(&sanitize_for_terminal(&summary.label));
        out.push_str(&format!("{prefix}  {label}\n"));

        if !summary.tags.is_empty() {
            let tags_str = summary.tags.join(" · ");
            out.push_str(&format!(
                "          {}\n",
                ctx.cyan(&sanitize_for_terminal(&tags_str))
            ));
        }

        let time_info = if let Some(updated) = summary.updated_at {
            format!("atualizada {}", format_datetime_local(Some(updated)))
        } else if let Some(created) = summary.created_at {
            format!("criada {}", format_datetime_local(Some(created)))
        } else {
            "data desconhecida".to_string()
        };

        out.push_str(&format!("          {}\n\n", ctx.dim(&time_info)));
    }

    let count = summaries.len();
    let count_label = if count == 1 {
        "1 nota"
    } else {
        &format!("{count} notas")
    };
    out.push_str(&ctx.dim(count_label));
    out.push('\n');

    out
}

pub fn render_note_read(ctx: &OutputContext, doc: &NoteDocument) -> String {
    let label = noteit_core::search::label_for(&doc.content);
    let mut out = String::new();

    out.push_str(&ctx.bold(&sanitize_for_terminal(&label)));
    out.push_str("\n\n");

    out.push_str(&format!("ID: {}\n", doc.metadata.id));

    if !doc.user_metadata.tags.is_empty() {
        let tags_str = doc.user_metadata.tags.as_slice().join(" · ");
        out.push_str(&format!(
            "Tags: {}\n",
            ctx.cyan(&sanitize_for_terminal(&tags_str))
        ));
    }

    if let Some(created) = doc.metadata.created_at {
        out.push_str(&format!(
            "Criada: {}\n",
            format_datetime_local(Some(created))
        ));
    }
    if let Some(updated) = doc.metadata.updated_at {
        out.push_str(&format!(
            "Atualizada: {}\n",
            format_datetime_local(Some(updated))
        ));
    }

    if !doc.user_metadata.properties.is_empty() {
        out.push('\n');
        out.push_str(&ctx.bold("Propriedades"));
        out.push('\n');
        for prop in doc.user_metadata.properties.as_slice() {
            let key = sanitize_for_terminal(&prop.key);
            let val = sanitize_for_terminal(&prop.value);
            out.push_str(&format!("  {:<16}  {}\n", key, val));
        }
    }

    out.push_str("\n────────────────────────────────────────\n\n");
    out.push_str(&sanitize_for_terminal(&doc.content));
    out.push('\n');

    out
}

pub fn render_search_results(ctx: &OutputContext, query: &str, results: &[SearchResult]) -> String {
    if results.is_empty() {
        return "Nenhuma nota encontrada.\n".to_string();
    }

    let sanitized_query = sanitize_for_terminal(query);
    let mut out = String::new();
    out.push_str(&ctx.bold(&format!("Busca: {sanitized_query}")));
    out.push_str("\n\n");

    for res in results {
        let prefix = ctx.dim(&id_prefix(&res.note_id));
        let label = ctx.bold(&sanitize_for_terminal(&res.label));
        let snippet = sanitize_for_terminal(&res.snippet);

        out.push_str(&format!("{prefix}  {label}\n"));
        out.push_str(&format!("          {snippet}\n"));

        if res.match_count > 0 {
            let count_label = if res.match_count == 1 {
                "1 ocorrência".to_string()
            } else {
                format!("{} ocorrências", res.match_count)
            };
            out.push_str(&format!("          {}\n", ctx.dim(&count_label)));
        }
        out.push('\n');
    }

    let count = results.len();
    let count_label = if count == 1 {
        "1 nota encontrada"
    } else {
        &format!("{count} notas encontradas")
    };
    out.push_str(&ctx.dim(count_label));
    out.push('\n');

    out
}

pub fn render_tags(ctx: &OutputContext, catalog: &MetadataCatalog) -> String {
    if catalog.tags.is_empty() {
        return "Nenhuma tag encontrada.\n".to_string();
    }

    let mut out = String::new();
    out.push_str(&ctx.bold("Tags"));
    out.push_str("\n\n");

    let max_len = catalog
        .tags
        .iter()
        .map(|entry| entry.tag.chars().count())
        .max()
        .unwrap_or(10)
        .max(8);

    for entry in &catalog.tags {
        let tag_display = sanitize_for_terminal(&entry.tag);
        let tag_styled = ctx.cyan(&tag_display);
        let pad = max_len.saturating_sub(tag_display.chars().count());
        out.push_str(&format!(
            "{}{}    {:>4}\n",
            tag_styled,
            " ".repeat(pad),
            entry.note_count
        ));
    }

    let count = catalog.tags.len();
    let count_label = if count == 1 {
        "1 tag"
    } else {
        &format!("{count} tags")
    };
    out.push_str(&format!("\n{}\n", ctx.dim(count_label)));

    out
}

pub fn render_properties(ctx: &OutputContext, catalog: &MetadataCatalog) -> String {
    if catalog.property_keys.is_empty() {
        return "Nenhuma propriedade encontrada.\n".to_string();
    }

    let mut out = String::new();
    out.push_str(&ctx.bold("Propriedades"));
    out.push_str("\n\n");

    let max_len = catalog
        .property_keys
        .iter()
        .map(|entry| entry.key.chars().count())
        .max()
        .unwrap_or(12)
        .max(8);

    for entry in &catalog.property_keys {
        let key_display = sanitize_for_terminal(&entry.key);
        let pad = max_len.saturating_sub(key_display.chars().count());
        out.push_str(&format!(
            "{}{}    {:>4}\n",
            key_display,
            " ".repeat(pad),
            entry.note_count
        ));
    }

    let count = catalog.property_keys.len();
    let count_label = if count == 1 {
        "1 propriedade"
    } else {
        &format!("{count} propriedades")
    };
    out.push_str(&format!("\n{}\n", ctx.dim(count_label)));

    out
}

pub fn render_tasks(ctx: &OutputContext, tasks: &[TaskEntry], state: TaskStateFilter) -> String {
    if tasks.is_empty() {
        return match state {
            TaskStateFilter::Pending => "Nenhuma tarefa pendente.\n".to_string(),
            TaskStateFilter::Completed => "Nenhuma tarefa concluída.\n".to_string(),
            TaskStateFilter::All => "Nenhuma tarefa encontrada.\n".to_string(),
        };
    }

    let mut out = String::new();
    out.push_str(&ctx.bold("Tarefas"));
    out.push_str("\n\n");

    // Group tasks by note while maintaining recency order of first occurrence
    let mut by_note: BTreeMap<Uuid, (String, Vec<&TaskEntry>)> = BTreeMap::new();
    let mut note_order: Vec<Uuid> = Vec::new();

    for t in tasks {
        let entry = by_note.entry(t.note_id).or_insert_with(|| {
            note_order.push(t.note_id);
            (t.note_label.clone(), Vec::new())
        });
        entry.1.push(t);
    }

    for note_id in note_order {
        let (label, note_tasks) = &by_note[&note_id];
        let prefix = ctx.dim(&id_prefix(&note_id));
        let label_styled = ctx.bold(&sanitize_for_terminal(label));
        out.push_str(&format!("{prefix}  {label_styled}\n"));

        for task in note_tasks {
            let indent = "  ".repeat(task.depth.saturating_add(1));
            let check_box = if task.checked {
                ctx.green("[x]")
            } else {
                "[ ]".to_string()
            };
            let text = sanitize_for_terminal(&task.text);

            let completed_suffix = if let Some(dt) = task.completed_at {
                format!(
                    " {}",
                    ctx.dim(&format!("(concluída {})", format_datetime_local(Some(dt))))
                )
            } else {
                String::new()
            };

            out.push_str(&format!("{indent}{check_box} {text}{completed_suffix}\n"));
        }

        out.push('\n');
    }

    let count = tasks.len();
    let count_label = if count == 1 {
        "1 tarefa"
    } else {
        &format!("{count} tarefas")
    };
    out.push_str(&ctx.dim(count_label));
    out.push('\n');

    out
}

pub fn render_trash(ctx: &OutputContext, entries: &[TrashEntry]) -> String {
    if entries.is_empty() {
        return "A lixeira está vazia.\n".to_string();
    }

    let mut out = String::new();
    out.push_str(&ctx.bold("Lixeira"));
    out.push_str("\n\n");

    for entry in entries {
        let prefix = ctx.dim(&id_prefix(&entry.note_id));
        let label = ctx.bold(&sanitize_for_terminal(&entry.label));
        let snippet = sanitize_for_terminal(&entry.snippet);
        let time_info = if let Some(deleted) = entry.deleted_at {
            format!("removida {}", format_datetime_local(Some(deleted)))
        } else {
            "data de remoção desconhecida".to_string()
        };

        out.push_str(&format!("{prefix}  {label}\n"));
        out.push_str(&format!("          {snippet}\n"));
        out.push_str(&format!("          {}\n\n", ctx.dim(&time_info)));
    }

    let count = entries.len();
    let count_label = if count == 1 {
        "1 nota na lixeira"
    } else {
        &format!("{count} notas na lixeira")
    };
    out.push_str(&ctx.dim(count_label));
    out.push('\n');

    out
}

pub fn render_warning(ctx: &OutputContext, warning: &ReadWarning) -> String {
    let prefix_str = if let Some(id) = &warning.note_id {
        format!("nota {} ", id_prefix(id))
    } else {
        "nota ".to_string()
    };
    let sanitized_msg = sanitize_for_terminal(&warning.message);
    format!(
        "{} a {prefix_str}foi ignorada por erro de leitura: {sanitized_msg}\n",
        ctx.yellow("Aviso:")
    )
}

pub fn render_error(ctx: &OutputContext, err: &clap::Error) -> String {
    use clap::error::{ContextKind, ContextValue, ErrorKind};

    let error_prefix = ctx.bold("Erro:");
    let hint_help = ctx.bold("noteit ajuda");

    let extract_context_str = |kind: ContextKind| -> Option<String> {
        match err.get(kind) {
            Some(ContextValue::String(s)) => Some(s.clone()),
            Some(ContextValue::Strings(strs)) => strs.first().cloned(),
            Some(ContextValue::StyledStr(s)) => Some(s.to_string()),
            Some(ContextValue::StyledStrs(strs)) => strs.first().map(|s| s.to_string()),
            _ => None,
        }
    };

    match err.kind() {
        ErrorKind::InvalidSubcommand => {
            let name = extract_context_str(ContextKind::InvalidSubcommand).unwrap_or_default();
            let sanitized_name = sanitize_for_terminal(&name);
            if !sanitized_name.is_empty() {
                format!(
                    "{error_prefix} comando desconhecido `{sanitized_name}`.\n\nUse `{hint_help}` para ver os comandos disponíveis.\n"
                )
            } else {
                format!(
                    "{error_prefix} comando desconhecido.\n\nUse `{hint_help}` para ver os comandos disponíveis.\n"
                )
            }
        }
        ErrorKind::UnknownArgument => {
            let arg = extract_context_str(ContextKind::InvalidArg).unwrap_or_default();
            let sanitized_arg = sanitize_for_terminal(&arg);
            if sanitized_arg.starts_with('-') {
                format!(
                    "{error_prefix} opção desconhecida `{sanitized_arg}`.\n\nUse `{hint_help}` para ver os comandos e opções disponíveis.\n"
                )
            } else if !sanitized_arg.is_empty() {
                format!(
                    "{error_prefix} argumento inesperado `{sanitized_arg}`.\n\nUse `{hint_help}` para ver o formato correto de uso.\n"
                )
            } else {
                format!(
                    "{error_prefix} argumento ou opção desconhecida.\n\nUse `{hint_help}` para ver os comandos e opções disponíveis.\n"
                )
            }
        }
        ErrorKind::MissingRequiredArgument => {
            let arg = extract_context_str(ContextKind::InvalidArg).unwrap_or_default();
            let sanitized_arg = sanitize_for_terminal(&arg);
            if !sanitized_arg.is_empty() {
                format!(
                    "{error_prefix} argumento obrigatório `{sanitized_arg}` não fornecido.\n\nUse `{hint_help}` para ver o formato correto de uso.\n"
                )
            } else {
                format!(
                    "{error_prefix} argumento obrigatório não fornecido.\n\nUse `{hint_help}` para ver o formato correto de uso.\n"
                )
            }
        }
        _ => {
            format!(
                "{error_prefix} uso inválido.\n\nUse `{hint_help}` para ver os comandos e opções disponíveis.\n"
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noteit_core::chrono::{FixedOffset, TimeZone};

    #[test]
    fn terminal_sanitization_removes_dangerous_escapes_and_preserves_unicode() {
        // Clear screen ANSI escape
        let malicious_clear = "Texto antes\x1b[2Jtexto depois";
        assert_eq!(
            sanitize_for_terminal(malicious_clear),
            "Texto antestexto depois"
        );

        // OSC 52 clipboard injection
        let malicious_osc = "Início\x1b]52;c;Y29waWVk\x07Fim";
        assert_eq!(sanitize_for_terminal(malicious_osc), "InícioFim");

        // BEL and Backspace
        let bell_backspace = "Alarme\x07\x08teste";
        assert_eq!(sanitize_for_terminal(bell_backspace), "Alarmeteste");

        // Standalone carriage return
        let cr_trick = "Linha1\rLinha2\r\nLinha3";
        assert_eq!(sanitize_for_terminal(cr_trick), "Linha1 Linha2\nLinha3");

        // Unicode, accents, emoji and Markdown preservation
        let good_text = "# Título com acentos: Biópsia & Coração 🎉\n- [x] Tarefa 1\n\t* Subitem";
        assert_eq!(sanitize_for_terminal(good_text), good_text);
    }

    #[test]
    fn deterministic_timezone_formatting_matches_gui_contract() {
        // UTC instant: 2026-09-01T22:30:00Z
        let utc_dt = Utc.with_ymd_and_hms(2026, 9, 1, 22, 30, 0).unwrap();

        // In UTC-3 (e.g. America/Sao_Paulo without DST)
        let tz_sp = FixedOffset::east_opt(-3 * 3600).expect("fixed offset -3h");
        assert_eq!(
            format_datetime_with_tz(Some(utc_dt), &tz_sp),
            "01/09/2026 19:30"
        );

        // In UTC+2 (e.g. Europe/Paris summer time or Cairo)
        let tz_cairo = FixedOffset::east_opt(2 * 3600).expect("fixed offset +2h");
        assert_eq!(
            format_datetime_with_tz(Some(utc_dt), &tz_cairo),
            "02/09/2026 00:30"
        );

        // In UTC
        assert_eq!(
            format_datetime_with_tz(Some(utc_dt), &Utc),
            "01/09/2026 22:30"
        );

        // Unknown timestamp
        assert_eq!(format_datetime_with_tz(None, &tz_sp), "desconhecida");
    }
}
