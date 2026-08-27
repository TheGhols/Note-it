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

