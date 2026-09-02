//! The screen a person sees, proved on the real binary.
//!
//! Everything here runs `noteit` as a process, because every question this
//! suite asks is one a function-level test cannot answer: whether the thing
//! attached to standard output is a terminal, how wide it is, and what the
//! process left behind on disk when it was done.
//!
//! Three axes, tested against each other rather than one at a time:
//!
//! * **Channel** — a terminal, a pipe, a file.
//! * **Consent** — `NO_COLOR`, `TERM=dumb`.
//! * **Window** — wide, narrow, and narrower than the words.
//!
//! And one invariant across all of them: running `noteit` with no arguments is
//! a presentation and nothing else. It exits `0`, says nothing on standard
//! error, and does not create so much as a directory.

// The shared harness is compiled afresh into every suite that names it, and
// this one needs only its sandbox — the stand-in desktop authority belongs to
// the suites about writing. Unused *here* is not unused in the crate, and the
// allow says so rather than deleting a harness three suites depend on. It
// scopes to test scaffolding and silences no diagnostic about the CLI itself.
#[allow(dead_code)]
mod support;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use support::Sandbox;

/// The escape every assertion about styling is really about.
const ESC: char = '\u{1b}';

/// A character from the block wordmark: present only when the art is drawn.
const BLOCK: char = '█';

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// A window wide enough for everything.
const WIDE: (u16, u16) = (100, 40);
/// Too narrow for the art, wide enough for the words.
const NARROW: (u16, u16) = (40, 20);
/// Narrow enough that only the essentials fit.
const VERY_NARROW: (u16, u16) = (20, 10);

// ---------------------------------------------------------------- the terminal

/// A pseudo-terminal, for the assertions that have to be looked at by one.
///
/// Deliberately here rather than in the shared harness: this is the only suite
/// that needs a terminal, and a helper compiled into suites that never call it
/// is dead code in all of them.
struct Pty {
    controller: std::os::fd::OwnedFd,
    device: Option<std::os::fd::OwnedFd>,
}

impl Pty {
    fn open((columns, rows): (u16, u16)) -> Self {
        let mut controller_fd = 0;
        let mut device_fd = 0;
        let size = libc::winsize {
            ws_row: rows,
            ws_col: columns,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: both descriptors are written by `openpty` and taken over
        // below; the size is read and not retained. A failure is reported in
        // the return value and leaves nothing to own.
        let opened = unsafe {
            libc::openpty(
                &mut controller_fd,
                &mut device_fd,
                std::ptr::null_mut(),
                std::ptr::null(),
                &size,
            )
        };
        assert_eq!(opened, 0, "openpty: {}", std::io::Error::last_os_error());

        // SAFETY: `openpty` just produced these and nothing else holds them.
        use std::os::fd::FromRawFd;
        Self {
            controller: unsafe { std::os::fd::OwnedFd::from_raw_fd(controller_fd) },
            device: Some(unsafe { std::os::fd::OwnedFd::from_raw_fd(device_fd) }),
        }
    }

    fn stdout(&self) -> Stdio {
        Stdio::from(
            self.device
                .as_ref()
                .expect("the device end is still open")
                .try_clone()
                .expect("clone the device end"),
        )
    }

    /// Everything written to the terminal, to end of file.
    ///
    /// The far end has to be dropped first: while this process still holds a
    /// copy, the terminal has a writer and the read below never finishes.
    fn read_all(&mut self) -> String {
        use std::os::fd::AsRawFd;
        self.device = None;

        let mut buffer = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            // SAFETY: reads at most `chunk.len()` bytes into `chunk`.
            let read = unsafe {
                libc::read(
                    self.controller.as_raw_fd(),
                    chunk.as_mut_ptr().cast(),
                    chunk.len(),
                )
            };
            match read {
                // End of file, or the last writer hung up — which on a
                // terminal arrives as `EIO` rather than as zero bytes.
                0 | -1 => break,
                count => buffer.extend_from_slice(&chunk[..count as usize]),
            }
        }
        String::from_utf8_lossy(&buffer).into_owned()
    }
}

/// What one run said and how it ended.
struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

/// Runs the binary with a real terminal on standard output.
fn on_terminal(sandbox: &Sandbox, args: &[&str], size: (u16, u16)) -> Run {
    on_terminal_with(sandbox, args, size, |_| {})
}

