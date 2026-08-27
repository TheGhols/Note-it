# Architecture Decision Records (ADRs)

## ADR-001: Separation of Native Shell and Web WYSIWYG Editor
- **Decision:** Use Rust + GTK4 + `gtk4-layer-shell` for native window lifecycle and WebKitGTK 6.0 embedding Tiptap/ProseMirror for the editor.
- **Rationale:** Native Wayland Layer Shell support is not available in Electron or standard Tauri without low-level C/Rust bridging. GTK4 and WebKitGTK 6.0 provide Wayland-native rendering with low memory overhead while Tiptap provides a rich, modular WYSIWYG editor engine.

## ADR-002: Individual Markdown Files for Note Persistence
- **Decision:** Store each post-it as a separate `.md` file with YAML front matter named by UUID.
- **Rationale:** Guarantees data ownership, portability, backup friendliness, and interoperability with other tools while avoiding single-point-of-failure database files.

## ADR-003: UI State Decoupling
- **Decision:** Store window coordinates, width, height, and display assignments in `$XDG_STATE_HOME/note-it/state.json`, not in the Markdown files.
- **Rationale:** Preserves Markdown cleanliness and portability across different screen setups.

## ADR-004: Official @tiptap/markdown & Tiptap 3 Ecosystem
- **Decision:** Use Tiptap 3 with the official `@tiptap/markdown` extension (all packages pinned to exact matching version `3.30.5`).
- **Rationale:** Third-party markdown extensions are deprecated and unmaintained. Tiptap 3's official markdown module provides built-in bidirectional tokenizers, stable AST handling, and extensible mark renderers for controlled HTML elements (`<u>`, `<mark>`, `<span>`).

## ADR-005: Collapse Reuses the Existing Geometry Pipeline
- **Decision:** Collapsing a note keeps `width`/`height` as the single source of truth for the live
  surface and records the previous size in `expanded_width`/`expanded_height`. The minimum height is
  relaxed to the header bar height only while `collapsed` is true, and resizing is disabled in that
  state.
- **Rationale:** A second, independent geometry system for collapsed notes would duplicate the
  clamping, persistence and multi-monitor logic stabilised in Phase 3.0R.1. Reusing one pipeline
  means a collapsed note is dragged, clamped and persisted by exactly the same code path as an
  expanded one, and expanding restores the recorded size at whatever position the bar was left.
  Resizing is disabled while collapsed because there is no coherent expanded geometry a vertical
  resize of a header bar could produce; the affordance is hidden rather than shown and ignored.
- **Note:** While the popover is open on a collapsed note the host lends the surface extra height so
  the menu is not clipped by a surface that is only a header bar tall. That height is presentation
  only — it is never written to `state.json`.

## ADR-006: GTK Compose-Table Warnings Are External and Left Alone
- **Decision:** Keep `GTK_IM_MODULE=simple` and do not suppress the
  `Gtk-WARNING **: Can't handle >16bit keyvals` / `Can't handle Unicode codepoint …` burst.
- **Rationale:** The warnings come from GTK itself — the strings exist only in `libgtk-4.so`, not in
  Note-it or WebKitGTK. `gtk_im_context_simple` parses the system X11 Compose file on first use and
  warns for the handful of entries whose keyvals or codepoints do not fit its 16-bit compose-table
  format (emoji compose sequences), then caches the parsed table in
  `$XDG_CACHE_HOME/gtk-4.0/compose/`. A stock GTK4 application with a focused text entry and a cold
  cache reproduces the identical burst with no Note-it code involved. The burst therefore appears
  once per cache generation, at startup only, and never during typing.
- **Impact:** None for pt-BR. Dead keys and accented characters are all BMP codepoints and are
  parsed normally; only the non-BMP entries are skipped. Removing `GTK_IM_MODULE=simple` would stop
  the warnings but regress dead-key composition on Niri, and a global log handler would hide real
  GTK warnings too.

## ADR-007: The Host Surface Carries the Note's Paper Colour
- **Decision:** Back every note window with a GTK stylesheet rule painting the paper colour and the
  same corner radius the page uses, keeping the WebView itself transparent. The class is swapped
  when the note's colour changes.
