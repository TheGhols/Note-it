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

## ADR-012: A Collapsed Note Expands Before Its Menu Opens
- **Decision:** Clicking a collapsed note expands it. The `☰` button expands the note and then opens
  the menu, in one click. The temporary surface-growing mechanism added for the collapsed menu was
  removed.
- **Rationale:** The settings popover was being clipped on a collapsed note. It is not a CSS
  problem: a collapsed note's Wayland surface is only the header bar tall, and nothing can paint
  outside a surface, so `overflow` and `z-index` are irrelevant. Phase 3.1 worked around it by
  lending the surface 120px while the menu was open, which was enough for a menu of two entries.
  Phase 3.2 grew the menu to seven entries — about 234px — and the workaround silently stopped
  covering it.
- **Why not simply lend more height:** the number would have to be re-tuned every time the menu
  changes, and a bar that balloons into a tall rectangle to show a menu is a strange thing to look
  at. Expanding the note is what the user wants anyway, needs no magic number, and reuses the
  collapse path that already exists.
- **Consequence:** the `menu_overlay` message and its height constant are gone, leaving one way for
  a note to change size.

## ADR-013: Highlighted Text Carries Its Own Foreground
- **Decision:** `.ProseMirror mark` sets a dark foreground for highlighted text, on every paper
  colour. An explicit text colour is an inline style and still wins.
- **Rationale:** On the dark paper the default text is light, and every highlight in the palette is
  pale, so highlighted text was light-on-pale and barely readable. Fixing it in the stylesheet keeps
  it a rendering concern: nothing is written into the Markdown, so a note does not gain a colour
  mark it never had just because of the paper it sits on, and it round-trips unchanged.
- **Palette:** rather than deciding at runtime whether a user's colour is "still legible" and
  overriding it, the palette itself was made safe — orange, yellow and green were darkened so every
  text colour clears a readable contrast on every highlight and on every paper colour. The user's
  intent is then always preserved, because no combination in the palette is unreadable.

## ADR-014: The Highlight Mark Paints Its Own Foreground
- **Decision:** `NoteItHighlight` overrides the `color` attribute's `renderHTML` to emit
  `background-color: <highlight>; color: #1E293B`, and the stylesheet no longer tries to colour
  highlighted text.
- **Root cause it fixes:** the upstream Highlight extension renders
  `style="background-color: X; color: inherit"`. That `color: inherit` is an **inline style**, so it
  beats any stylesheet rule — including the `.ProseMirror mark { color: … }` added in Phase 3.3.
  Highlighted text therefore kept inheriting the paper's colour, which on the dark paper is white
  on a pale highlight. The Phase 3.3 fix never applied; only its contrast arithmetic was tested, and
  arithmetic about a palette proves nothing about what the DOM actually paints.
- **Testing:** the tests now assert the colour the element really resolves to via
  `getComputedStyle`, and that no `inherit` is left on the mark, rather than computing contrast
  ratios in isolation.
- **Explicit text colour:** ProseMirror nests the highlight inside the colour span, so the mark's
  inline foreground wins while the highlight is present — legibility is preserved — and the user's
  colour is still recorded in the Markdown, reappearing as soon as the highlight is removed.
  Nothing about the paper colour is ever written to the document.

## ADR-015: Paper Is a Note Property, the Theme Is an Application Property
- **Decision:** `paper_type` and `paper_intensity` live in the note's YAML front matter beside
  `color`; the interface `theme` lives in `config.toml`.
- **Rationale:** the paper is what a note *is* — it belongs to the note and travels with the file,
  exactly as its colour already did, and it goes through the same save path, which never touches
  `updated_at`. The theme is what the *application* looks like: one preference, shared by every
  note, so it belongs with the other global preferences rather than being copied into every file.
- **Not in the Markdown body:** nothing about the paper is written into the document. No wrapper
  element, no class, no decoration — the body round-trips byte for byte through every paper type
  and intensity.
- **Strings, not serde enums:** both fields are stored as plain strings and resolved against the
  supported set on read. A serde enum would fail the whole parse on a value written by a newer
  version or by hand, costing the user the note; resolving to the default costs them a pattern.
- **Retro-compatibility:** a note written before this phase carries neither field and opens as
  plain paper at normal intensity. `paper_intensity` is kept even for `blank`, so switching paper
  back and forth never silently discards the choice.

## ADR-016: One Parameterised Paper Pattern, Composed Where It Is Painted
- **Decision:** the five papers are one CSS system, not five implementations. The type selects a
  pattern and `--paper-pattern-spacing`, the intensity selects `--paper-pattern-alpha`, and the
  paper colour selects `--paper-pattern-ink` and `--paper-pattern-gain`. Both grids are the same
  rule at two spacings.
- **Where the colour is composed:** `--paper-pattern-color` is declared on `.editor-wrapper`, the
  element that paints it — deliberately, and not on `:root`.
- **The defect that forced it:** `var()` is substituted where the declaration sits, using that
  element's own values. Composing the colour on `:root` froze the root's ink and opacity into it,
  so the per-paper and per-intensity overrides on `body` never reached the paint: every intensity
  rendered at "normal", and the dark paper was drawn with the *pale* papers' dark ink, which is
  invisible on `#18181B`. Measuring the real WebView caught it — the black paper's rules came out
  at `#17181D` against `#18181B` paper. Declaring it on the consumer lets the three inputs inherit
  down with the note's real choices first.
- **Contrast:** the dark paper carries a gain of `0.72` rather than a boost. Measuring perceptual
  lightness rather than assuming showed the opposite of the intuition: a near-black paper sits on
  the steep part of the lightness curve, so the same alpha lifts it *further* than it darkens a
  pale paper. The gain pulls all three intensities onto the strength they have everywhere else.
- **Zoom:** spacing is in pixels and never references `--note-zoom` or `--note-font-size`, so the
  content scales and the background stays put. Verified in the WebView: ruled paper measured
  exactly 24px between lines at both 75% and 200%.
- **Where it is painted:** on the scrolling surface with `background-attachment: local`, so it
  travels with the text, while `#app` keeps its flat colour fill underneath — a fast resize can
  expose paper but never an unpainted strip. Hiding that surface on collapse takes the pattern
  with it, leaving the bar as a clean band of the note's colour, with no extra code.

## ADR-017: The Theme Dresses the Chrome, Never the Paper
- **Decision:** a `--ui-*` token set (`surface`, `surface-hover`, `text`, `text-muted`, `border`,
  `shadow`, `focus-ring`) dresses menus, popovers and focus states. The `--paper-*` tokens keep
  dressing everything drawn on the paper. The light palette is defined on bare `:root`, and only
  the same tokens are redefined under `:root[data-theme="dark"]`.
- **Rationale:** the popover used to take `--popover-bg` from the *paper*, and its foreground from
  `--paper-text`. That could not survive a theme: a dark popover over a yellow note would inherit
  that paper's dark text and be unreadable. Splitting the two means the menu is legible over a
  black note and a yellow one alike, in either theme, and a note still keeps its own colour.
- **What is deliberately left on the paper:** the header buttons, the resize handle, the editor's
  scrollbar and everything inside `.ProseMirror`. They sit on the paper, so they follow it.
- **Phase 3.3R is untouched:** highlighted text still carries its own dark foreground inline, which
  beats both token sets, so it stays readable on every paper under either theme.
- **System preference:** resolved in the page with `matchMedia('(prefers-color-scheme: dark)')`,
  watched live so the desktop switching scheme reaches an open note. `matchMedia` is treated as
  optional throughout — a WebView that reports no colour scheme resolves `Sistema` to the light
  theme rather than ending up with no theme at all.