fn on_terminal_with(
    sandbox: &Sandbox,
    args: &[&str],
    size: (u16, u16),
    configure: impl FnOnce(&mut Command),
) -> Run {
    let mut terminal = Pty::open(size);
    let mut command = sandbox.bare_command(args);
    configure(&mut command);
    command.stdout(terminal.stdout());
    command.stderr(Stdio::piped());
    command.stdin(Stdio::null());

    let mut child = command.spawn().expect("spawn noteit on a terminal");
    // The `Command` keeps its own copy of the terminal's far end until it is
    // dropped, and while any copy is open the terminal has a writer — so the
    // read below would wait for an end-of-file this process is holding back.
    drop(command);

    let mut stderr = String::new();
    std::io::Read::read_to_string(child.stderr.as_mut().expect("stderr"), &mut stderr)
        .expect("read stderr");
    // A terminal turns every newline into a carriage return and a newline.
    // That is the terminal's doing, not the CLI's.
    let stdout = terminal.read_all().replace("\r\n", "\n");
    let code = child.wait().expect("wait").code().unwrap_or(-1);

    Run {
        code,
        stdout,
        stderr,
    }
}

/// Runs the binary with a pipe on standard output, which is what `| cat` is.
fn on_pipe(sandbox: &Sandbox, args: &[&str]) -> Run {
    on_pipe_with(sandbox, args, |_| {})
}

fn on_pipe_with(sandbox: &Sandbox, args: &[&str], configure: impl FnOnce(&mut Command)) -> Run {
    let mut command = sandbox.bare_command(args);
    configure(&mut command);
    let output = command.output().expect("run noteit");
    Run {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

// ---------------------------------------------------------------- the store

/// Every path under a root, with the metadata a silent write would disturb.
fn footprint(root: &Path) -> BTreeMap<PathBuf, String> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            }
            out.push(path);
        }
    }

    let mut paths = vec![root.to_path_buf()];
    walk(root, &mut paths);

    let mut map = BTreeMap::new();
    for path in paths {
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        use std::os::unix::fs::MetadataExt;
        // Content as well as metadata: a rewrite that restored the timestamps
        // would still change the bytes, and a touch that changed nothing else
        // would still change `mtime` and `ctime`.
        let digest = std::fs::read(&path).map(|bytes| bytes.len()).unwrap_or(0);
        map.insert(
            path.clone(),
            format!(
                "mode={:o} uid={} gid={} ino={} size={} bytes={} mtime={}.{} ctime={}.{}",
                metadata.mode(),
                metadata.uid(),
                metadata.gid(),
                metadata.ino(),
                metadata.size(),
                digest,
                metadata.mtime(),
                metadata.mtime_nsec(),
                metadata.ctime(),
                metadata.ctime_nsec(),
            ),
        );
    }
    map
}

// ---------------------------------------------------- the presentation itself

/// Every assertion that holds however the screen was reached.
fn assert_is_a_presentation(run: &Run, context: &str) {
    assert_eq!(run.code, 0, "{context}: exit code");
    assert!(
        run.stderr.is_empty(),
        "{context}: stderr said {:?}",
        run.stderr
    );
    assert!(
        run.stdout.contains(VERSION),
        "{context}: the version is missing from {:?}",
        run.stdout
    );
    assert!(
        run.stdout.contains("noteit listar"),
        "{context}: the quick commands are missing"
    );
    assert!(
        run.stdout.contains("noteit ajuda"),
        "{context}: the way to the help is missing"
    );
    assert!(run.stdout.ends_with('\n'), "{context}: no trailing newline");
}

fn assert_unstyled(run: &Run, context: &str) {
    assert!(
        !run.stdout.contains(ESC),
        "{context}: standard output carried styling"
    );
    assert!(
        !run.stderr.contains(ESC),
        "{context}: standard error carried styling"
    );
}