- **Rationale:** A WebView repaints asynchronously. When a fast resize grows the surface, the
  compositor presents the larger surface a frame before the page has painted it, and the strip that
  is not yet painted showed the default dark window background — the black band reported after
  Phase 3.1. Filling it from the host means the gap is already the right colour. Painting the
  background on the window rather than on the WebView keeps the rounded corners: an opaque WebView
  background would have squared them off.
- **Consequence:** The host needs its own copy of the palette. A test compares it against
  `ui/src/styles/theme.css` so the two cannot drift apart.

## ADR-008: Task Completion Timestamps Travel With Their Task
- **Decision:** Store a completed task's timestamp in an HTML comment appended to that task's own
  Markdown line: `- [x] Comprar material <!-- note-it:completed_at=2026-08-27T11:32:00-03:00 -->`.
- **Rationale:** Standard Markdown has no syntax for this. Keeping the main line as plain `- [x] …`
  leaves the note readable in any other tool, while the comment is invisible in rendered Markdown.
  Because the metadata sits on the task's own line it moves with the task when tasks are reordered,
  which a front-matter table keyed by task position could not do.
- **Audit:** The sanitizer stripped every HTML comment, and the Markdown lexer dropped them before
  Tiptap saw them. Both were extended narrowly: the sanitizer keeps this one comment form after
  validating the timestamp, and the task item's own Markdown hooks read it into a node attribute
  and strip it from the visible content.
- **Unknown dates stay unknown:** a task arriving already checked — loaded from Markdown, pasted, or
  restored by undo — is never given a timestamp, so `- [x]` written outside Note-it shows no date.

## ADR-009: Zoom Is a View Scale, Text Size Is Content
- **Decision:** Zoom scales the editor through the font size the content inherits, is stored as
  `zoom_percent` in `state.json`, and never touches the document. Text size is a separate inline
  mark that is part of the note's content.
- **Rationale:** They answer different questions — "make this note easier to read right now" versus
  "make this word big". Implementing either through the other would either write view preferences
  into the Markdown or make a formatting choice vanish when the window is reopened. A CSS transform
  was rejected for the zoom: it scales painted pixels while leaving the caret and pointer
  coordinates on the unscaled geometry, so the text cursor would drift away from the characters.
- **Consequence:** `Ctrl+=` / `Ctrl+-` now drive the zoom instead of the note's base font size. The
  base `font_size` in the front matter is still honoured when a note is loaded; it simply no longer
  has a keyboard binding.

## ADR-010: Summoning Goes Through the Command Line, Not the WebView
- **Decision:** A global summon is a compositor keybinding that spawns `note-it`, which reaches the
  running instance through the existing single-instance dispatcher. In-application shortcuts stay
  as they are, for when the note is already focused.
- **Rationale:** Shortcuts inside the note are ordinary key events in its WebView, and a Wayland
  client only receives key events while it holds keyboard focus. They can never fire while the
  browser is in front — no amount of work inside the application changes that. The compositor is
  the only component that sees the key, so the reliable path has to start there.
- **Layer handling:** a `bottom` surface is always below ordinary windows, so a note on the desktop
  cannot be shown over another application without moving it to `overlay`. Summoning elevates it
  but keeps the stored preference, so `note-it toggle`, `Ctrl+Shift+Space` and the next restart all
  still reflect what the user chose. `note-it show` remains the explicit, persisted mode change.
- **Not a summon:** launching the application honours the stored preference instead of pulling the
  note to the front, so starting Note-it on the desktop layer leaves it on the desktop.

## ADR-011: Closing a Note Must Leave a Way Back
- **Decision:** With every note closed, a summon reopens the most recently saved note instead of
  creating a blank one. A note is only created when none exist at all, or on `note-it new`.
- **Rationale:** The `×` button saves the note and records `is_open = false`, keeping the Markdown,
  the geometry and every other stored property. But startup only ever restored notes marked open,
  so once the last note was closed it became unreachable and the application answered with an empty
  note. Nothing was lost on disk; there was simply no route back to it.
- **Ordering:** recency comes from the note file's modification time, so no note has to be parsed
  to decide which one to reopen, and the order still reflects the last save.
- **Consequence:** restoring also records the notes as open again, so a reopened note is not left
  contradicting its own state file.

