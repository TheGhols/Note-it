//! What a person reads.
//!
//! One of the two renderers over [`crate::outcome`]; the other is
//! [`crate::machine`]. This one styles, abbreviates identifiers, shows dates
//! in the machine's own timezone and neutralises terminal escapes, because all
//! of those are right for a terminal and none of them is right for a parser.
//! Neither renderer is ever built from the other's output.

use crate::outcome::{
    CliResponse, CommandError, Executed, HelpText, Outcome, ReadError, SemanticStatusReport,
    StatusReport, UsageError,
};
use noteit_core::chrono::{DateTime, Utc};
use noteit_core::settings::{SemanticFallbackPolicy, SemanticMode, SemanticProvider};
use noteit_core::write::{WriteError, WriteOutcome, WriteOutcomeKind};
use noteit_core::{
    MetadataCatalog, NoteDocument, NoteSelectorError, NoteSummary, ReadWarning, SearchResult,
    TaskEntry, TaskStateFilter, TrashEntry, Uuid,
};
use std::collections::BTreeMap;
use std::io::IsTerminal;

/// What one output channel can do.
///
/// A value rather than a call to [`IsTerminal`] wherever a decision happens,
/// so every renderer is a pure function of it and the whole matrix — styled,
/// plain, wide, narrow — is reachable from a test with no terminal at all.
///
/// Each channel gets its own: standard output being a terminal says nothing
/// about standard error, and a warning styled into a redirected file is a
/// warning nobody can grep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputContext {
    /// Whether this channel accepts ANSI.
    pub color_enabled: bool,
    /// Columns the terminal reported, when one did.
    ///
    /// `None` is "nobody said", not "zero": a pipe has no width, and neither
    /// does a terminal that answered nonsense. Layout uses
    /// [`Self::effective_width`], which turns that into an assumption.
    pub width: Option<usize>,
    /// Whether block-drawing characters can be expected to arrive intact.
    ///
    /// A separate question from colour, and it has a different answer: a file
    /// holds UTF-8 perfectly well, so a pipe still gets the wordmark, while
    /// `TERM=dumb` is a terminal saying it has no capabilities to speak of and
    /// is not one to hand six lines of box-drawing to.
    pub block_art_enabled: bool,
}

impl OutputContext {
    /// The width assumed when no terminal reported one.
    ///
    /// Eighty columns is what every terminal emulator opens at and what a
    /// redirected file is read back in, and it is wide enough for the whole
    /// presentation — so the conservative answer is also the complete one.
    pub const ASSUMED_WIDTH: usize = 80;

    /// Column counts worth believing. Zero columns is a terminal saying it
    /// does not know, and a five-digit width is a stale variable rather than
    /// a window.
    const PLAUSIBLE_WIDTH: std::ops::RangeInclusive<usize> = 1..=10_000;

    /// What standard output can do, asked of standard output itself.
    pub fn for_stdout() -> Self {
        let is_tty = std::io::stdout().is_terminal();
        Self {
            color_enabled: Self::color_allowed(is_tty),
            width: Self::detect_width(is_tty),
            block_art_enabled: !Self::term_is_dumb(),
        }
    }

    /// What standard error can do, asked of standard error itself.
    ///
    /// Width is not detected here because nothing is laid out to this channel:
    /// errors and warnings are sentences, and sentences wrap.
    pub fn for_stderr() -> Self {
        Self {
            color_enabled: Self::color_allowed(std::io::stderr().is_terminal()),
            width: None,
            block_art_enabled: !Self::term_is_dumb(),
        }
    }

    /// The shared half of both answers: a terminal, and neither of the two
    /// conventions by which a person turns styling off.
    ///
    /// `NO_COLOR` counts when it is set at all, including to the empty string,
    /// which is what the convention asks for.
    fn color_allowed(is_tty: bool) -> bool {
        let no_color = std::env::var_os("NO_COLOR").is_some();
        is_tty && !no_color && !Self::term_is_dumb()
    }