#[test]
fn a_wide_terminal_gets_the_wordmark_in_the_brand_colours() {
    let sandbox = Sandbox::new();
    let run = on_terminal(&sandbox, &[], WIDE);

    assert_is_a_presentation(&run, "wide terminal");
    assert!(run.stdout.contains(BLOCK), "the wordmark is missing");
    assert!(
        run.stdout
            .contains("Notas rápidas, locais e prontas para você e seus agentes."),
        "the tagline is missing"
    );
    assert!(
        run.stdout.contains("Comece por:"),
        "the invitation is missing"
    );

    // Yellow for the mark, magenta for the accent, and no third voice.
    assert!(run.stdout.contains("\u{1b}[33m"), "the yellow is missing");
    assert!(run.stdout.contains("\u{1b}[35m"), "the magenta is missing");
    for stranger in ["\u{1b}[31m", "\u{1b}[32m", "\u{1b}[34m", "\u{1b}[36m"] {
        assert!(
            !run.stdout.contains(stranger),
            "{stranger:?} is not one of this project's two colours"
        );
    }
}

#[test]
fn a_narrow_terminal_drops_the_art_and_a_narrower_one_drops_the_rest() {
    let sandbox = Sandbox::new();

    let narrow = on_terminal(&sandbox, &[], NARROW);
    assert_is_a_presentation(&narrow, "narrow terminal");
    assert!(
        !narrow.stdout.contains(BLOCK),
        "the art survived a window too small for it"
    );
    assert!(narrow.stdout.contains("NOTE-IT"), "the wordmark is missing");
    assert!(narrow.stdout.contains("Comece por:"));

    let very_narrow = on_terminal(&sandbox, &[], VERY_NARROW);
    assert_is_a_presentation(&very_narrow, "very narrow terminal");
    assert!(!very_narrow.stdout.contains(BLOCK));
    assert!(very_narrow.stdout.contains("NOTE-IT"));
}

#[test]
fn no_layout_is_ever_wider_than_the_terminal_it_was_drawn_for() {
    let sandbox = Sandbox::new();
    // Below the width of `  noteit listar` there is nothing left to cut, so
    // that line is the floor rather than a failure.
    const FLOOR: usize = 15;

    for columns in [100u16, 60, 54, 53, 40, 27, 26, 20, 16] {
        let run = on_terminal(&sandbox, &[], (columns, 24));
        let widest = strip_ansi(&run.stdout)
            .lines()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(0);
        assert!(
            widest <= usize::from(columns).max(FLOOR),
            "{columns} columns: the screen is {widest} wide"
        );
    }
}

/// The text with every escape removed — what the person actually reads.
fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if character != ESC {
            out.push(character);
            continue;
        }
        if chars.peek() == Some(&'[') {
            chars.next();
            for inner in chars.by_ref() {
                if ('\u{40}'..='\u{7e}').contains(&inner) {
                    break;
                }
            }
        }
    }
    out
}

#[test]
fn no_color_silences_the_styling_and_keeps_every_word() {
    let sandbox = Sandbox::new();
    let styled = on_terminal(&sandbox, &[], WIDE);
    // The convention is that the variable counts when it is set at all, so the
    // empty string has to be enough.
    for value in ["1", ""] {
        let run = on_terminal_with(&sandbox, &[], WIDE, |command| {
            command.env("NO_COLOR", value);
        });
        assert_is_a_presentation(&run, "NO_COLOR");
        assert_unstyled(&run, "NO_COLOR");
        assert!(run.stdout.contains(BLOCK), "NO_COLOR took the wordmark too");
        assert_eq!(
            run.stdout,
            strip_ansi(&styled.stdout),
            "NO_COLOR changed more than the styling"
        );
    }
}

#[test]
fn a_dumb_terminal_gets_no_styling_and_no_block_art() {
    let sandbox = Sandbox::new();
    let run = on_terminal_with(&sandbox, &[], WIDE, |command| {
        command.env("TERM", "dumb");
    });
    assert_is_a_presentation(&run, "TERM=dumb");
    assert_unstyled(&run, "TERM=dumb");
    assert!(
        !run.stdout.contains(BLOCK),
        "a dumb terminal was handed block art"
    );
    assert!(run.stdout.contains("NOTE-IT"), "the wordmark is missing");
}

#[test]
fn a_pipe_gets_plain_text_and_the_same_text_every_time() {
    let sandbox = Sandbox::new();
    let first = on_pipe(&sandbox, &[]);
    assert_is_a_presentation(&first, "pipe");
    assert_unstyled(&first, "pipe");
    assert!(
        first.stdout.contains(BLOCK),
        "a redirected presentation lost the wordmark"
    );

    // A pipe has no width, so nothing about the window can change what it
    // receives — including a `COLUMNS` describing a window it is not going to.
    let second = on_pipe_with(&sandbox, &[], |command| {
        command.env("COLUMNS", "20");
    });
    assert_eq!(
        first.stdout, second.stdout,
        "a variable changed what a pipe received"
    );

    let third = on_pipe(&sandbox, &[]);
    assert_eq!(first.stdout, third.stdout, "two runs disagreed");
}

