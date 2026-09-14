//! Shared helpers for the integration tests.
//!
//! Rust compiles this module separately into every test binary that includes
//! it, so anything one binary does not use looks dead to that binary even
//! though another exercises it thoroughly. The allow below is about that, and
//! nothing else: every item here is used by at least one test.

#![allow(dead_code)]

use noteit_core::{
    authority::perform_at,
    coordination::WriteCoordinationPaths,
    write::{NoteDraft, WriteOperation},
    StorePaths, Uuid,
};
use std::{
    fs, io,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

pub fn wait(description: &str, mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !ready() {
        assert!(Instant::now() < deadline, "timeout: {description}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

pub fn store(root: &Path, runtime: &Path, body: &str) -> (StorePaths, Uuid) {
    let paths = StorePaths::from_custom_paths(
        root.join("data/note-it/notes"),
        root.join("config/note-it"),
        root.join("state/note-it"),
        runtime.join("note-it"),
    );
    let created = perform_at(
        &paths,
        &WriteOperation::CreateNote {
            draft: NoteDraft {
                content: body.into(),
                ..Default::default()
            },
        },
    )
    .unwrap();
    (paths, created.outcome.note_id)
}

pub fn cleanup_coordination(paths: &StorePaths) {
    let paths = WriteCoordinationPaths::for_store(paths).unwrap();
    if fs::read_to_string(paths.store_marker_path())
        .ok()
        .as_deref()
        .map(str::trim_end)
        == paths.notes_dir().to_str()
    {
        let _ = fs::remove_dir_all(paths.store_dir());
    }
}

pub struct Tui {
    master: OwnedFd,
    slave: OwnedFd,
    pub child: Child,
    pub output: Vec<u8>,
    cursor_queries: usize,
    original_termios: rustix::termios::Termios,
}

impl Tui {
    pub fn spawn(root: &Path, configure: impl FnOnce(&mut Command)) -> Self {
        Self::spawn_program(root, env!("CARGO_BIN_EXE_noteit-tui").as_ref(), configure)
    }

    pub fn spawn_program(
        root: &Path,
        program: &Path,
        configure: impl FnOnce(&mut Command),
    ) -> Self {
        Self::spawn_sized(root, program, 160, 32, configure)
    }

    /// The same pseudoterminal at a chosen size, so a narrow terminal is a
    /// dimension a test can ask for rather than a constant.
    pub fn spawn_sized(
        root: &Path,
        program: &Path,
        columns: u16,
        rows: u16,
        configure: impl FnOnce(&mut Command),
    ) -> Self {
        let mut master = 0;
        let mut slave = 0;
        let size = libc::winsize {
            ws_row: rows,
            ws_col: columns,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: all output pointers are valid; descriptors become owned once.
        assert_eq!(
            unsafe {
                libc::openpty(
                    &mut master,
                    &mut slave,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    &size,
                )
            },
            0
        );
        let master = unsafe { OwnedFd::from_raw_fd(master) };
        let slave = unsafe { OwnedFd::from_raw_fd(slave) };
        let original_termios = rustix::termios::tcgetattr(&slave).unwrap();
        assert_eq!(
            unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) },
            0
        );
        let mut command = Command::new(program);
        command
            .env("XDG_DATA_HOME", root.join("data"))
            .env("XDG_CONFIG_HOME", root.join("config"))
            .env("XDG_STATE_HOME", root.join("state"))
            .env("TMPDIR", root)
            .env_remove("DBUS_SESSION_BUS_ADDRESS")
            .env_remove("NOTEIT_TUI_PANIC_FOR_TEST")
            .stdin(Stdio::from(slave.try_clone().unwrap()))
            .stdout(Stdio::from(slave.try_clone().unwrap()))
            .stderr(Stdio::from(slave.try_clone().unwrap()));
        configure(&mut command);
        let child = command.spawn().unwrap();
        Self {
            master,
            slave,
            child,
            output: Vec::new(),
            cursor_queries: 0,
            original_termios,
        }
    }
    pub fn input(&self, bytes: &[u8]) {
        assert_eq!(
            unsafe { libc::write(self.master.as_raw_fd(), bytes.as_ptr().cast(), bytes.len()) },
            bytes.len() as isize
        );
    }
    // Cargo compiles `tests/support` separately into every test binary, so a
    // helper only two of them need is dead code in the third. That is a
    // property of the build, not of the harness.
    #[allow(dead_code)]
    /// Sends a signal to *this* child and nobody else.
    ///
    /// The PID comes from the process this harness itself spawned, so there is
    /// no pattern, no name and no way for the signal to reach a Note-it the
    /// person running the tests actually cares about.
    pub fn signal(&self, signal: libc::c_int) {
        assert_eq!(
            unsafe { libc::kill(self.child.id() as libc::pid_t, signal) },
            0,
            "kill({signal}) on pid {}",
            self.child.id()
        );
    }

    /// Waits for the child to leave on its own after a signal.
    #[allow(dead_code)]
    pub fn wait_exit(&mut self) {
        wait("TUI exit after signal", || {
            self.drain();
            self.child.try_wait().unwrap().is_some()
        });
        self.child.wait().unwrap();
    }

    pub fn cooked(&self) -> bool {
        let mut state = std::mem::MaybeUninit::<libc::termios>::uninit();
        assert_eq!(
            unsafe { libc::tcgetattr(self.slave.as_raw_fd(), state.as_mut_ptr()) },
            0
        );
        let flags = unsafe { state.assume_init() }.c_lflag;
        flags & (libc::ICANON | libc::ECHO) == libc::ICANON | libc::ECHO
    }
    pub fn assert_restored(&self) {
        let after = rustix::termios::tcgetattr(&self.slave).unwrap();
        // Rustix Termios has no PartialEq. Its complete Debug representation
        // includes all flag bits, line discipline, every control code and speeds.
        let before = format!("{:?}", self.original_termios);
        let after = format!("{after:?}");
        println!("TERMIOS_BEFORE={before}\nTERMIOS_AFTER={after}");
        assert_eq!(
            after, before,
            "the application must restore the original PTY"
        );
    }
    pub fn drain(&mut self) {
        let mut bytes = [0u8; 8192];
        loop {
            let count = unsafe {
                libc::read(
                    self.master.as_raw_fd(),
                    bytes.as_mut_ptr().cast(),
                    bytes.len(),
                )
            };
            if count > 0 {
                self.output.extend_from_slice(&bytes[..count as usize]);
            } else {
                if count < 0 {
                    assert!(matches!(
                        io::Error::last_os_error().raw_os_error(),
                        Some(libc::EAGAIN | libc::EIO)
                    ));
                }
                break;
            }
        }
        // A PTY supplies termios, not a terminal emulator. Answer ANSI DSR
        // requested by Ratatui's clear() after returning from the editor.
        let queries = self
            .output
            .windows(4)
            .filter(|bytes| *bytes == b"\x1b[6n")
            .count();
        while self.cursor_queries < queries {
            self.input(b"\x1b[1;1R");
            self.cursor_queries += 1;
        }
    }
    pub fn wait_text(&mut self, text: &str) {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            self.drain();
            if String::from_utf8_lossy(&self.output).contains(text) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "timeout {text}: {}",
                String::from_utf8_lossy(&self.output)
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    /// Walks from the list to the reader, which is where `$EDITOR` lives.
    ///
    /// Since Fase 5.0D.2 `Enter` opens the note in the native editor and `Esc`
    /// steps out of it, so reaching the reader takes both. Each step waits for
    /// something emitted *after* the key that should have caused it. Ratatui's
    /// differential renderer may rewrite a footer in disjoint ANSI fragments,
    /// so the durable reader title is observed only in bytes appended after
    /// `Esc`; an earlier title cannot answer the wait. Nothing is cleared here,
    /// so a caller counting escape sequences still counts every one the
    /// application emitted. The lone `Esc` is written by itself:
    /// bundled with the next byte it would be read as `Alt`.
    pub fn leave_native_editor(&mut self) {
        self.input(b"\r");
        self.wait_text("Edição:");
        let reader_transition = self.output.len();
        self.input(b"\x1b");
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            self.drain();
            if String::from_utf8_lossy(&self.output[reader_transition..]).contains("Leitura:") {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "timeout reader transition: {}",
                String::from_utf8_lossy(&self.output[reader_transition..])
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// The whole way from the list to a running `$EDITOR`.
    pub fn open_external_editor(&mut self) {
        self.leave_native_editor();
        self.input(b"e");
    }

    pub fn finish(&mut self) {
        self.input(b"q");
        wait("TUI exit", || {
            self.drain();
            self.child.try_wait().unwrap().is_some()
        });
        assert!(self.child.wait().unwrap().success());
        self.assert_restored();
        assert!(self.cooked());
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

// ---------------------------------------------------------------------------
// The screen harness (Fase 5.0D.R6 §8)
// ---------------------------------------------------------------------------
//
// R5 passed its suite and failed the person using it. The reason is written in
// the shape of the old tests: they proved a `SourceTransaction` was right and
// never looked at the screen. This harness exists so a regression can be
// stated the way the reader states it — "after Down the caret is on the row
// below" — by driving the real `App` through real key events and reading the
// real rendered buffer back.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use noteit_tui::app::{App, EditorMode, Focus};
use ratatui::{backend::TestBackend, style::Modifier, Terminal};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// One rendered frame, read back as text plus the cells drawn reversed.
pub struct Frame {
    pub width: u16,
    pub height: u16,
    pub rows: Vec<String>,
    /// The same buffer, one entry per terminal cell.
    ///
    /// `rows` is unusable as an index: a wide grapheme occupies two cells and
    /// a cluster like `👨‍👩‍👧‍👦` is many `char`s in one, so the nth `char` of a row
    /// is not the nth column of the screen. Anything asking "what is *at* this
    /// column" has to ask here.
    pub cells: Vec<Vec<String>>,
    /// Every reversed cell as `(column, row, symbol)`. Nothing but the caret is
    /// reversed when there is no selection, so this *is* the caret.
    pub carets: Vec<(u16, u16, String)>,
}

impl Frame {
    /// The whole screen as one string, rows separated by newlines.
    pub fn text(&self) -> String {
        self.rows.join("\n")
    }

    /// The one caret, or a failure naming how many there were instead.
    pub fn caret(&self) -> (u16, u16) {
        assert_eq!(
            self.carets.len(),
            1,
            "exactly one caret must be drawn, found {:?} in:\n{}",
            self.carets,
            self.text()
        );
        (self.carets[0].0, self.carets[0].1)
    }

    /// The row the caret is on, as text.
    pub fn caret_row(&self) -> String {
        let (_, row) = self.caret();
        self.rows[row as usize].clone()
    }

    /// Fails when any of `needles` appears anywhere on screen.
    pub fn assert_hides(&self, needles: &[&str], context: &str) {
        let text = self.text();
        for needle in needles {
            assert!(
                !text.contains(needle),
                "{context}: `{needle}` must never be visible in the visual editor:\n{text}"
            );
        }
    }
}

/// Canonical markup the visual editor must never put in front of a reader.
pub const CANONICAL_MARKUP: &[&str] = &[
    "<span",
    "</span",
    "<mark",
    "</mark",
    "data-note-it-",
    "style=\"",
];

/// A real `App`, a real store and a real terminal buffer.
pub struct Screen {
    pub app: App,
    pub width: u16,
    pub height: u16,
    root: tempfile::TempDir,
}

impl Screen {
    /// A note with `content`, open in the editor at 80x24.
    pub fn open(content: &str) -> Self {
        Self::open_sized(content, 80, 24)
    }

    pub fn open_sized(content: &str, width: u16, height: u16) -> Self {
        let root = tempfile::tempdir().unwrap();
        let runtime = root.path().join("runtime");
        let (paths, _) = store(root.path(), &runtime, content);
        let mut app = App::new_at(paths, Arc::new(AtomicBool::new(false)));
        app.handle_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(app.focus, Focus::Editor, "Enter opens the editor");
        let mut screen = Self {
            app,
            width,
            height,
            root,
        };
        // A frame before the first key: the viewport is the renderer's, and a
        // test that never drew has no viewport at all.
        screen.frame();
        screen
    }

    pub fn store_root(&self) -> &std::path::Path {
        self.root.path()
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        self.frame();
    }

    /// Renders at the current size and reads the buffer back.
    pub fn frame(&mut self) -> Frame {
        let mut terminal = Terminal::new(TestBackend::new(self.width, self.height)).unwrap();
        self.app.draw(&mut terminal).unwrap();
        let buffer = terminal.backend().buffer().clone();
        let mut rows = Vec::with_capacity(self.height as usize);
        let mut cells = Vec::with_capacity(self.height as usize);
        let mut carets = Vec::new();
        for row in 0..self.height {
            let mut text = String::new();
            let mut line = Vec::with_capacity(self.width as usize);
            for column in 0..self.width {
                let cell = &buffer[(column, row)];
                text.push_str(cell.symbol());
                line.push(cell.symbol().to_owned());
                if cell.modifier.contains(Modifier::REVERSED) {
                    carets.push((column, row, cell.symbol().to_owned()));
                }
            }
            rows.push(text);
            cells.push(line);
        }
        Frame {
            width: self.width,
            height: self.height,
            rows,
            cells,
            carets,
        }
    }

    /// A key, then the frame it produced. Drawing after every key is the whole
    /// point: a viewport only moves while something is being drawn.
    pub fn press(&mut self, key: KeyEvent) -> Frame {
        self.app.handle_key(key);
        self.frame()
    }

    pub fn key(&mut self, code: KeyCode) -> Frame {
        self.press(KeyEvent::from(code))
    }

    pub fn shift(&mut self, code: KeyCode) -> Frame {
        self.press(KeyEvent::new(code, KeyModifiers::SHIFT))
    }

    pub fn alt(&mut self, character: char) -> Frame {
        self.press(KeyEvent::new(KeyCode::Char(character), KeyModifiers::ALT))
    }

    pub fn ctrl(&mut self, character: char) -> Frame {
        self.press(KeyEvent::new(
            KeyCode::Char(character),
            KeyModifiers::CONTROL,
        ))
    }

    /// Types `text` one character at a time, drawing after each, and returns
    /// every frame so a test can assert on all of them rather than the last.
    pub fn type_text(&mut self, text: &str) -> Vec<Frame> {
        text.chars()
            .map(|character| self.press(KeyEvent::from(KeyCode::Char(character))))
            .collect()
    }

    pub fn mode(&self) -> EditorMode {
        self.app.editor_mode
    }

    /// The draft's text, which is the canonical source.
    pub fn source(&self) -> String {
        self.app.draft.as_ref().map(|draft| draft.text()).unwrap()
    }

    pub fn visual_offset(&self) -> Option<usize> {
        self.app.visual_cursor.map(|cursor| cursor.offset.get())
    }

    /// One line of trace, for the reproduction report of §7.
    pub fn trace(&self, label: &str) -> String {
        let cursor = self.app.draft.as_ref().map(|draft| draft.cursor());
        format!(
            "{label:<26} mode={:?} draft=({},{}) visual={:?} scroll={} vscroll={:?} column={} gen={:?}",
            self.app.editor_mode,
            cursor.map(|c| c.line).unwrap_or(0),
            cursor.map(|c| c.column).unwrap_or(0),
            self.visual_offset(),
            self.app.editor_scroll(),
            self.app.visual_scroll(),
            self.app.editor_column(),
            self.app.draft.as_ref().map(|draft| draft.generation()),
        )
    }
}

// ---------------------------------------------------------------------------
// Reading a real terminal back (Fase 5.0D.R6 §24/§25)
// ---------------------------------------------------------------------------

/// Replays a stream of terminal output into the screen it would produce.
///
/// A PTY test that greps the raw bytes is asserting about Ratatui's diffing,
/// not about what a person sees: the renderer rewrites only the cells that
/// changed, so a title can reach the terminal as `Cor: Vermelh`, a cursor
/// jump, `Ma`, another jump and `ca: Amarelo`. Every one of those cells is on
/// screen and no substring search finds them.
///
/// This understands exactly the sequences a full-screen TUI emits — absolute
/// cursor positioning, the two erases, and carriage returns — and ignores
/// every other escape, which is enough to reconstruct the frame and not enough
/// to pretend to be a terminal emulator.
pub fn replay_screen(output: &str, columns: usize, rows: usize) -> Vec<String> {
    let mut grid = vec![vec![' '; columns]; rows];
    let (mut row, mut column) = (0usize, 0usize);
    let mut chars = output.chars().peekable();

    while let Some(character) = chars.next() {
        match character {
            '\u{1b}' => {
                if chars.peek() != Some(&'[') {
                    // A two-character escape; its second byte is not text.
                    chars.next();
                    continue;
                }
                chars.next();
                let mut params = String::new();
                let mut final_byte = None;
                for candidate in chars.by_ref() {
                    if candidate.is_ascii_alphabetic() {
                        final_byte = Some(candidate);
                        break;
                    }
                    params.push(candidate);
                }
                let numbers: Vec<usize> = params
                    .trim_start_matches(['?', '<', '>'])
                    .split(';')
                    .map(|value| value.parse().unwrap_or(0))
                    .collect();
                match final_byte {
                    Some('H') | Some('f') if !params.starts_with('?') => {
                        row = numbers.first().copied().unwrap_or(1).saturating_sub(1);
                        column = numbers.get(1).copied().unwrap_or(1).saturating_sub(1);
                    }
                    Some('J') if !params.starts_with('?') => {
                        grid = vec![vec![' '; columns]; rows];
                        row = 0;
                        column = 0;
                    }
                    Some('K') if !params.starts_with('?') => {
                        if let Some(line) = grid.get_mut(row) {
                            for cell in line.iter_mut().skip(column) {
                                *cell = ' ';
                            }
                        }
                    }
                    _ => {}
                }
            }
            '\r' => column = 0,
            '\n' => {
                row += 1;
                column = 0;
            }
            printable if !printable.is_control() => {
                if let Some(cell) = grid.get_mut(row).and_then(|line| line.get_mut(column)) {
                    *cell = printable;
                }
                column += 1;
            }
            _ => {}
        }
    }

    grid.into_iter()
        .map(|line| line.into_iter().collect())
        .collect()
}

impl Screen {
    /// Everything §9 asks to be true after *every* key, checked at once.
    ///
    /// Stated as properties of what is observable rather than of the model,
    /// because the model was right in R5 and the screen was not.
    pub fn assert_caret_is_sane(&mut self, label: &str) {
        let scroll_before = self.app.editor_scroll();
        let frame = self.frame();

        // 1 and 3: exactly one caret, and it was drawn, so it is inside the
        // viewport by construction.
        let (column, row) = frame.caret();

        // 4: the row it landed on is a row of this frame.
        assert!(
            (row as usize) < frame.rows.len(),
            "{label}: caret row {row} is off the frame"
        );
        assert!(
            (column as usize) < frame.width as usize,
            "{label}: caret column {column} is off the frame"
        );

        if self.app.editor_mode != EditorMode::Visual {
            return;
        }

        let document = self.app.visual_document().expect("a projection");
        let draft = self.app.draft.as_ref().expect("a draft");

        // 8: the projection the caret is measured against is this text's.
        assert_eq!(
            document.generation(),
            draft.generation(),
            "{label}: the projection is a generation behind the draft"
        );

        // 2: the caret is a legal position — a real slot, or the empty
        // paragraph at the end of the note, which no block claims.
        let offset = self.visual_offset().expect("a visual caret");
        let legal = noteit_tui::source_map::SourceOffset::in_source(document.source(), offset)
            .is_some_and(|offset| document.slot_at_offset(offset).is_some())
            || document.is_open_tail(offset);
        assert!(
            legal,
            "{label}: caret at {offset} is not a legal slot:\n{}",
            frame.text()
        );

        // 7: the Markdown viewport is not being moved by the visual editor,
        // and the visual viewport is not being moved by the raw cursor.
        assert_eq!(
            self.app.editor_scroll(),
            scroll_before,
            "{label}: drawing the visual editor moved the Markdown viewport"
        );
    }

    /// Types one character and proves it landed at the caret.
    ///
    /// "At the caret" is the whole property R6-001 was about: the character
    /// appears in the cell the caret was in, and the caret moves on by one
    /// place in reading order — the next column, or the start of the next row
    /// when the one it was on had no room left.
    pub fn assert_types_at_caret(&mut self, character: char, label: &str) {
        let (before_column, before_row) = self.frame().caret();
        let after = self.press(KeyEvent::from(KeyCode::Char(character)));
        let (column, row) = after.caret();

        assert_eq!(
            after.cells[before_row as usize][before_column as usize],
            character.to_string(),
            "{label}: `{character}` is not where the caret was:\n{}",
            after.text()
        );

        let advanced = (row == before_row && column == before_column + 1)
            || (row == before_row + 1 && column <= before_column);
        assert!(
            advanced,
            "{label}: the caret went from {:?} to {:?}, which is not one place on:\n{}",
            (before_column, before_row),
            (column, row),
            after.text()
        );
    }
}