    fn term_is_dumb() -> bool {
        std::env::var("TERM")
            .map(|term| term == "dumb")
            .unwrap_or(false)
    }

    /// How wide the terminal on standard output is.
    ///
    /// A pipe is never measured: there is no window behind it, and a `COLUMNS`
    /// inherited from the shell that started the pipeline describes a window
    /// the output is not going to. When there is a terminal it is asked
    /// directly — `TIOCGWINSZ` is the window, where `COLUMNS` is a variable
    /// that may not have been updated since it was last resized — and the
    /// variable is only the fallback.
    fn detect_width(is_tty: bool) -> Option<usize> {
        if !is_tty {
            return None;
        }
        Self::window_columns().or_else(Self::columns_variable)
    }

    fn window_columns() -> Option<usize> {
        let mut size: libc::winsize = unsafe { std::mem::zeroed() };
        // SAFETY: `TIOCGWINSZ` reads nothing and writes one `winsize`, into a
        // `winsize` this function owns for the length of the call. A failed
        // call is reported in the return value and leaves the struct zeroed,
        // which the plausibility check below rejects anyway.
        let answered = unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut size) };
        if answered != 0 {
            return None;
        }
        Self::plausible(usize::from(size.ws_col))
    }

    fn columns_variable() -> Option<usize> {
        let raw = std::env::var("COLUMNS").ok()?;
        Self::plausible(raw.trim().parse::<usize>().ok()?)
    }

    fn plausible(columns: usize) -> Option<usize> {
        Self::PLAUSIBLE_WIDTH.contains(&columns).then_some(columns)
    }

    /// The width to lay out for: what the terminal said, or the assumption.
    pub fn effective_width(&self) -> usize {
        self.width.unwrap_or(Self::ASSUMED_WIDTH)
    }

    /// Explicitly creates a plain (non-ANSI) output context.
    pub fn plain() -> Self {
        Self {
            color_enabled: false,
            width: None,
            block_art_enabled: true,
        }
    }

    /// Explicitly creates a styled (ANSI enabled) output context.
    pub fn styled() -> Self {
        Self {
            color_enabled: true,
            width: None,
            block_art_enabled: true,
        }
    }

    /// The same context, laying out for a window of a stated size.
    pub fn with_width(self, width: Option<usize>) -> Self {
        Self { width, ..self }
    }

    /// The same context, for a terminal that can or cannot draw blocks.
    pub fn with_block_art(self, block_art_enabled: bool) -> Self {
        Self {
            block_art_enabled,
            ..self
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

/// Both channels, each with its own answer about what it can do.
///
/// One execution writes to two places, and they are not the same place: `noteit
/// listar > lista.txt` has a terminal on standard error and a file on standard
/// output, and `noteit listar 2> erros.txt` has it the other way round. Carrying
/// the pair means the renderer never has to guess which one it is writing to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Channels {
    pub stdout: OutputContext,
    pub stderr: OutputContext,
}

impl Channels {
    /// What this process actually has, asked of each channel separately.
    pub fn detect() -> Self {
        Self {
            stdout: OutputContext::for_stdout(),
            stderr: OutputContext::for_stderr(),
        }
    }

    /// Both channels plain, which is what a test wants unless it says otherwise.
    pub fn plain() -> Self {
        Self {
            stdout: OutputContext::plain(),
            stderr: OutputContext::plain(),
        }
    }

    /// Both channels styled, the shape of a process attached to a terminal.
    pub fn styled() -> Self {
        Self {
            stdout: OutputContext::styled(),
            stderr: OutputContext::styled(),
        }
    }

    /// The same pair, laying standard output out for a window of a stated size.
    pub fn with_width(self, width: Option<usize>) -> Self {
        Self {
            stdout: self.stdout.with_width(width),
            ..self
        }
    }

    /// The same pair, for a terminal that can or cannot draw blocks.
    pub fn with_block_art(self, block_art_enabled: bool) -> Self {
        Self {
            stdout: self.stdout.with_block_art(block_art_enabled),
            stderr: self.stderr.with_block_art(block_art_enabled),
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

        // Neutralize Unicode bidirectional control characters (Trojan Source)
        // by replacing them with an explicit, visible, non-spoofing representation.
        if is_bidi_control(ch) {
            use std::fmt::Write;
            let _ = write!(out, "[U+{:04X}]", ch as u32);
            i += 1;
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
            // Everything else (Unicode letters, punctuation, emojis, natural RTL) is preserved
            _ => {
                out.push(ch);
                i += 1;
            }
        }
    }

    out
}

/// Identifies invisible Unicode bidirectional override/isolate/embedding control characters.
pub fn is_bidi_control(c: char) -> bool {
    matches!(
        c,
        '\u{202A}'..='\u{202E}'
            | '\u{2066}'..='\u{2069}'
            | '\u{200E}'
            | '\u{200F}'
            | '\u{061C}'
    )
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

/// The presentation `noteit` shows when it was given nothing to do.
///
/// Delegated rather than written here: it is the one screen whose shape
/// depends on the size of the window as well as on whether the channel takes
/// colour, and [`crate::welcome`] is where that lives.
pub fn render_welcome(ctx: &OutputContext) -> String {
    crate::welcome::render(ctx)
}

/// The help, which is the same text whether it was asked for by name, by flag
/// or in Portuguese.
///
/// It documents every option this CLI really has and no option it does not,
/// including the two clap answers for itself — `--help` and `--version` are
/// real arguments, and a help that omits them is wrong about the program.
/// The presentation belongs to `noteit` alone and is deliberately not repeated
/// here: help is a reference, and a reference does not open with a logo.
pub fn render_help(ctx: &OutputContext) -> String {
    let title = ctx.bold("Note-it CLI");
    let section_usage = ctx.bold("Uso:");
    let section_reading = ctx.bold("Leitura:");
    let section_writing = ctx.bold("Escrita:");
    let section_options = ctx.bold("Opções comuns:");
    let section_examples = ctx.bold("Exemplos:");
    let section_aliases = ctx.bold("Aliases internacionais:");

    format!(
        "{title}\n\n\
         {section_usage}\n  noteit <comando> [opções]\n\n\
         {section_reading}\n\
         \x20 listar       Listar notas vivas em ordem de atualização\n\
         \x20 ler <ID>     Ler uma nota pelo UUID ou prefixo de 8 caracteres\n\
         \x20 buscar <Q>   Buscar notas pelo conteúdo de texto\n\
         \x20 tags         Listar tags e contagem de notas\n\
         \x20 propriedades Listar propriedades e contagem de notas\n\
         \x20 tarefas      Listar tarefas (pendentes por padrão), com a referência de cada uma\n\
         \x20 lixeira      Listar notas excluídas recuperáveis\n\
         \x20 status       Verificar o ambiente e store do Note-it\n\
         \x20 ajuda        Mostrar esta ajuda\n\
         \x20 versao       Mostrar a versão\n\n\
         \x20 Nenhum comando de leitura altera o store.\n\n\
         {section_writing}\n\
         \x20 criar [TEXTO]                      Criar uma nota e devolver o identificador dela\n\
         \x20 adicionar <ID> <TEXTO>             Acrescentar Markdown ao final de uma nota\n\
         \x20 editar <ID> <TEXTO>                Substituir todo o corpo Markdown da nota\n\
         \x20 tags adicionar <ID> <TAG>          Adicionar uma tag\n\
         \x20 tags remover <ID> <TAG>            Remover uma tag\n\
         \x20 propriedades definir <ID> K=V      Definir uma propriedade\n\
         \x20 propriedades remover <ID> K        Remover uma propriedade\n\
         \x20 tarefas concluir <ID> <REF>        Concluir a tarefa que a referência nomeia\n\
         \x20 tarefas reabrir <ID> <REF>         Reabrir uma tarefa concluída\n\
         \x20 lixeira restaurar <ID>             Restaurar uma nota da lixeira\n\n\
         \x20 Nenhum comando de escrita abre uma janela, muda o foco ou altera a configuração.\n\
         \x20 Com o Note-it aberto, a alteração passa por ele; sem o Note-it, é feita direto.\n\n\
         {section_options}\n\
         \x20 --limite, --limit N              Limitar quantidade de resultados (1-100)\n\
         \x20 --tag <TAG>                      Filtrar por tag, ou aplicar uma em `criar`\n\
         \x20 --propriedade, --property K=V    Filtrar por propriedade, ou aplicar uma em `criar`\n\
         \x20 --estado, --state <E>            Estado das tarefas: pendentes, concluidas, todas\n\
         \x20                                  (também aceita pending, completed, all)\n\
         \x20 --stdin                          Ler o texto da entrada padrão (criar, adicionar, editar)\n\
         \x20 --vazio, --empty                 Esvaziar o corpo da nota, de propósito (editar)\n\
         \x20 --json                           Devolver um único documento JSON em vez de texto,\n\
         \x20                                  para scripts e agentes. Vale antes ou depois do\n\
         \x20                                  comando, e desliga cor, dica e apresentação\n\
         \x20 --help, -h                       Mostrar esta ajuda; depois de um comando,\n\
         \x20                                  mostrar a ajuda daquele comando\n\
         \x20 --version, -V                    Mostrar a versão\n\n\
         {section_examples}\n\
         \x20 noteit listar --limite 5\n\
         \x20 noteit buscar \"choque séptico\" --tag Medicina\n\
         \x20 noteit criar \"Minha nota\" --tag Medicina\n\
         \x20 echo \"- [ ] revisar\" | noteit adicionar 8c4f1a2b --stdin\n\
         \x20 noteit --json listar\n\
         \x20 noteit listar --help\n\n\
         {section_aliases}\n\
         \x20 list, read, search, properties, tasks, trash, help, version\n\
         \x20 create, append, edit, add, remove, set, complete, reopen, restore\n"
    )
}

pub fn render_version(_ctx: &OutputContext) -> String {
    format!("Note-it {}\n", env!("CARGO_PKG_VERSION"))
}

pub fn render_status(ctx: &OutputContext, report: &StatusReport) -> String {
    let paths = &report.paths;
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
        "{version_line}\n\nCLI       {cli_status}\nCore      {core_status}\nStore     {store_status}\nDados     {data_path}\nConfig    {config_path}\nEstado    {state_path}\n{}",
        render_semantic_status(ctx, &report.semantic)
    )
}

/// The retrieval half of `noteit status`.
///
/// Says what the configuration says and what the filesystem shows, and stops
/// there. No digest, no vector, no note text — and the artifact directory,
/// which is here because somebody provisioning a model needs to know where it
/// goes. That is a local diagnostic on the user's own machine; `noteit_context`
/// publishes none of it.
fn render_semantic_status(ctx: &OutputContext, semantic: &SemanticStatusReport) -> String {
    let mode = match semantic.mode {
        SemanticMode::LexicalOnly => ctx.dim("lexical_only (padrão de fábrica)"),
        SemanticMode::Semantic => ctx.green("semantic"),
    };
    let mut out = format!("\nSemântica {mode}\n");
    if !semantic.enabled {
        // Nothing below would be true of a machine in this state: no provider
        // is constructed, no artifact is read and no index exists.
        out.push_str(&format!(
            "          {}\n",
            ctx.dim("nenhum modelo é carregado e nada é baixado")
        ));
        if semantic.mode == SemanticMode::Semantic {
            out.push_str(&format!(
                "          {}\n",
                ctx.yellow("fallback = lexical_only mantém o canal desligado")
            ));
        }
        return out;
    }
    let availability = if semantic.artifact_present {
        ctx.green("presente")
    } else {
        ctx.yellow("ausente — rode scripts/fetch-embedding-artifact")
    };
    let fallback = match semantic.fallback {
        SemanticFallbackPolicy::Automatic => "automatic",
        SemanticFallbackPolicy::SemanticRequired => "semantic_required",
        SemanticFallbackPolicy::LexicalOnly => "lexical_only",
    };
    let provider = match semantic.provider {
        SemanticProvider::Local => "local (em processo, nada sai desta máquina)",
    };
    out.push_str(&format!("Provider  {provider}\n"));
    out.push_str(&format!("Modelo    {}\n", semantic.model));
    out.push_str(&format!("Fallback  {fallback}\n"));
    out.push_str(&format!("Artefato  {availability}\n"));
    if let Some(directory) = &semantic.artifact_directory {
        out.push_str(&format!(
            "          {}\n",
            sanitize_for_terminal(&directory.display().to_string())
        ));
    }
    // The index lives in the process that answers questions, which is the MCP
    // server and never this one. Saying so is more useful than a number that
    // would always be zero here.
    out.push_str(&format!(
        "Índice    {}\n",
        ctx.dim("em memória, por processo — construído por quem responde consultas")
    ));
    out
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
            // The reference the write commands name this task by. Dimmed and
            // in front, exactly as the note's own identifier is above it, so
            // the listing stays a listing rather than becoming a table.
            let reference = ctx.dim(task.task_ref.as_str());
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

            out.push_str(&format!(
                "{indent}{reference}  {check_box} {text}{completed_suffix}\n"
            ));
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

/// What a person reads after a write that went through.
///
/// Two successful outcomes are told apart here: something changed, or the note
/// already said exactly that and was left alone. Both are successes and
/// neither is a warning — repeating a command that made no difference is not a
/// mistake, and being told nothing happened is more useful than being told it
/// worked.
///
/// No path is ever printed. A note is named by its identifier, the way every
/// other command in this CLI names one.
pub fn render_write_outcome(ctx: &OutputContext, outcome: &WriteOutcome) -> String {
    if !outcome.changed {
        let sentence = match outcome.kind {
            WriteOutcomeKind::TagAdded => "A nota já tem essa tag. Nada foi alterado.",
            WriteOutcomeKind::TagRemoved => "A nota não tem essa tag. Nada foi alterado.",
            WriteOutcomeKind::PropertySet => "A propriedade já tem esse valor. Nada foi alterado.",
            WriteOutcomeKind::PropertyRemoved => {
                "A nota não tem essa propriedade. Nada foi alterado."
            }
            WriteOutcomeKind::TaskCompleted => "A tarefa já estava concluída. Nada foi alterado.",
            WriteOutcomeKind::TaskReopened => "A tarefa já estava aberta. Nada foi alterado.",
            _ => "A nota já estava assim. Nada foi alterado.",
        };
        return format!("{sentence}\n");
    }

    let prefix = ctx.dim(&id_prefix(&outcome.note_id));
    match outcome.kind {
        // The one place the whole identifier is printed: a note that has just
        // been created has no other name yet, and whoever asked for it needs
        // something they can address it by.
        WriteOutcomeKind::NoteCreated => {
            format!("Nota criada: {}\n", ctx.bold(&outcome.note_id.to_string()))
        }
        WriteOutcomeKind::ContentAppended | WriteOutcomeKind::ContentReplaced => {
            format!("Nota atualizada: {prefix}\n")
        }
        WriteOutcomeKind::ContentCleared => format!("Nota esvaziada: {prefix}\n"),
        WriteOutcomeKind::TagAdded => "Tag adicionada.\n".to_string(),
        WriteOutcomeKind::TagRemoved => "Tag removida.\n".to_string(),
        WriteOutcomeKind::PropertySet => "Propriedade atualizada.\n".to_string(),
        WriteOutcomeKind::PropertyRemoved => "Propriedade removida.\n".to_string(),
        WriteOutcomeKind::TaskCompleted => "Tarefa concluída.\n".to_string(),
        WriteOutcomeKind::TaskReopened => "Tarefa reaberta.\n".to_string(),
        WriteOutcomeKind::NoteRestored => format!("Nota restaurada: {prefix}\n"),
    }
}

/// A warning about a write that *did* happen.
///
/// Deliberately worded so nobody reads it as a failure and runs the command
/// again: the change is on disk, and only the open window is behind.
pub fn render_write_warning(ctx: &OutputContext, detail: &str) -> String {
    format!(
        "{} A alteração foi gravada, mas a janela aberta pode não estar \
         mostrando o texto novo. Não repita o comando: {}\n",
        ctx.yellow("Aviso:"),
        sanitize_for_terminal(detail)
    )
}

/// The sentence for a write that did not happen.
///
/// Every branch says what the store is now, because that is the question the
/// person actually has. The one that cannot answer it — an indeterminate
/// result — says so instead of guessing, and asks them to look rather than
/// inviting a retry that could duplicate the text.
pub fn render_write_error(ctx: &OutputContext, error: &WriteError) -> String {
    let message = match error {
        WriteError::InvalidInput { detail } | WriteError::Validation { detail } => {
            sanitize_for_terminal(detail)
        }
        WriteError::NotFound { selector } => format!(
            "nenhuma nota encontrada para o seletor `{}`.",
            sanitize_for_terminal(selector)
        ),
        WriteError::AmbiguousSelector { selector, matches } => format!(
            "seletor ambíguo `{}` corresponde a {matches} notas.",
            sanitize_for_terminal(selector)
        ),
        WriteError::RevisionConflict {
            current_revision, ..
        } => format!(
            "a nota mudou desde a leitura e nada foi gravado. \
             A revisão atual é `{current_revision}`; leia a nota de novo, \
             confira o que mudou e refaça a alteração sobre ela.",
        ),
        WriteError::StaleTaskRef { task_ref } => format!(
            "a referência `{}` não corresponde mais a uma tarefa desta nota. \
             A nota mudou; liste as tarefas de novo. Nada foi alterado.",
            sanitize_for_terminal(task_ref)
        ),
        WriteError::AmbiguousTaskRef { task_ref } => format!(
            "a referência `{}` corresponde a mais de uma tarefa. Nada foi alterado.",
            sanitize_for_terminal(task_ref)
        ),
        WriteError::WriterBusy { detail } => format!(
            "o repositório está sendo usado por outro escritor do Note-it. \
             Nenhuma alteração foi feita: {}",
            sanitize_for_terminal(detail)
        ),
        WriteError::AuthorityUnavailable { .. } => {
            "o repositório está sendo usado por outro escritor do Note-it, mas a \
             autoridade não pôde ser contatada. Nenhuma alteração foi feita."
                .to_string()
        }
        WriteError::Indeterminate { .. } => {
            "a conexão caiu antes da resposta, então não é possível dizer se a \
             alteração foi gravada. Verifique a nota antes de repetir o comando."
                .to_string()
        }
        WriteError::TrashTargetOccupied { note_id } => format!(
            "já existe uma nota ativa com o identificador {}. Nada foi alterado.",
            id_prefix(note_id)
        ),
        WriteError::Persistence { detail } => format!(
            "a alteração não pôde ser gravada e a nota continua como estava: {}",
            sanitize_for_terminal(detail)
        ),
        WriteError::StoreUnavailable { detail } => format!(
            "repositório indisponível: {}",
            sanitize_for_terminal(detail)
        ),
    };
    format!("{} {message}\n", ctx.bold("Erro:"))
}

/// Which exit code a refusal carries.
///
/// The split is the one the rest of the CLI already uses: `2` is "you asked
/// for something that is not a valid request", `1` is "the request was
/// understood and could not be carried out". A stale task reference is
/// deliberately the second — the command was well formed and the note simply
/// moved on.
pub fn exit_code_for_write_error(error: &WriteError) -> u8 {
    match error {
        WriteError::InvalidInput { .. } | WriteError::Validation { .. } => 2,
        _ => 1,
    }
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

/// Everything one execution says to a person, on the two channels it says it on.
///
/// The single place the human adapter is entered from. Warnings go to standard
/// error and results to standard output, exactly as they always have — the
/// difference since Phase 4.0F is that both are values, so nothing can slip
/// onto a channel from the middle of a command, and since Phase 4.0G each
/// channel is styled for itself rather than for whatever standard output
/// happened to be.
pub fn render(executed: &Executed, channels: &Channels) -> CliResponse {
    match &executed.result {
        Ok(outcome) => CliResponse {
            exit_code: crate::EXIT_SUCCESS,
            stdout: render_outcome(&channels.stdout, outcome),
            // Warnings are written to standard error and are styled for
            // standard error, which may be a file while standard output is a
            // terminal, or the other way round.
            stderr: render_outcome_warnings(&channels.stderr, outcome),
        },
        Err(error) => CliResponse::failure(
            error.exit_code(),
            render_command_error(&channels.stderr, error),
        ),
    }
}

fn render_outcome(ctx: &OutputContext, outcome: &Outcome) -> String {
    match outcome {
        Outcome::Welcome => render_welcome(ctx),
        Outcome::Help(HelpText::Own) => render_help(ctx),
        Outcome::Help(HelpText::Sub(text)) => text.clone(),
        Outcome::Version => render_version(ctx),
        Outcome::Status(report) => render_status(ctx, report),
        Outcome::Notes(batch) => render_notes_list(ctx, &batch.items),
        Outcome::Note { document, .. } => render_note_read(ctx, document),
        Outcome::Search { query, batch } => render_search_results(ctx, query, &batch.items),
        Outcome::Tags { catalog, .. } => render_tags(ctx, catalog),
        Outcome::Properties { catalog, .. } => render_properties(ctx, catalog),
        Outcome::Tasks { state, batch } => render_tasks(ctx, &batch.items, *state),
        Outcome::Trash(entries) => render_trash(ctx, entries),
        Outcome::Write(outcome) => render_write_outcome(ctx, outcome),
    }
}

fn render_outcome_warnings(ctx: &OutputContext, outcome: &Outcome) -> String {
    let mut out = String::new();
    let read_warnings: &[ReadWarning] = match outcome {
        Outcome::Notes(batch) => &batch.warnings,
        Outcome::Search { batch, .. } => &batch.warnings,
        Outcome::Tasks { batch, .. } => &batch.warnings,
        Outcome::Tags { warnings, .. } => warnings,
        Outcome::Properties { warnings, .. } => warnings,
        _ => &[],
    };
    for warning in read_warnings {
        out.push_str(&render_warning(ctx, warning));
    }
    if let Outcome::Write(outcome) = outcome {
        // A committed write whose window could not be refreshed is still a
        // committed write. The warning goes here and the success line to
        // standard output, so nothing about it reads as "try that again".
        if let Some(detail) = &outcome.ui_sync_warning {
            out.push_str(&render_write_warning(ctx, detail));
        }
    }
    out
}

pub fn render_command_error(ctx: &OutputContext, error: &CommandError) -> String {
    match error {
        CommandError::Usage(usage) => render_usage_error(ctx, usage),
        CommandError::Read(read) => render_read_error(ctx, read),
        CommandError::Write(write) => render_write_error(ctx, write),
    }
}

/// A malformed request, said the way this CLI has always said one.
pub fn render_usage_error(ctx: &OutputContext, error: &UsageError) -> String {
    format!(
        "{} {}\n\nUse `{}` {}.\n",
        ctx.bold("Erro:"),
        error.sentence_with(sanitize_for_terminal),
        ctx.bold("noteit ajuda"),
        error.hint().phrase()
    )
}

/// A read that was understood and could not be carried out.
pub fn render_read_error(ctx: &OutputContext, error: &ReadError) -> String {
    let message = match error {
        ReadError::Selector(NoteSelectorError::InvalidFormat(selector)) => {
            let selector = sanitize_for_terminal(selector);
            format!(
                "formato de seletor inválido `{selector}`. Forneça um UUID completo ou prefixo de no mínimo 8 caracteres hexadecimais."
            )
        }
        ReadError::Selector(NoteSelectorError::NotFound(selector)) => {
            let selector = sanitize_for_terminal(selector);
            format!("nenhuma nota encontrada para o seletor `{selector}`.")
        }
        ReadError::Selector(NoteSelectorError::Ambiguous(selector, matches)) => {
            let selector = sanitize_for_terminal(selector);
            let count = matches.len();
            format!("seletor ambíguo `{selector}` corresponde a {count} notas vivas.")
        }
        ReadError::Selector(NoteSelectorError::SymlinkRefused(selector)) => {
            let selector = sanitize_for_terminal(selector);
            format!("a nota `{selector}` é um link simbólico e não pode ser aberta.")
        }
        ReadError::Selector(NoteSelectorError::StoreUnavailable(reason)) => {
            let reason = sanitize_for_terminal(reason);
            format!("repositório indisponível: {reason}")
        }
        ReadError::NoteRead { detail } | ReadError::Listing { detail } => {
            sanitize_for_terminal(detail)
        }
    };
    format!("{} {message}\n", ctx.bold("Erro:"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outcome::{Command, Executed};
    use noteit_core::chrono::{FixedOffset, TimeZone};

    /// The pair a terminal on one channel and a file on the other produces.
    fn split(stdout_styled: bool, stderr_styled: bool) -> Channels {
        Channels {
            stdout: if stdout_styled {
                OutputContext::styled()
            } else {
                OutputContext::plain()
            },
            stderr: if stderr_styled {
                OutputContext::styled()
            } else {
                OutputContext::plain()
            },
        }
    }

    #[test]
    fn a_redirected_channel_is_never_styled_because_the_other_one_is_a_terminal() {
        // `noteit comando-inexistente 2> erros.txt` from a terminal: the
        // sentence is going into a file, and a file gets no escapes. Before
        // Phase 4.0G both channels shared standard output's answer, and this
        // is the case that got it wrong.
        let failed = Executed::failed(
            Some(Command::List),
            CommandError::Usage(UsageError::detail("comando desconhecido `batata`.")),
        );

        let terminal_out_file_err = render(&failed, &split(true, false));
        assert!(
            !terminal_out_file_err.stderr.contains('\u{1b}'),
            "a redirected standard error was styled: {:?}",
            terminal_out_file_err.stderr
        );
        assert!(terminal_out_file_err.stderr.contains("Erro:"));

        // And the other way round: `noteit ... > saida.txt` still styles the
        // error it prints to the terminal the person is looking at.
        let file_out_terminal_err = render(&failed, &split(false, true));
        assert!(
            file_out_terminal_err.stderr.contains('\u{1b}'),
            "a terminal on standard error stopped being styled"
        );

        // The two say the same thing, and only the styling differs.
        assert_eq!(
            sanitize_for_terminal(&file_out_terminal_err.stderr),
            terminal_out_file_err.stderr
        );
    }

    #[test]
    fn results_follow_standard_output_and_warnings_follow_standard_error() {
        let executed = Executed::ok(Command::Welcome, Outcome::Welcome);
        let styled_result = render(&executed, &split(true, false));
        assert!(
            styled_result.stdout.contains('\u{1b}'),
            "a terminal on standard output stopped being styled"
        );
        assert!(styled_result.stderr.is_empty());

        let plain_result = render(&executed, &split(false, true));
        assert!(
            !plain_result.stdout.contains('\u{1b}'),
            "a redirected standard output was styled"
        );
    }

    #[test]
    fn the_help_is_a_reference_and_never_opens_with_the_presentation() {
        let help = render_help(&OutputContext::plain());
        assert!(!help.contains('█'), "the help opened with the wordmark");
        assert!(!help.contains("Comece por:"));
        // And the presentation is not the help either: it points at it.
        let presentation = render_welcome(&OutputContext::plain());
        assert!(presentation.contains("noteit ajuda"));
        assert!(!presentation.contains("Aliases internacionais:"));
    }

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