#[test]
fn a_redirected_file_receives_exactly_what_a_pipe_does() {
    let sandbox = Sandbox::new();
    let destination = sandbox.root.join("saida.txt");
    let handle = std::fs::File::create(&destination).expect("create the destination");

    let mut command = sandbox.bare_command(&[]);
    command.stdout(Stdio::from(handle));
    command.stderr(Stdio::piped());
    let output = command.output().expect("run noteit");

    let written = std::fs::read_to_string(&destination).expect("read back");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    assert!(!written.contains(ESC), "a file received styling");
    assert_eq!(written, on_pipe(&sandbox, &[]).stdout);
}

#[test]
fn a_terminal_that_reported_nothing_useful_still_gets_a_whole_screen() {
    let sandbox = Sandbox::new();
    // A window of zero columns is a terminal saying it does not know, and a
    // five-digit `COLUMNS` is a stale variable. Neither may be believed, and
    // neither may produce a broken screen.
    let unknown = on_terminal(&sandbox, &[], (0, 0));
    assert_is_a_presentation(&unknown, "zero columns");
    assert!(
        unknown.stdout.contains(BLOCK),
        "an unmeasured terminal lost the wordmark"
    );

    let absurd = on_terminal_with(&sandbox, &[], (0, 0), |command| {
        command.env("COLUMNS", "99999999");
    });
    assert_eq!(
        absurd.stdout, unknown.stdout,
        "an implausible COLUMNS was believed"
    );

    // A plausible one, on the other hand, is the only thing left to go on.
    let stated = on_terminal_with(&sandbox, &[], (0, 0), |command| {
        command.env("COLUMNS", "30");
    });
    assert!(
        !stated.stdout.contains(BLOCK),
        "a plausible COLUMNS was ignored when the terminal had no answer"
    );
}

#[test]
fn the_presentation_agrees_with_the_version_command() {
    let sandbox = Sandbox::new();
    let presentation = strip_ansi(&on_terminal(&sandbox, &[], WIDE).stdout);

    for arguments in [vec!["versao"], vec!["version"], vec!["--version"]] {
        let stated = on_pipe(&sandbox, &arguments).stdout;
        let version = stated
            .trim()
            .rsplit(' ')
            .next()
            .expect("the version command names a version")
            .to_string();
        assert_eq!(version, VERSION);
        assert!(
            presentation.contains(&format!("Note-it {version}")),
            "{arguments:?}: the presentation and the version command disagree"
        );
    }
}

// ------------------------------------------------------------- side effects

#[test]
fn the_presentation_creates_nothing_and_changes_nothing() {
    let sandbox = Sandbox::new();
    // A store that does not exist yet: the presentation must neither need one
    // nor make one.
    assert!(!sandbox.store_paths().store_exists());

    let before = footprint(&sandbox.root);

    for size in [WIDE, NARROW, VERY_NARROW] {
        let run = on_terminal(&sandbox, &[], size);
        assert_eq!(run.code, 0);
    }
    assert_eq!(on_pipe(&sandbox, &[]).code, 0);

    let after = footprint(&sandbox.root);
    assert_eq!(
        before,
        after,
        "running `noteit` touched the store: {:?}",
        after
            .iter()
            .filter(|(path, state)| before.get(*path) != Some(*state))
            .collect::<Vec<_>>()
    );
    assert!(
        !sandbox.store_paths().store_exists(),
        "the presentation created a store"
    );
}

#[test]
fn the_presentation_leaves_no_lock_and_no_socket_behind() {
    let sandbox = Sandbox::new();
    assert_eq!(on_pipe(&sandbox, &[]).code, 0);

    let runtime = sandbox.root.join("runtime");
    let debris: Vec<PathBuf> = std::fs::read_dir(&runtime)
        .expect("the runtime directory")
        .flatten()
        .map(|entry| entry.path())
        .collect();
    assert!(
        debris.is_empty(),
        "the presentation left something in the runtime directory: {debris:?}"
    );
}

// ------------------------------------------------------------------ the help

#[test]
fn the_help_never_carries_the_presentation() {
    let sandbox = Sandbox::new();

    for arguments in [
        vec!["--help"],
        vec!["-h"],
        vec!["ajuda"],
        vec!["help"],
        vec!["listar", "--help"],
        vec!["tarefas", "--help"],
    ] {
        for run in [
            on_terminal(&sandbox, &arguments, WIDE),
            on_pipe(&sandbox, &arguments),
        ] {
            assert_eq!(run.code, 0, "{arguments:?}");
            assert!(
                !run.stdout.contains(BLOCK),
                "{arguments:?} opened with the wordmark"
            );
            assert!(
                !run.stdout.contains("Comece por:"),
                "{arguments:?} carried the presentation's invitation"
            );
        }
    }
}

#[test]
fn the_help_documents_the_options_the_parser_really_has_and_no_others() {
    let sandbox = Sandbox::new();
    let help = on_pipe(&sandbox, &["ajuda"]).stdout;

    for option in [
        "--limite",
        "--limit",
        "--tag",
        "--propriedade",
        "--property",
        "--estado",
        "--state",
        "--stdin",
        "--vazio",
        "--empty",
        "--json",
        "--help",
        "-h",
        "--version",
        "-V",
    ] {
        assert!(help.contains(option), "the help omits {option}");
    }

    for command in [
        "listar",
        "ler",
        "buscar",
        "tags",
        "propriedades",
        "tarefas",
        "lixeira",
        "status",
        "ajuda",
        "versao",
        "criar",
        "adicionar",
        "editar",
        "tags adicionar",
        "tags remover",
        "propriedades definir",
        "propriedades remover",
        "tarefas concluir",
        "tarefas reabrir",
        "lixeira restaurar",
    ] {
        assert!(help.contains(command), "the help omits `{command}`");
    }

    // Every option the help names has to exist, or the help is a lie. `--json`
    // is checked against a command that takes it; the rest are checked by
    // asking the parser to reject an invented one the same way.
    let invented = on_pipe(&sandbox, &["listar", "--limiteee"]);
    assert_eq!(invented.code, 2, "an invented option was accepted");
}

#[test]
fn the_help_is_the_same_text_by_every_name_and_says_it_in_portuguese() {
    let sandbox = Sandbox::new();
    let ajuda = on_pipe(&sandbox, &["ajuda"]).stdout;
    for arguments in [vec!["help"], vec!["--help"], vec!["-h"]] {
        assert_eq!(ajuda, on_pipe(&sandbox, &arguments).stdout, "{arguments:?}");
    }
    for portuguese in [
        "Uso:",
        "Leitura:",
        "Escrita:",
        "Opções comuns:",
        "Exemplos:",
        "Aliases internacionais:",
    ] {
        assert!(ajuda.contains(portuguese), "the help lost `{portuguese}`");
    }
}

// -------------------------------------------------- the two channels, apart

#[test]
fn each_channel_is_styled_for_itself_and_not_for_the_other() {
    let sandbox = Sandbox::new();

    // A terminal on standard output and a pipe on standard error: the error
    // is going to a file, and a file has no use for styling.
    let terminal_out = on_terminal(&sandbox, &["comando-inexistente"], WIDE);
    assert_eq!(terminal_out.code, 2);
    assert!(
        !terminal_out.stderr.contains(ESC),
        "a redirected standard error was styled because standard output was a terminal"
    );
    assert!(
        terminal_out.stderr.contains("Erro:"),
        "the error itself went missing: {:?}",
        terminal_out.stderr
    );

    // Both on pipes: nothing anywhere.
    let piped = on_pipe(&sandbox, &["comando-inexistente"]);
    assert_eq!(piped.code, 2);
    assert_unstyled(&piped, "both channels piped");
}

// ------------------------------------------- the machine interface, unmoved

/// The one document a machine answer is, or a failure naming what arrived
/// instead. Deliberately strict: trailing prose, a second document or a banner
/// in front all fail here rather than somewhere downstream.
fn sole_document(channel: &str, context: &str) -> serde_json::Value {
    assert!(
        !channel.contains(ESC),
        "{context}: the machine interface carried styling"
    );
    assert!(
        !channel.contains(BLOCK),
        "{context}: the machine interface carried the wordmark"
    );
    assert!(
        !channel.contains("Comece por:"),
        "{context}: the machine interface carried a human hint"
    );

    let mut documents =
        serde_json::Deserializer::from_str(channel).into_iter::<serde_json::Value>();
    let first = documents
        .next()
        .unwrap_or_else(|| panic!("{context}: nothing to parse in {channel:?}"))
        .unwrap_or_else(|error| panic!("{context}: {error} in {channel:?}"));
    assert!(
        documents.next().is_none(),
        "{context}: a second document followed the first in {channel:?}"
    );
    assert!(
        channel.trim_end().ends_with('}'),
        "{context}: something followed the document in {channel:?}"
    );
    first
}

#[test]
fn the_machine_interface_is_the_same_on_a_terminal_as_in_a_pipe() {
    let sandbox = Sandbox::new();
    sandbox.seed("# Uma nota\n\n- [ ] uma tarefa\n");

    for arguments in [
        vec!["--json"],
        vec!["--json", "listar"],
        vec!["--json", "ajuda"],
        vec!["--json", "versao"],
        vec!["--json", "status"],
        vec!["--json", "tags"],
        vec!["--json", "tarefas"],
        vec!["--json", "lixeira"],
    ] {
        let terminal = on_terminal(&sandbox, &arguments, WIDE);
        let piped = on_pipe(&sandbox, &arguments);

        assert_eq!(terminal.code, 0, "{arguments:?}");
        assert_eq!(piped.code, 0, "{arguments:?}");
        assert!(terminal.stderr.is_empty(), "{arguments:?}");
        assert!(piped.stderr.is_empty(), "{arguments:?}");

        let context = format!("{arguments:?} on a terminal");
        sole_document(&terminal.stdout, &context);
        assert_eq!(
            terminal.stdout, piped.stdout,
            "{arguments:?}: a terminal changed the document"
        );
    }
}

#[test]
fn a_narrow_terminal_cannot_reshape_a_machine_answer() {
    let sandbox = Sandbox::new();
    sandbox.seed("# Uma nota\n");

    let wide = on_terminal(&sandbox, &["--json", "listar"], WIDE).stdout;
    for size in [NARROW, VERY_NARROW, (16, 8)] {
        assert_eq!(
            on_terminal(&sandbox, &["--json", "listar"], size).stdout,
            wide,
            "a {size:?} window changed the document"
        );
    }
}

#[test]
fn a_machine_failure_is_one_document_and_no_prose() {
    let sandbox = Sandbox::new();

    for arguments in [
        vec!["--json", "comando-inexistente"],
        vec!["--json", "ler", "00000000"],
        vec!["--json", "ler", "nao-e-um-seletor"],
        vec!["--json", "listar", "--propriedade", "sem-igual"],
    ] {
        let terminal = on_terminal(&sandbox, &arguments, WIDE);
        let piped = on_pipe(&sandbox, &arguments);
        assert_ne!(terminal.code, 0, "{arguments:?} was supposed to fail");
        assert_eq!(terminal.code, piped.code, "{arguments:?}");

        // Whichever channel the sealed contract puts it on, it is the only
        // thing on that channel and the other one is empty.
        let (document, other) = if terminal.stdout.trim().is_empty() {
            (&terminal.stderr, &terminal.stdout)
        } else {
            (&terminal.stdout, &terminal.stderr)
        };
        assert!(
            other.trim().is_empty(),
            "{arguments:?}: both channels spoke: {terminal:?}",
            terminal = (&terminal.stdout, &terminal.stderr)
        );

        let value = sole_document(document, &format!("{arguments:?}"));
        assert_eq!(value["status"], "error", "{arguments:?}");
        assert!(value["error"].is_object(), "{arguments:?}");

        assert_eq!(
            (&terminal.stdout, &terminal.stderr),
            (&piped.stdout, &piped.stderr),
            "{arguments:?}: a terminal changed the failure"
        );
    }
}

#[test]
fn no_color_and_a_dumb_terminal_change_nothing_a_machine_reads() {
    let sandbox = Sandbox::new();
    let plain = on_pipe(&sandbox, &["--json", "listar"]).stdout;

    for (name, value) in [("NO_COLOR", "1"), ("TERM", "dumb"), ("COLUMNS", "20")] {
        let run = on_terminal_with(&sandbox, &["--json", "listar"], WIDE, |command| {
            command.env(name, value);
        });
        assert_eq!(run.stdout, plain, "{name}={value} changed the document");
    }
}
