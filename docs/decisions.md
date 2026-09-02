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
  to decide which one to reopen, and the order still reflects the last save. *(Superseded by
  ADR-027.1: the ordering key is now the note's own `updated_at`, with `mtime` as the fallback,
  because an appearance change rewrites the file without being an edit.)*
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
  exactly 24px between lines at both 75% and 300%.
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

## ADR-018: `updated_at` Is Compared, Not Assumed
- **Decision:** `save_content` compares the incoming text with the content already held before
  recording anything. Identical content updates nothing, writes nothing, and still returns `Ok`.
- **The defect:** every path that carries content back from the page — autosave, the flush before
  hide and quit, and save-and-close — funnelled into `save_content`, which assigned the content and
  called `touch_content_modified()` unconditionally. All three routinely arrive with content that
  has not changed: closing and flushing send whatever the editor holds whether or not it was
  touched, and autosave can fire on an edit that serialises back to the same Markdown. So merely
  opening a note and closing it moved its modification date, which contradicted the contract
  `docs/storage.md` already stated. Measured on the previous release: an untouched note went from
  `15:31:25` to `15:31:35` across one open/quit cycle.
- **Where the fix lives:** in that single funnel, not in each caller. The three callers do not need
  to agree on what counts as an edit, and no second dirty-tracking mechanism was introduced — the
  document's own `content` field already *is* the last-persisted text.
- **Why it still returns `Ok`:** save-and-close waits on this result before finalising the close,
  and the hide and quit flushes wait on it before destroying surfaces or exiting. "Nothing changed"
  must never become "nothing answered", or an untouched note would refuse to close.
- **No write at all:** when the content matches, the file is left alone entirely — no temp file, no
  rename, no fsync. Metadata-only changes (paper colour, type, intensity, font size) take their own
  direct save path and are unaffected, so nothing that must be persisted is skipped.
- **Consequence for recency:** the file's `mtime` decides which note a summon brings back when
  every note is closed. It now tracks the last real edit rather than the last close. That is the
  better reading of "the note used last", and it is covered by a test rather than left to chance.
  *(Superseded by ADR-027.1: `mtime` was only a proxy for it, and an appearance change moved the
  proxy without being an edit. The ordering now reads `updated_at` directly.)*
  Introducing a `last_active_note` in `state.json` was deliberately not done here: nothing approved
  depends on the old meaning, and inventing state for it would have been a larger change than the
  defect warranted.

## ADR-019: A Document Is Adopted Only After It Is Written
- **Decision:** every change to a note — the content arriving from the page, and the paper colour,
  paper type, pattern intensity and font size arriving from its menu — is prepared on a *copy* of
  the `NoteDocument`. `save_note_atomic` runs against that copy, and only a successful write makes
  it the document held in memory. A failure leaves the in-memory note exactly as it was.
- **The defect this closes:** ADR-018 rests on one premise — "the document's own `content` field
  already *is* the last-persisted text" — and `save_content` broke that premise itself. It assigned
  the content and stamped `updated_at` *before* calling `save_note_atomic`, so a failed write left
  memory holding B while the file still held A. The identical-content shortcut then compared the
  next payload against B: autosave, both flushes and save-and-close all resend whatever the editor
  holds, so the same B arrived again, matched, and returned `Ok` without writing anything.
  Save-and-close waits on exactly that result, so the note could close over an edit that never
  reached the disk. The optimisation did not cause the divergence, but it turned it from a
  transient inconsistency into silent content loss.
- **Why transactional rather than a dirty flag:** a second piece of state tracking "what was last
  persisted" would have to be kept in step with the document by hand, at every one of the four
  write paths, and getting *that* wrong reproduces the same class of defect one level up. Preparing
  a candidate and swapping it in on success needs no new state at all: the document *is* the record
  of what is on disk, which is what ADR-018 already assumed and now actually holds.
- **Appearance saves too:** paper colour, paper type, intensity and font size mutate the very
  document the content comparison is made against, so they take the same route through
  `save_metadata`. A colour that could not be written is not left in memory as though it had been,
  and choosing it again writes it. They still do not touch `updated_at`; appearance is not content.
- **What ADR-018 keeps:** identical, already-persisted content still writes nothing and still
  returns `Ok`, `updated_at` still moves only on a real edit, `created_at` is still immutable, and
  close and flush still succeed when there is genuinely nothing pending. Only a payload that
  coincides with a *failed* write is now treated as pending, because it is.
- **The editor's copy is not at stake.** The page owns the live text and resends it on every
  autosave, flush and close; the `NoteDocument` is the record of the file. Two earlier tests
  asserted the opposite — that a failed save leaves the latest text in memory — and nothing ever
  read it back: no path recovers content from that field, `save_now` only re-persists, and the
  `LoadNote` sent on a page reload should describe the stored note anyway. That expectation was the
  hazard, so it was replaced rather than preserved.
- **New-note creation is already safe:** `create_new_note` writes the document before any window
  exists and returns on failure, so there is no in-memory note left claiming to be stored.
- **A failed save cleans up after itself:** the temp file is removed when anything up to and
  including the rename fails. Nothing else ever collected one, so a run of failures used to leave
  `.tmp.*` debris in the notes directory permanently.
- **Testing I/O failure without touching the store:** the notes directory is moved aside and a
  plain file put in its place, so the kernel refuses every create and rename underneath it with
  `ENOTDIR`. That is path resolution rather than a permission bit, so it also fails for root, which
  is how the Rust CI job runs — a `chmod` would have silently passed there. The notes wait
  untouched in the directory that was moved aside, which is what lets the tests assert that the
  stored note survived the failed save unchanged.

## ADR-020: The Rename Is the Commit Point
- **Decision:** `save_note_atomic` reports failure for anything that happens **before or at** the
  rename, and success from the rename onwards. Syncing the notes directory comes after the commit
  point, so a failure there is reported as a durability warning on stderr and the save still
  returns `Ok`.
- **The defect this closes:** ADR-019 has the caller adopt a document only when the save returns
  `Ok`, which is right for every failure that leaves the file alone. The directory sync is not one
  of those. It runs *after* `rename` has already replaced the target, and it was inside the same
  `?` chain, so its failure was reported as a failed save. The caller then kept the old document
  while the file held the new one — memory and disk describing opposite versions, which is exactly
  the divergence ADR-019 exists to prevent, mirrored. A save-and-close would also have refused to
  close a note that really had been written.
- **Why the rename:** it is the moment the change becomes visible. Every reader from then on gets
  the new note, and nothing later in the function can put the old one back. Any report other than
  "saved" would be false, and acting on it means describing a file that no longer exists that way.
- **What the directory sync actually buys:** the note's *bytes* are already on stable storage — the
  temp file is `fsync`ed before the rename. Syncing the directory is what makes the *rename* itself
  survive a power loss. Without it, a crash at the wrong moment can leave the name still pointing
  at the previous note. That is a lost update, never a torn or corrupted file: a reader sees the
  old note or the new one, never half of either.
- **Why no pending-durability state:** an `fsync` on a directory flushes every pending entry in it,
  not just the most recent, so the next successful save of *any* note in the notes directory makes
  the earlier rename durable too. There is nothing to remember, nothing to retry by hand, and the
  identical-content shortcut has nothing to mask: after a committed-but-unsynced save the note on
  disk really is the new one, so resending it really is a no-op. Tracking a missed sync would be
  state that heals itself, which is the kind of bookkeeping ADR-019 refused for the same reason.
- **What is deliberately not claimed:** there is no retry of the sync, no guarantee that a save is
  durable when the sync fails, and no `fsync` of the note file after the rename. The contract is
  that a note is never half-written and never silently reverts *within a running system*; the
  durability window is documented in `docs/storage.md` rather than papered over.
- **Testing past the commit point:** once `rename` has returned there is nothing a test can do to
  the filesystem that reaches back into the sync that follows it, so this one failure is injected
  in-process by a `#[cfg(test)]` handle whose directory sync always fails. It is compiled out of
  every real build, and it drives the real `save_note_atomic` and the real `save_content`, so what
  the tests check is the production path and not a reimplementation of it. The pre-commit failures
  keep their real `ENOTDIR` injection, which reaches the syscalls themselves.

## ADR-021: Four Blocks, Three Shapes, No Block Framework
- **Decision:** the code block, the callout, the blockquote and the comment were each built as the
  smallest thing that could carry them, and no shared block architecture was extracted.
  - a **code block** is upstream's `CodeBlock` with `lowlight` on top and one method overridden;
  - a **callout** is the existing `Blockquote` with one attribute;
  - a **comment** is a new node, because nothing already in the schema is a block of literal text
    that is not part of the document's prose.
- **Rationale:** the roadmap allowed a reusable block architecture "where the shape of these
  features justifies one", and it does not. They share a menu section and nothing else: different
  content models (`text*` versus `block+`), different Markdown syntax (a fence, a quote prefix, an
  HTML comment), different parse rules, different escaping. A common base would have been an empty
  interface with four unrelated implementations behind it, which is a layer to read through rather
  than a layer that carries weight.
- **The callout is an attribute, not a node.** That one decision pays for most of the phase. A
  callout inherits the blockquote's content model, so several paragraphs, lists and nested blocks
  work without being designed for; it inherits the `>` prefixing, so serialization is the parent's
  output with one line in front; it inherits the commands and input rules. And the failure mode is
  free: an unrecognised `[!KIND]` produces no attribute, which *is* a plain blockquote with the
  marker still in its text. A separate `callout` node would have needed all of that written twice
  and a rule for what to do when the kind is unknown.
- **Highlighting is decoration and only decoration.** `lowlight` paints ProseMirror decorations over
  the same characters, so the file stays a plain fence. Sixteen grammars are imported by name rather
  than the `highlight.js` bundle, which carries nearly two hundred: the whole phase costs about
  30 kB gzipped.
- **Never guess a language.** Upstream falls back to `highlightAuto` for a block with no language or
  one it cannot resolve; both are replaced with a `highlightAuto` that returns nothing. A fence
  written without a language is plain on purpose, and colouring an unknown one as whatever it most
  resembles tells the reader something the note does not say.
- **The language identifier is never rewritten.** Not normalised, not defaulted, not dropped. An
  alias stays an alias and an unknown language keeps its spelling, because the note is the file and
  the file said what it said. Aliases are resolved for highlighting and for the menu label only, and
  the alias table is read from the grammars themselves rather than written by hand.
- **Comments became content.** The sanitizer used to drop every comment except Note-it's own task
  metadata, so a note holding one lost it on the first save. A comment is inert data, never
  executable markup, and it is now kept. Two tests asserted the old behaviour and were replaced.
  An unterminated `<!--` is escaped rather than swallowing the rest of the file, which is the same
  rule the rest of the sanitizer follows: degrade to text, never delete.
- **A comment is visible-but-not-content.** In a WYSIWYG editor a hidden comment is a comment nobody
  can edit or remove, and a file holding something the window never shows loses things quietly. It
  is drawn as a small labelled block instead, set apart from the prose, and serialized as
  `<!-- ... -->`. A `-->` inside is written escaped, because the literal sequence would close the
  comment early and spill the note out of it.
- **No new surface for arbitrary HTML.** Every label is a constant on the element and every kind
  comes from a five-value whitelist, so no note content reaches an attribute, a class or a style.
  Code block content is text in a node declaring itself as code; comment content is text in a node
  that takes no marks.
- **One menu, one section.** The four live under **Blocos** in the popover that already exists,
  built from the same panel and row helpers as every other section, and the rows reflect what the
  cursor is in rather than offering a fixed list. No shortcuts were added: the useful chords are
  taken, and typing the Markdown still works.

## ADR-022: One Atomic Write, One Commit Point, for Every File Note-it Stores

Phase 3.4R.2 established that the rename is the commit point for a note: a save
reports failure for anything up to and including it, and success from it
onwards, because after the rename the file on disk *is* the new content and no
caller may believe otherwise. `state.json` and `config.toml` never got that
rule, and they had drifted apart from it in opposite directions.

`state.json` was written atomically but propagated a post-rename directory-sync
failure as a failed save. Every caller treats that as "nothing was written":
closing a note rolled its state back in memory and left the window open, and
hiding refused to close the windows — while the file already held the new
state. Memory and disk then described different applications.

`config.toml` was not written atomically at all. It went straight over the real
file with a truncating open, so an interrupted write left a half-written
configuration; loading falls back to the defaults without a word, which turns a
partial write into a silent reset of the theme and every other preference.

Three copies of a subtle rule is how it drifted, so there is now one:
`atomic_file::write_atomic` holds the rule and its explanation, and notes,
window state and configuration all go through it. Creating the parent directory
is left to the caller — the store's directories are made once at startup, and a
notes directory that has since vanished is a fault to report rather than one to
paper over.

## ADR-023: The Page Is the Window's Focus Widget

A note is a `gtk4-layer-shell` window holding one WebView. Such a window is
mapped with no focus widget at all: the window can be active, with the
compositor sending it keys, while GDK has nowhere to deliver them and drops
them before WebKit. Every shortcut inside a note was therefore dead until a
click happened to focus the WebView as a side effect.

Focus is not something to grab once at startup. The window loses and regains
keyboard focus over its lifetime — a click, a layer change, a summon — so the
WebView is focused whenever the window *becomes* active. That covers the first
map and every re-map with one rule, and it grabs nothing while the note is not
the surface the compositor is talking to.

What this does not do, and cannot, is give keys to a surface the compositor is
not sending them to. A note on the `bottom` layer is behind every window and is
granted focus only when it is clicked; if it is covered there is nothing to
click. The authoritative `Ctrl+Shift+Space` therefore belongs to Niri and calls
the running application's `toggle-layer` GAction. The WebView chord remains a
local fallback. See `docs/niri.md`.

Measurements on Niri 26.04 and layer-shell protocol version 4 also corrected a
separate assumption: setting `Bottom`/`Overlay` does not inherently remap the
surface. The old `present()`/visibility fast path was application behaviour,
not a protocol requirement. Note-it now deliberately remaps only an occluded
Desktop-to-Overlay promotion to force the pending Wayland commit, maps it with
keyboard interactivity disabled to retain normal-window focus, and restores
click-to-focus after the compositor has observed the map. The reverse direction
uses the live protocol change without presentation.

## ADR-024: A Calculated Result Is a Decoration, and the Parser Has No Evaluator

Two decisions carry Phase 3.6, and neither is about arithmetic.

**A result is never content.** Every value the engine produces is a ProseMirror
widget decoration — the same mechanism that paints syntax highlighting over a
code fence. Writing results into the document would have been simpler to build
and wrong in five separate ways at once: the `.md` would gain numbers nobody
typed; `updated_at` would move because something was *recalculated* rather than
edited, undoing everything Phase 3.4R established; opening a note would be an
edit; a stale result would be saved over a note edited elsewhere; and the file
would stop being portable Markdown. As a decoration the note on disk is the note
that was written, reopening recomputes from the text, and there is nothing to go
stale. It also means undo and redo needed no work at all: results are not steps,
so one edit is one undo and the results follow whatever the document becomes.

**The parser has no evaluator behind it.** A lexer that knows ten token shapes,
a recursive-descent parser producing six node kinds, and a walk over that tree.
No `eval`, no `Function`, no property access, no call syntax, no host object.
`= window.location` is not a filtered input — it is unspellable, and stops at
the `.`. Variables live in a `Map` rather than an object, which is a security
property and not a style choice: an object would answer `constructor`,
`__proto__` and `toString` with real JavaScript values. Nothing was added to
`package.json`; a general expression library would have been larger than the
grammar and would have brought capabilities this note format has no use for.
The engine costs about 2.5 kB gzipped.

**Explicit syntax, because the alternative is a guessing machine.** A
calculation starts with `=` and a declaration uses `:=`. Without a marker the
engine would spend its life deciding which numbers in a note are arithmetic —
a date, a version, "2 + 2 = 4" written in a sentence — and would be wrong
visibly and often. The same reasoning fixes the aggregation boundary: `sum`
reads the block of `=` lines directly above it and never a bare number sitting
in prose.

**Predictability over cleverness, at the two points where they conflict.**
`200 + 10%` reads as an increase because that is what everyone means by it, but
the rule is attached to a `%` written on the line and not to a value that came
from one, so `taxa := 10%` followed by `= 200 + taxa` adds `0,1`. And a number
with two separators is refused rather than read as a grouping: `1.234.567` is a
thousand-grouped number in one convention and nonsense in the other, and the
result of guessing is a wrong answer that looks right. Results are printed
without a thousands separator for the same reason — a result that this same
engine cannot read back would be a trap.

**Top-down, so there is no graph.** A variable exists from its declaration
downwards. That makes `= preco * 2` above `preco := 100` an unknown variable
rather than a puzzle, and it makes cycles impossible without a resolver to
prevent them: `a := b + 1` over `b := a + 1` fails on the first line because `b`
is not there yet. A dependency graph would have been a resolver, a cycle
detector and an evaluation order to get wrong, in exchange for a behaviour
nobody asked for.

**Recalculate everything, and measure before optimising.** Each document change
re-evaluates the whole note. It is a scan and a small parser over one window's
worth of text; on a note with 100 paragraphs, 20 variables, 50 expressions and
three aggregators it is a fraction of a millisecond, which is less than the
bookkeeping an incremental version would need. Reactivity then falls out for
free rather than being a feature: there is no cache to invalidate.

**Plain paragraphs only.** Calculation is not read inside code blocks, inline
code, comments, headings, lists, tasks, quotes or callouts. Half-supporting them
would produce a note where the same line calculates in one place and not in
another for reasons the reader cannot see. The boundary is one rule, stated in
the documentation and tested; widening it later is a change to one function.

## ADR-025: Conversion Is a Line-Level Suffix, and the Unit Table Is Data

Phase 3.7 had to add conversions without building a second calculating engine
beside the first one, and without letting a unit table grow into a physics
library. Four decisions did that.

**`em` sits at the line level, not inside the expression grammar.** A line is
`expression unitRef 'em' unitRef`, and the expression parser runs first and
stops of its own accord at the source unit — an identifier following a complete
expression is not something any rule can continue. That single placement is why
conversion cost the expression grammar nothing: `10`, `distancia`, `(10 + 5)`
and `x * 2` parse exactly as they did in 3.6, and whatever they leave behind is
where the units are read from. Putting `em` inside the grammar, as `de` is,
would have meant a unit becoming a kind of operand, and with it a decision about
what `2 * 3 km` means before there was any need to have one.

The cost is one stated rule: **the unit applies to the whole left-hand
expression**, so `= 10 + 5 km em m` is fifteen kilometres. There is no unit
algebra to give the other reading a meaning, and one rule a reader can hold in
their head beats two they have to guess between.

**A unit is a row in a table, not a branch.** Every conversion library
eventually learns that `if km then m, if m then cm` is O(n²) rules to write and
O(n²) rules to get wrong. Each row carries a dimension and a scale, the
conversion is `value × from.scale ÷ to.scale`, and adding a unit is adding a
line. Temperature is the exception the shape has to allow for — its scales have
different zeroes, and no multiplication takes 0 to 32 and 100 to 212 at the same
time — so those rows carry `toBase`/`fromBase` instead, and nothing outside
`convert.ts` has to know which kind a row is.

Two consequences worth stating. **Area is its own dimension**: `m²` is a row
with factor 1 and `cm²` a row with factor 0.0001, not `m` with an exponent, so
`1 m²` is `10 000 cm²` and not `100`. And **speed is three named rows**, not a
length divided by a time. A derived-unit system would have been the start of a
physics library; `km/h`, `m/s` and `mph` are a table with three lines and one
extra rule in the unit reader, which is where this phase drew the line.

**Exact spellings, no case folding, and only values that are not opinions.**
Lookup is a `Map` keyed by every spelling the table lists, matched exactly. `m`
is a metre and `M` is nothing, because a rule that folded them would fold `MB`
onto `mb`, which differ by a factor of eight million; where a lower-case
convenience is safe it is listed as an alias, which is why `ml` and `l` work.
The `Map` is also the security property, the same one the math engine's
variables have: an object would answer `constructor` and `__proto__` with real
JavaScript values.

What is *not* in the table matters as much. `cup`, `tsp`, `xícara` and
`alqueire` are real measurements with more than one real value, and a conversion
whose answer depends on which definition the reader had in mind is worse than no
conversion, because it is wrong silently. Portuguese aliases are ASCII
(`quilometros`, not `quilômetros`) because variable names and unit names share
one lexer, and widening it for accents would change what an accented word means
in an expression — a policy decision about variables, made accidentally, to buy
an alias. Three characters *were* added, `°`, `²` and `³`, because they appear
in unit symbols and in nothing else, and refusing a pasted `1 m² em cm²` would
have been the wrong kind of strict.

**A variable holds a number, not a quantity.** `distancia := 10 km` is an
invalid expression, and the supported form is `distancia := 10` with the unit on
the line that uses it. Carrying units through variables means the engine's value
type stops being `number`, and percentages, aggregation, `isLiteral` and every
rule already established have to be re-decided around a quantity type. That is a
real feature and a coherent one, but it is not a thing to slip in beside a unit
table; the roadmap asked for a conscious choice rather than a hybrid, and this
is it, documented where a reader meets it.

For the same reason **a converted line ends an aggregation block**. `sum`, `avg`
and `count` add up plain numbers and know nothing about units. Letting a
converted quantity into a block would total ten thousand of one thing against
five of another and present the answer as a fact. Aggregating over units is a
feature; aggregating silently across them is a bug.

**Currencies are absent, and the absence is the deliverable.** Everything in the
registry is a constant: a kilometre is a thousand metres on a machine that has
never had a network interface, and it will be in ten years. That property is
what makes it safe to compute a conversion silently, as a decoration, with no
cache and no staleness to reason about. A currency has none of it — no answer
without a rate, a different rate every minute, and a rate hardcoded here would
be wrong before the commit adding it finished pushing.

So the boundary is the module edge, and honouring it now cost nothing:
`Dimension` lists only quantities that are constants, and `convertValue` is
synchronous and total. A rate-backed conversion is neither, so it cannot be
added to this function without the change being obvious — it belongs behind an
asynchronous provider with its own staleness, its own failure state and its own
way of telling the reader how old the number is. No provider interface was
written, because an empty abstraction is a worse guide to the future than a
plain statement of what the future has to look like. What this phase owes the
next one is the absence of a hardcoded rate, and a test asserts that nothing in
the engine can reach the network at all.

## ADR-026: Test Isolation Has to Cover the IPC Channel, Not Only the Filesystem

`scripts/note-it-isolated` overrode the four XDG base directories and nothing
else. That is the obvious reading of "isolate the store", and it is wrong for
this application in a way that is invisible until it costs something.

Note-it is a single-instance `GApplication`. Single-instance is not a lock file
or a pid check: it is a well-known name on the **session bus**. The second
process to start finds the name owned, hands its command line to the owner over
D-Bus, and exits. The owner does the work.

So the XDG variables configured a process that never opened a store. With a
daemon already running on the real bus, every "isolated" command was forwarded
to it, and the real daemon wrote to the real store. During Phase 3.7's physical
testing that put a test note in the user's own notes directory.

**The decision: a test environment must isolate every channel by which work can
leave it, and for a single-instance application the IPC bus is one of them.** The
harness now starts a private `dbus-daemon` per session and points
`DBUS_SESSION_BUS_ADDRESS` at it. On that bus the well-known name is unowned, so
the isolated process becomes the primary instance and does its own work. The
real daemon is never stopped and never notices, which matters: a harness that
required killing the user's session would simply not be used.

**Fail-closed, with no partial success.** Every check runs before Note-it is
launched — the bus starts, it is proved to be a different address from the real
one, it is proved to answer — and the launched process's environment is then read
back from `/proc` and compared. Four exit codes name the four guarantees (90 XDG,
91 binary, 92 bus, 93 launched environment). There is deliberately no path that
degrades to "at least the XDG part worked", because that is exactly the state the
old script was in while it was failing.

**`XDG_RUNTIME_DIR` stays real.** `WAYLAND_DISPLAY` resolves inside it, so
replacing it would cost the display. `DBUS_SESSION_BUS_ADDRESS` decides the bus
and wins over the runtime directory's socket, so setting it is both sufficient
and the only thing that does not break something else. The D-Bus *starter*
variables are cleared for the same reason a belt has a brace.

**The bus is per-session, not per-command.** A single-instance application can
only be tested across several commands if they share a bus, so `--root DIR`
records the bus under that root and every later invocation naming it reuses it.
That is what makes "start a daemon, then send it `new`" testable at all, and it
is the shape every physical test in this project takes.

**The regression test builds the incident rather than describing it.**
`scripts/test-isolation` stands up an ambient session with its own bus and its
own lived-in store, fingerprints it to the nanosecond, and — where a display
exists — puts a genuine `note-it --background` daemon on it owning the real
well-known name. Then it runs the harness. Against the fixed harness the note
lands only in the throwaway store; against the old one the test reports the stray
note sitting in the ambient daemon's store, which is the incident exactly. It
runs under `cargo test`, because a test nobody remembers to run is documentation.

The stub half of that test exists so the whole thing runs in CI, where there is
no display. What the stub proves is where the harness *points* a process, which
is the thing that failed; the daemon half proves the consequence.


## ADR-027: Search Without an Index, and Two Different Ideas of "the Same Word"

Phase 3.8 had to make every note findable, take the reader to what was found,
and let them change it — without any of that becoming a second source of truth
about what the notes contain. Four decisions did that.

**No index, because the scan is already fast enough to be invisible.** A
thousand notes are listed, read, accent-folded, matched and turned into snippets
in about 40 ms on this machine (about 20 ms before Phase 3.8R made the ordering
read each note's own `updated_at`); a query that matches nothing costs the same, and
one that matches everything costs less because the result list is capped. That
is well under the threshold where a person perceives a delay, and it is the
whole budget — there is no warm cache and no first-run penalty, because there is
nothing to warm.

An index would buy nothing measurable here and would cost a great deal that is
not measured in milliseconds: invalidation when a file changes underneath it,
rebuilding after a crash, a format version to migrate, a file to back up that is
not a note, and a second implementation for the CLI to agree with. Every one of
those is a way for search to disagree with the notes. Reading the notes cannot
disagree with the notes. The measurement lives in
`searching_a_thousand_notes_is_fast_and_writes_nothing`, so the claim is
re-checked rather than remembered, and the day it fails is the day this decision
should be revisited — with the number in hand.

**Search reads; it never writes.** Nothing in the search path flushes, saves or
touches a note, and opening a result does not either: activating, opening and
expanding are window state, and `updated_at` means "when the text last changed".
A reader must be able to search their notes a hundred times and find every
timestamp exactly where they left it. The same test that measures the scan also
asserts that the notes' modification times are unchanged after it.

**Two different foldings, on purpose.** Global search is
accent-insensitive: `biopsia` finds `Biópsia`, which in Portuguese is the
difference between search working and search being a typing exercise. Find and
Replace inside a note is accent-*sensitive*, because replacing is destructive
and a reader who types `saude` has not asked to overwrite `saúde`. Being able to
say why they differ is worth more than the tidiness of one rule.

That leaves a seam, and it is closed explicitly: a result carries the spelling
that actually matched *in that note*, so choosing `biopsia` from the palette
tells the note to look for `Biópsia`. Without it the note would open on a
highlight of nothing, which is a worse answer than not searching. Recovering
that spelling is why folding is length-preserving where it can be and mapped
back through the source where it cannot — the folded offsets have to name real
positions in the original bytes.

**Replace is a transaction, not a string operation.** Serialising the note to
Markdown, running `String.replace` and reloading would be a few lines and would
throw away marks, selection, scroll position and the undo history, and it would
apply the replacement to link targets, escape characters and everything else the
serialiser writes that the reader never typed. Instead every occurrence is a
document range, and `Replace All` is one ProseMirror transaction applying them
last-to-first — last-to-first so earlier positions stay valid, one transaction
so twenty replacements are one `Ctrl+Z`. Marks, list structure and headings
survive because the document was never rebuilt.

The same principle answers what Find is allowed to see. A calculation's `4` and
a conversion's `10000 m` are decorations, and decorations are not in the
document; a search over the document therefore cannot find them, with no rule
needed to exclude them. `Ctrl+F` for `4` in a note whose only `4` is a result
finds nothing, which is exactly right: that character is not in the file.

**Pasting a URL on a selection is one behaviour with one gate; compact links
are none.** (Phase 3.8 called this "AutoPaste"; Phase 3.9 renamed it, without
changing it, so the word is free for the clipboard capture mode Phase 3.11 will
bring — see Phase 3.9 in the roadmap.) Pasting a
URL over selected text is the one paste where the reader's intent is
unambiguous — they chose the words first — and where the default behaviour
throws away the thing they chose. It reuses `safeLinkUrl`, the allowlist the
autolink policy already had, so there is exactly one opinion in the application
about what a URL is; Tiptap's own `linkOnPaste` was switched off for that
reason, because it uses `linkifyjs` and would have accepted schemes this
application does not allow. Nothing is fetched: no title, no favicon, no
preview, and therefore no network, no tracking and no waiting.

Compact link rendering was evaluated and deliberately not implemented. Its whole
effect is to hide part of a destination, and the reader who most needs to see
`https://evil.example.com/path` in full is the one a shortened form would fool.
Note-it already renders a link's text and keeps its target in the Markdown,
which is the honest version of the same idea. The roadmap asked for it "only
where it fits the architecture"; it does not, and saying so is the deliverable.


### ADR-027.1: Making the Promises Match the Behaviour (Phase 3.8R)

Phase 3.8 shipped a search that worked. Four things it *said* were not quite
what it did, and 3.8R corrected the four rather than growing the feature.

**"Every note" now means every note.** The scan stopped at 5 000 notes. It was
a ceiling nobody would meet and a promise nobody could check: the 5 001st note
was unfindable, and nothing anywhere would have said so. A cap on results is a
different thing from a cap on the scan — a hundred rows is what a person can
read, and the reader can see there are a hundred of them. A note that is never
examined leaves no trace of having been skipped. So the scan reads the whole
store and `MAX_RESULTS` still caps the answer, with
`a_note_past_the_old_scan_ceiling_is_still_searched` putting a note at position
5 001 and finding it. The empty-query listing keeps a limit, because it shows
at most a hundred notes and reading past them would answer no question.

**A stale answer is any answer to a question that is no longer being asked.**
The palette numbered every request and refused any answer older than the last
one it had *accepted*. That covers a slow reply arriving after a fast one, and
misses the opposite order: ask `bio`, then ask `biopsia`, and the reply to
`bio` arrives while `biopsia` is still in flight — older than the current
question, but newer than anything accepted, so it was shown. The rule is now
the simpler one it should always have been: only the answer to the request
currently outstanding may change the list.

**The limits bound the query and the answer, not the note.** `MAX_QUERY_CHARS`,
`MAX_RESULTS` and `MAX_SNIPPET_CHARS` were described as making a pathological
*note* cost a bounded amount. They do not, and they must not: search finds text
at the end of a large note, which means reading to the end of a large note, and
a silent cut on how much of a file is searchable would put text in the store
that no search could ever return. The documentation now says what is true —
these are ceilings on the question and on the answer, the cost of a large note
is measured rather than capped, and no formal guarantee is claimed for an
arbitrarily large single file. Nothing was made asynchronous to satisfy a
sentence; the sentence was corrected.

**"Most recent" means most recently written in.** Phase 3.4R defined
`updated_at` as the last change to a note's *text*, and appearance — colour,
paper, pattern intensity, font size — deliberately does not move it. But
appearance is stored in the note file, so changing it rewrites the file, and
the ordering read the file's `mtime`. Recolouring a note therefore made it the
most recently "edited" note in the quick switcher, and the note a summon
brought back. The ordering now reads `updated_at`, the field the contract is
already written in, and falls back to `mtime` for a note that has none — one
written before the field existed, one with no front matter, one whose header
cannot be parsed. That fallback is the rule every note followed before there
was a field to read, so nothing about an old store changes.

It costs a bounded read of each note's head — 4 KB, enough for a front matter
of a handful of short lines — where listing previously cost only a `readdir`.
Measured, that is what took a search of a thousand notes from about 20 ms to
about 40 ms in release, roughly half in the reads and half in the YAML; the
removed scan ceiling accounts for none of it, because no store in the
measurement reached 5 000. Forty milliseconds is a third of the 120 ms the
palette waits before asking at all, so it is a price worth paying to have "most
recent" mean the same thing in the quick switcher, in search and in a summon.
An unreadable header costs that note its timestamp, never the listing: nothing
here writes, nothing here panics, and a tie is broken by identifier so the same
store always lists in the same order.

The head is read once per note to decide the order, and the notes that will
actually be shown are then read in full. That is deliberately not merged into a
single full read of every note: opening the palette on an empty query shows a
hundred notes, and a store holding a few enormous ones must not pay to read all
of them to list a hundred. Searching does read every note in full, because a
search cannot know which note holds the word until it has looked.

## ADR-028: Deleting a Note Means Moving Its File, and the Move Is the Commit Point

Phase 3.9 gave Note-it a deletion. Until then closing a note left it on disk,
which was safe and also meant there was no way to get rid of one from inside
the application. The version added is deliberately the smallest one that cannot
lose text.

**A deleted note leaves the active store.** `notes/<uuid>.md` becomes
`trash/<uuid>.md`, in a sibling directory of the same `note-it` data directory.
The alternative — a `deleted: true` flag in the front matter, with the file
staying where it is — was rejected: every reader of the store would then have
to know about the flag, and the one that forgot would list, search, summon or
restore a note the user had deleted. Listing, search, recency and startup all
read the notes directory, so taking the file out of it is what makes a deleted
note stop being a note everywhere at once, with no rule added anywhere.

**The move is the commit point**, the same rule ADR-020 established for a save.
The order is flush, move, state, surface, and every failure before the move
leaves the note open, live and editable. That order is the whole feature: a
note whose latest text could not be written must never disappear from the
screen as though it had been deleted, because the reader would have been shown
a deletion and charged an edit for it. Past the move the note *is* in the trash,
so neither the window-state write nor the surface teardown may report otherwise
— the state write is best-effort, and the window goes either way, because a
window still showing a note whose file has moved is showing something that is
not there. `commit_trash` is a free function over three closures precisely so
each of those failures is a test rather than a claim.

**Two directions, two tools, for two different risks.** Moving *to* the trash
is `rename`: one syscall, so there is no instant in which the note is both live
and deleted. Moving *back* is `hard_link` followed by `remove_file`, because
`rename` would silently replace a live note carrying the same identifier, and
checking first only narrows the race rather than closing it. `hard_link`
refuses an existing name atomically, so "restore never overwrites a live note"
is a property of the syscall rather than of a check. It is also the strictest
possible preservation of the file: the restored name is the same inode, not a
copy of it. The asymmetry is the point — each direction uses the primitive that
makes its own dangerous failure impossible, and the leftover a failed unlink
could produce (a note both live and listed in the trash) is visible and
harmless, where a silent overwrite would not be.

**Nothing is written into the note.** The date it was deleted goes in a
`<uuid>.json` sidecar beside the file. Writing it into the front matter would
mean the file that comes back is not the file that went in, and would make the
trash a second opinion about the note's content. A missing or unreadable
sidecar costs that entry its exact date and nothing else: the file's own
modification time answers instead, and nothing is written to repair it. The
consequence worth stating is that a note Note-it cannot even parse — damaged
front matter, hand-edited YAML — still goes to the trash and still comes back
byte for byte, because the trash moves files and never reads them.

**Trashing and restoring are not edits.** Neither one opens, parses or
serialises the note, so `updated_at` cannot move: a restored note returns to
exactly the position in the quick switcher it had, rather than pretending to
have just been written in. The window state entry is set to closed rather than
removed, so a note that comes back comes back the size and place it was — and a
stale entry naming a note that is no longer in `notes/` is inert, because what
startup restores comes from the files on disk.

**No permanent delete, and no empty-the-trash.** Both were deliberately left
out of this phase. The phase is about recovery, and an interface offering
irreversible destruction beside a restore button is one where the wrong click
cannot be undone. The trash is therefore unbounded, which is a real limitation
and is written down as one; a person who wants the space back can delete files
from `trash/` with any file manager, which is a property of storing notes as
ordinary files.

**The panel is a panel.** The trash reuses the search palette's shape — an
element in the page, not a second layer-shell surface to place, focus, stack and
tear down. Labels and snippets are written with `textContent`, exactly as search
results are, and every action addresses a `Uuid`: there is no message in the
bridge that carries a path, so `../../etc/passwd` is not a request that can be
spelled.

## ADR-029: A Backup Is a Directory of Files, Taken Before the Change It Protects Against

Phase 3.9's second half is a local snapshot of everything recoverable: `notes/`,
`trash/`, `config.toml` and `state.json`, copied into
`backups/<timestamp>/`.

**A plain directory, not an archive.** No tar, no zip, no database, no format
of Note-it's own. Whatever has gone wrong, a snapshot can be read with `ls` and
put back with `cp`, and recovering it requires nothing that could itself be
broken. Compressing it would buy space on a store measured in kilobytes and cost
the one property that matters when a backup is finally needed.

**Nothing leaves the machine.** No server, no cloud, no WebDAV, no Git remote,
no HTTP client — and none was added, so there is no network surface to audit
here at all. The honest consequence is written down rather than implied: a local
snapshot protects against an accidental deletion, a logical corruption, an edit
to undo, a version to go back to. It sits on the same disk as the notes, so it
protects against **none** of a dead drive, a lost machine or a stolen one, and
it is not encrypted. Selling local backup as disaster recovery would be the kind
of promise this project does not make.

**Taken before the first eligible change, not after it, and not on a timer.**
The check runs at the start of a persistent mutation — a note save, a move to
the trash — and nowhere else. A timer waking the process to ask whether a day
has passed would be continuous work in an application whose idle cost is a
feature; and a snapshot taken *after* an edit is a snapshot of the state you
wanted to get away from. So a daemon nobody is using does nothing at all, and a
daemon left open for a week takes its snapshot the moment its owner starts
typing again.

**The store is its own record of when it was last backed up.** The newest valid
snapshot's manifest answers "when", so there is no bookkeeping file to write, to
lose, to version or to keep honest — and no state that can disagree with what is
on disk. It is read once per session and remembered, because the question is
asked before every autosave and has to be free when the answer is "not yet",
which it is for all but one save a day. A failed attempt is not retried for
fifteen minutes, so a store whose backups cannot be written does not try again
on every keystroke.

**A failed backup is never a failed save.** A snapshot is an extra layer of
safety; turning its failure into a refusal to write would cost the reader the
edit the backup exists to protect. The failure is reported to `stderr` and the
save goes through. A backup the reader *asked* for is the opposite: someone is
waiting to know whether they have a safety point, so it always says which it
was, in a line at the foot of the note rather than a dialog over it.

**The rename is the commit point here too.** A snapshot is built in
`backups/.tmp.…` and renamed into place whole. A process killed halfway leaves a
`.tmp.…` directory, which can never be mistaken for a snapshot: it does not have
a snapshot's name and it has no manifest, and both are required. The next backup
sweeps it — and sweeps only directories carrying that prefix, because mistaking
a person's own file for debris would be a worse failure than the debris.

**Retention runs only after a new snapshot has been committed.** Seven are kept,
in one pool whatever made them. Deleting an old backup to make room for one that
then fails would trade real protection for none, so the order is create, commit,
prune — and a snapshot that cannot be pruned is reported rather than allowed to
fail a backup that already exists. Daily/weekly/monthly tiers were not built:
seven snapshots and one rule is something a person can hold in their head, and
nothing yet says the tiers would be used.

**A backup never follows a symlink.** Only regular files in the known
directories are copied, and a name beginning with `.` is skipped — which is
also what keeps a `.tmp.…` left by an interrupted save out of a snapshot. A
single crafted entry in the store must not be able to make the backup copy
`/etc`, a home directory, or anything else outside the two directories it was
asked to copy. `backups/` is never a source, so a snapshot can never contain
snapshots.

**Restoring a whole store is deliberately not a button.** Putting a snapshot
back over a live store is a multi-file transaction, and a one-click version of
it beside a normal menu entry is the most destructive control the application
could offer. What this phase owes instead is proof that the snapshot *is*
restorable, and that is a test: a snapshot is copied into a second, empty XDG
tree and opened, and the notes, identifiers, Markdown, trash, configuration and
window state all come back. The manual procedure is written down in
`docs/storage.md`, and it is `cp` with the application closed.

## ADR-030: The Timer's Truth Is an Instant, and It Is Not Part of the Note

**Status:** Accepted (Phase 3.10)

Two decisions, and everything else in the phase follows from them.

### A running countdown is stored as the moment it ends

The obvious implementation is a number and a repeating tick:

```js
setInterval(() => { remaining -= 1000; render(); }, 1000);
```

It is wrong in exactly the situations a timer exists for. `setInterval` does
not promise a tick a second; it promises no more than one. A WebView the
compositor is not showing gets throttled, a machine under load delivers late, a
suspended laptop delivers nothing at all — and each missed tick is a second the
countdown silently keeps. Come back from lunch and a 25-minute Pomodoro still
claims eleven minutes left. The error is not a rounding artefact to be tuned
away; it is the model paying out whatever the scheduler happened to deliver.

So a running run stores `deadline` — a wall-clock instant — and every reading
is `deadline - now`. Nothing decrements. A redraw that arrives late, or never,
costs a stale *picture* and not a wrong *answer*: the next reading is correct
whenever it happens. Drift cannot accumulate because there is no accumulator.

Pausing is the mirror image, and the mirror matters. A paused run has no end
instant — it has a debt — so the deadline is **discarded** and the remainder
frozen. Leaving a stale deadline on a paused timer is precisely what makes
paused time get spent anyway the next time something reads it, which is why
`sanitize` clears the field belonging to the state a record is not in, and why
there is a test for a paused record carrying a deadline.

`Date.now()` is the clock, deliberately, and not `performance.now()`. The
monotonic clock is the right answer to "how long did this take" and the wrong
answer to "how much of the afternoon is left": on Linux it does not advance
across suspend, so a machine closed for ten minutes would come back believing
no time had passed. Civil time is what a timer is about, and a jump in the
system clock is a rarer and more visible problem than a suspended laptop is.

The whole thing is therefore reconstructible from a number. Close the
application at 14:05 with a timer started at 14:00 for 25 minutes, reopen at
14:10, and the fifteen minutes that remain are computed, not remembered. Reopen
at 14:30 and the state is `finished` rather than a countdown through zero.

**Completion is guarded by the transition, not by a flag.** Only a `running`
run can finish, and finishing makes it `finished`. However many redraws observe
a deadline in the past, exactly one of them is the one that moves the state —
one write, one line at the foot of the note, one notification. The check and
the assignment are the same step, which is the only version of this that is not
a race.

**Restoring never rings.** A run that ended while the application was closed is
restored as finished and silently. An alarm about the past is not an alarm, and
any rule for "recent enough to still ring" would be an arbitrary number. The
finished state is on the bar instead, which is what actually tells the reader.

### A timer is operational state, in `state.json`, and never in the Markdown

The tempting place to put it is the note: it belongs to that note, and the note
is a file that already has front matter. That would be wrong for the same
reason the zoom is not in the note. A note's file is the reader's document —
the thing they wrote, the thing they can open in any editor, the thing whose
modification date orders their quick switcher. Starting a timer is not writing.

Putting the timer in the Markdown would mean: the file changes when nobody
edited it; `updated_at` moves, so a note jumps to the top of the switcher
because a countdown ticked over; the trash and the search index see a key
nobody typed; and `25:00` becomes findable text in a note that merely has a
Pomodoro running. Every one of those is a defect, and none of them is worth a
convenient home for seven scalars.

So it lives in the note's entry in `state.json`, beside the geometry, the
collapse state and the zoom — all of which are already "state of the
application about this note" rather than "state of the note". The consequences
are the properties the phase is judged on, and they hold structurally rather
than by care: search reads `notes/`, the trash moves files in `notes/`, the
collapsed title is projected from the Markdown, and none of those three ever
opens `state.json`.

**Written on a change, never on a tick.** Starts, pauses, resumes, cancels,
resets, phase changes and completions are writes; the seconds going by are not,
because the stored deadline does not change while it is being counted down to.
A running timer therefore costs no disk traffic and no IPC at all. A note whose
timer is in its pristine state stores nothing: the field is absent, so a note
that never had one looks exactly as it did before this phase existed.

**One per note, by construction.** The record hangs off the note's identifier
and the engine is one machine, so there is no arrangement of clicks that starts
two countdowns in a note and no shared slot for one note's timer to appear in
another. Changing mode is not a way around it: the tabs are unavailable while a
run is live and the engine refuses the change anyway. There is deliberately no
global timer manager — a note is the scope, and a second one is a second note.

### The page cannot write a notification

The completion message on the wire is a `TimerFinishKind`, a value from a
closed set of four. The words are `TimerFinishKind::notification` in the host.
The page reports *which kind of run* ended and has no field in which to supply
text, so there is no route by which a line of a note, a title or a snippet
could reach the desktop's notification area — not through a bug in the page,
and not through anything a note could contain. The notification is also
optional in the working sense: a desktop with no notification daemon gets none,
and everything else about the feature is unchanged, because the signal the
feature actually depends on is the one inside the note.

### What the split costs

The host does not run the countdown, so a timer does not complete while the
application is hidden — hiding destroys the WebViews, and there is nothing left
to observe the deadline. The completion is then delivered when the note comes
back, from the deadline, which is correct but late. The alternative is a second
state machine in Rust holding a `glib` alarm per note, and two owners of one
completion is exactly the shape that produces two notifications. Switching to
another application does **not** hide Note-it — the notes stay on screen with
live WebViews, and that is the case the feature is for — so the deferral
applies only to an explicit "put everything away". One owner, one transition,
one notification was worth that.

## ADR-031: Off Means No Listener, and the Toolkit Decides What Is Ours

**Status:** Accepted (Phase 3.11)

AutoPaste watches the system clipboard. The clipboard carries passwords,
tokens, private messages, medical notes and everything else somebody happens to
copy, so the decisions below are about that before they are about anything else.

### "Off" is the absence of a listener, not a branch inside one

The easy implementation keeps a handler connected and returns early when the
mode is off. It would behave identically and it would be the wrong shape,
because then "Note-it does not look at your clipboard" is a claim about a
conditional — one refactor, one inverted boolean, one early-return moved, and it
quietly stops being true.

So the `changed` handler is connected in exactly one place, when a note is
armed, and disconnected in exactly one place, when it is released. While
AutoPaste is off there is nothing subscribed to the clipboard at all. `AutoPaste`
in `autopaste.rs` still answers `NotArmed` for a change it should never see,
because a total policy is worth having; but the guarantee does not rest on it.

The same reasoning runs through the rest. `AutoPaste` holds four small fields
and none of them is text: there is no last-clipboard, no hash of one and no
buffer, so there is nothing to leak, nothing to persist and nothing that has to
be remembered to be cleared. The formats are checked before any read, so an
image is refused without being transferred. And no clipboard content reaches a
log at any level — the diagnostics record the *shape* of a decision (`read`,
`queued`, `ignored-own`, `ignored-not-text`) and never a byte of what was in it.

### Whether the mode is on is not written down anywhere

The delimiter is a preference and lives in `config.toml`. Whether AutoPaste is
*on* is deliberately stored nowhere: not in the Markdown, not in `state.json`,
not in the configuration, not in a sidecar.

This is not a limitation, and persisting it would not be a convenience. A mode
that observes the clipboard must never come back by itself after a reboot, a
crash, a logout or an update, because the person who switched it on last Tuesday
for one note is not necessarily consenting to it today. Having nothing to
restore from is what makes that certain — there is no field on `LoadNote` that
could switch it back on, which is why the test asserting that is about the
protocol rather than about the code.

### One target, because the clipboard is one thing

Two notes both capturing would mean every `Ctrl+C` filed twice, in two places,
which is surprising the first time and dangerous the tenth. So arming a note
releases whatever held it, in the same step, and both notes are told: a note
that has lost the target is still showing that it has it otherwise.

There is deliberately no capture manager, no queue of targets and no per-note
mode. One `Option<CaptureSession>` for the application is the whole model.

### The loop guard is `gdk_clipboard_is_local`, not a comparison

Copying inside the note that is capturing must not append the note's own words
back to itself. The tempting fix is `if text == last_text { ignore }`, and it is
wrong in a way that only shows up in use: copying `ABC` twice from a browser, in
two deliberate actions, is two captures, and content dedupe silently eats the
second one forever.

The right question is not "is this the same text" but "did *we* put it there",
and GDK answers it. A `Ctrl+C` or `Ctrl+X` inside a WebView is this application
claiming the clipboard, so `is_local()` is true and the change is refused before
any read starts. It is a property of the toolkit rather than a heuristic, and it
is checked at the only moment it can be checked reliably.

**What that costs, stated plainly:** `is_local()` is true for the whole process,
so copying from note B while note A is capturing is also refused. Distinguishing
them would mean the WebView reporting its own copy and the host racing that
report against GDK's signal on the same main loop, with nothing ordering the
two. A wrong answer there is either a note eating its own text or a capture
silently lost, so the conservative answer is the honest one: Note-it captures
from other applications, and note-to-note copying is done by pasting. That is a
real boundary and it is documented rather than papered over.

### A generation, checked when the read lands

A clipboard read is asynchronous, and everything can change while it is in the
air: the mode switched off, the target moved to another note, the note closed,
the application hiding. So every armed run carries a generation, every read
carries the session it started under, and the check when it returns is exact
equality against the state as it is *then*. Arming and disarming both mint a new
generation, which is what makes every read already in flight stale — including
the one that would otherwise arrive in a note the reader stopped capturing into
a moment ago.

Reads are also serialised, one at a time. Two in flight can finish in either
order, and captures arriving as A, C, B would be a defect nobody could explain.
A change arriving during a read is remembered and read after it; several
collapse into one, because the clipboard holds one value and the intermediate
ones are already gone.

Measured on a real Niri session rather than assumed: GDK emits exactly one
`changed` per copy there, so three copies produce three reads and three
captures, and no coalescing window was needed.

### Disarmed before the flush, never after

Closing a note, hiding, quitting and moving a note to the trash all end with a
WebView being destroyed, and all of them flush first. AutoPaste is switched off
*before* the flush in every one of those paths, so a read still in the air
cannot reach a document that is about to be written out and torn down. The
generation check would already refuse it; doing it in this order means the
question never arises.

### The host reads, the page inserts, the ordinary save writes

The capture goes from the read callback to the target note's WebView and
nowhere else. It is not written to the `.md` by the watcher, because the open
WebView owns the live document and two authorities over one file is how a note
loses an edit. The page appends it through a normal editor transaction, the
editor's own update path debounces, and the existing autosave writes the note —
which is also why a capture behaves like the edit it is: `updated_at` moves,
search finds the text, and a failed save fails the way every other failed save
here does.

Switching the mode on or off, and changing the delimiter, touch none of that.
They are application state, so they leave the note byte for byte as it was.

## ADR-032: A Picture Is a File Beside the Note, Reached Through a Scheme

**Status:** Accepted (Phase 3.12)

### The bytes are a file, and the note holds a path

The tempting shortcut is a `data:` URI in the Markdown. It works immediately
and it ruins the thing the file is for: one screenshot turns a note somebody
can read, diff, grep and hand-edit into a megabyte of base64 they cannot, and
it does the same to every backup and every commit that note ever appears in.
A note is a text file on purpose.

So the bytes go to `assets/<note-uuid>/<asset-uuid>.<ext>`, a sibling of
`notes/` and `trash/`, and the note stores `../assets/<note>/<asset>.<ext>`.

**Relative, and relative to `notes/` specifically.** `notes/` and `trash/` are
siblings, so `..` climbs to the same data directory from either of them: a note
moved to the trash and restored needs no rewriting, and the reference is valid
the whole way. An absolute path would have to be rewritten on every move, would
break the moment a store was copied to another machine, and would write the
reader's home directory into a file they may well put in Git.

**The identifiers are ours.** Whatever the file was called on the reader's disk
is not what it is called here. Nothing a filename can carry — a `..`, a
separator, a newline, a control character, a case fold that means something
different in another locale — survives into a path, because none of it is used
to build one. The format is decided the same way: by the first few bytes, never
by the extension. A PNG called `.txt` is a PNG, and an SVG called `.png` is
still an SVG and is still refused.

**SVG is refused, and by construction rather than by a rule about it.** It is a
document format that can carry script and external references. Admitting it
would mean auditing that whole surface for the sake of a picture. It has no
binary signature, so the same sniffing that accepts the other four rejects it
without a special case.

### The page asks for a picture; it never names a file

An `<img src="file:///home/…">` would have worked. It would also have put an
absolute filesystem path in the page's reach, in an application whose whole
frontend contract is that it never spells one — search takes a `Uuid`, the
trash takes a `Uuid`, and there is no message in the bridge carrying a path
precisely so there is nothing to traverse.

So the host registers `note-it-asset:` and serves it. The page loads
`note-it-asset:/<note>/<asset>.<ext>`, and the handler parses both halves as
`Uuid`s before anything touches the disk. A `..`, an absolute path, an extra
segment, a percent-encoded separator: none of them resolve to a file, because
none of them *parse*. Traversal is not blocked by a check that could be got
around; it is unrepresentable.

The page's Content-Security-Policy was widened by exactly that scheme —
`img-src 'self' note-it-asset:` — and by nothing else. No `http:`, no `https:`,
no `data:`, no `file:`. A note cannot fetch anything, which is also the answer
to remote images: one somebody typed by hand round-trips as the text it is and
is drawn with no source at all. Opening a note reaches the network for nothing,
and cannot be used to tell anybody that it was opened.

Measured before it was built on: a synthetic asset served this way loads in the
real WebKitGTK under the real policy, reporting its true dimensions. The icons
in Phase 3.9UX failed silently under this same policy, which is why this was
measured first rather than assumed.

### Two stored forms, and a rule for which

Markdown's image syntax has nowhere to put a width or an alignment, and those
are two of the four things this phase exists to deliver. HTML has, and this
codebase already stores what Markdown cannot as canonical inline tags —
`<span data-note-it-color>`, `<mark data-note-it-highlight>`.

So: plain `![alt](src)` while there is nothing to say beyond where the picture
is, and a canonical `<img>` once a width or a non-default alignment is chosen.
The rule is deterministic in both directions — the default alignment normalises
*back* to the plain form — so one picture is always one set of bytes and a save
that changed nothing changes nothing on disk.

The tag is canonicalised by the same function that canonicalises a `<span>`,
under the same discipline: four attributes, always in one order, each validated
rather than copied, and the source must be one of this store's own managed
assets. An `onerror`, a `style`, a `srcset` or a path climbing out of the assets
directory is not escaped and not kept — the tag is simply not one of ours, and
it is dropped.

**The alternative text is stored and never projected.** Every image this
application inserts carries `alt=""`. That is what keeps a note holding one
picture and no words still unnamed, and what keeps an asset's identifier out of
search — and it means the plain form and the tag form agree about what a note
reads as, which they would not if a filename-derived alt were projected from one
and stripped from the other. A hand-written `![alt](url)` keeps the behaviour it
has always had.

### The host stores; the page edits; the ordinary save writes

The host never writes the `.md`. It takes bytes, decides what they are, stores
them, and sends back a relative path; the page puts that into the document
through a normal editor transaction and the existing autosave carries it to
disk. One authority over the document, which is the same rule a clipboard
capture follows and the reason an image behaves like the edit it is:
`updated_at` moves, search finds the words around it, and a failed save fails
the way every other failed save here does.

The three ways in differ only in where the bytes come from. The file chooser is
the host's own dialog, so the path is one the *reader* picked rather than one
the page named. A paste and a drop hand the page a `File`, and the page sends
its bytes — base64 for the length of one message, never anything that reaches a
note. In none of the three does the page get to point the host at a file.

### A snapshot holds the pictures too (3.12R)

This did not, when 3.12 shipped. The bytes went to `assets/` and the backup
went on copying `notes/`, `trash/`, `config.toml` and `state.json`, so a
snapshot taken in between restores a note's Markdown and not the file its
`![](../assets/…)` points at. A backup whose promise is "everything
recoverable" that quietly holds half a note is worse than one that had never
claimed it, and 3.12 was not accepted until this was closed.

`assets/` is a tree rather than a flat directory, so it gets a copy of its own
rather than the flat one for notes being loosened into a general recursion —
a routine that descends wherever it finds a directory is how a backup ends up
following something out of the tree it was asked to copy, and it would put the
notes' own guarantees at risk to serve a different shape.

It is strict where the notes' copy is forgiving, and the asymmetry is the
point. `notes/` holds files a person may reasonably have put there themselves,
so an oddity is skipped with a warning. `assets/` is written by Note-it and by
nothing else, so anything that is not `<note-uuid>/<asset-uuid>.<ext>` means the
store is not in the state this believes it to be — and the one thing a backup
may never do is omit managed content while reporting success. No symbolic link
is followed at either level; each name is validated by the same
`parse_asset_request` the URI scheme uses, so a snapshot holds exactly the files
the application can serve.

An image no note points at any more is copied like the rest. Deciding it is
dispensable would be the garbage collection this phase deliberately does not
do, arrived at by omission instead of by design.

The transaction is the one that was already there: the copy happens inside the
scratch directory, before the rename that commits, so a failure copying an
image leaves no snapshot, no manifest and no pruned predecessor. The manifest
moves to version 2 and records the count; version 1 keeps parsing, because the
field defaults and nothing branches on the number — every snapshot on disk
today was written by version 1 and none of them may become unreadable.

### Removing a picture leaves the file

Taking an image out of a note takes it out of the note. The bytes stay.

There is no automatic collection of assets no note points at any more, and that
is deliberate rather than unfinished. Deciding a file is unused means being sure
about every note, including ones in the trash, ones being edited in a WebView
that has not saved yet, and ones a backup will later restore — and being wrong
destroys something the reader cannot get back. Keeping a file nobody references
costs disk space; deleting one somebody does costs the picture.

The arrangement leaves a future sweep possible: assets are grouped by note
identifier, so the set of live references is `notes/` plus `trash/` parsed for
`../assets/…`, and anything under `assets/<id>/` not named there is a candidate.
If that is ever built it should be something the reader asks for and can see the
result of first, not something that runs on its own.

## ADR-033: Semantic Metadata Lives in Markdown, but YAML Stays Behind Core

**Decision.** Tags and V1 textual Properties are top-level front-matter values beside the reserved
`note_it` mapping. `noteit-core` owns their validation, identity, ordering, persistence and derived
catalogs. Adapters receive domain structs; neither the WebView nor a future CLI reparses YAML.

**Identity.** Tags and property keys reuse `search::fold`: Unicode lowercase plus the documented
Latin accent table. The first tag spelling is presentation; folded identity is comparison. A fixed
FNV-1a hash of that identity selects one of seven reviewed UI colour slots, so colour is stable and
never stored.

**Limits.** One note has at most 32 tags of 64 characters and 32 Properties with 64-character keys
and 512-character single-line values. Rejection is explicit and truncation never occurs. V1 values
are strings: adding later types can extend the domain representation without changing existing
strings, but nested objects, schemas, relations and formulas are deliberately absent now.

**Preservation.** The typed front-matter wrapper flattens unknown top-level YAML into a private map
and writes those values back. This is semantic preservation, not a concrete-syntax tree: serde_yaml
does not retain comments, aliases/anchors or original whitespace. Those can normalize on a real
save, which is documented; an untouched open/close never serializes and stays byte-identical.

**Transactions and dates.** A metadata request carries the live Markdown. The host validates the
draft, clones its live `NoteDocument`, folds any pending text into the same candidate, calls the one
`StorageManager::save_note_atomic` path, and adopts/acknowledges only after rename commits. A failed
write leaves disk and memory old and the same draft retryable. Semantic-only change touches neither
timestamp; pending text moves `updated_at` because text changed.

**Catalogs and bounded reads.** Catalogs scan live notes on demand and therefore cannot become
stale; trash is excluded by directory membership. No index or database exists. Front-matter-only
reads stop on the real delimiter and cap work at 256 KiB, comfortably beyond every V1 field at its
limit. This replaces the 4096-byte recency assumption without reading note bodies.

**Rejected alternatives.** Sidecars split a note from its portable metadata and create a second
transaction. A persistent tag index introduces invalidation and recovery before measurement asks
for it. Sending YAML through IPC makes the WebView another format authority. Putting metadata into
ProseMirror makes search, titles and Study interpret bookkeeping as prose. All four are rejected.

## ADR-034: Headless CLI (`noteit`) and Desktop Adapter (`note-it`) Separation

**Decision.** Create a separate headless binary `noteit` in a dedicated `noteit-cli` workspace member
rather than embedding CLI functionality into the existing desktop GUI binary (`note-it`). Both
executables consume `noteit-core` as their shared domain and persistence authority.

**Rationale.**
1. **Zero GUI Overhead.** `noteit` must operate in headless environments (SSH, containers, scripts,
   agents) without requiring an X11 or Wayland display server, GTK initialization, WebKitGTK runtime,
   or `GApplication` session bus registration.
2. **Preserving Desktop Lifecycle.** `note-it` remains a specialized desktop adapter and single-instance
   lifecycle manager for sticky note windows. Modifying its command dispatcher for rich CLI tasks would
   couple desktop lifecycle with non-interactive CLI semantics.
3. **Strict Dependency Isolation.** `noteit-cli` depends only on `noteit-core` and lightweight headless
   libraries (`clap`). The boundary script `scripts/check-cli-boundary` and CI ensure no desktop
   dependencies enter `noteit-cli`.
4. **Pure Path Resolution.** `noteit status` must be strictly read-only and never create missing
   directories on disk. Path resolution was extracted into pure `StorePaths::resolve()` in `noteit-core`,
   reused by `StorageManager` only when actually initializing or opening stores.
5. **Bilingual UX & Human Error Presentation.** Human presentation is in Portuguese (`ajuda`,
   `versao`, `status`), with standard international aliases (`help`, `version`, `status`, `--help`, `-h`,
   `--version`, `-V`). Usage errors from Clap are mapped to user-friendly Portuguese messages on stderr
   using typed `ErrorKind` and error context without bypassing Clap as the parsing authority.
6. **Workspace Version Authority.** The project version is centralized in `[workspace.package]` with
   `version.workspace = true` across all crates (`note-it`, `noteit-core`, `noteit-cli`), preventing
   version drift.

### ADR-035: Headless Read API Architecture and Security Boundaries

**Decision.** Implement a strictly read-only, headless inspection API across `noteit-core` and `noteit-cli`, exposing notes listing, individual note retrieval, search, tag/property catalogs, task extraction, and trash inspection.

**Rationale.**
1. **Core-Centric Read Projections.** All domain read logic (filtering, canonical title derivation via `search::label_for`, task parsing, and metadata matching) lives directly in `noteit-core`. `noteit-cli` remains an adapter focused purely on CLI argument parsing and terminal presentation.
2. **Strictly Read-Only Open Mode.** `NoteItCore::open_read_only()` and `StorageManager::open_read_only()` inspect paths without calling `ensure_directories()`. Absent stores return clean empty results with exit code 0 rather than creating empty directories or state files.
3. **Safe Note Selector Resolution.** Note selectors (full UUID or >= 8 hex characters) are validated against path traversal (`..`, `/`, `\`) and non-hex characters before prefix matching against live note IDs. Ambiguous prefixes, non-existent IDs, and symlinks fail closed with exit code 1.
4. **Terminal Security & Sanitization.** Output rendered to terminals is sanitized (`output::sanitize_for_terminal`) to neutralize ANSI escape codes (CSI, OSC, OSC 52 clipboard hijacking), BEL, backspaces, and control characters, preventing malicious note content from manipulating terminal states.
5. **Task Parsing & Timestamp Integrity.** Task checkboxes (`- [ ]`, `- [x]`, `- [X]`) and depth nesting are extracted purely from Markdown text outside code fences (``` and ~~~) and front matter. `completed_at` timestamps are extracted only from valid ISO 8601 comment markers without ever inventing timestamps for missing or unparseable dates.
6. **Zero Store Mutations.** No state files, backups, temporary files, or directory structures are touched during read operations. Byte-for-byte store integrity is proven by test gates.

## ADR-036: Read API Contract Hardening, Local Datetimes, and Typed Warnings

**Decision.** Standardize human datetime presentation across `noteit-cli` to use the machine's local timezone matching the GUI contract, expand terminal input sanitization to all rendered untrusted strings, decouple non-fatal read warnings into typed `ReadWarning` / `ReadBatch<T>` in `noteit-core` with zero print statements, and align task metadata comment matching strictly with the TypeScript specification.

**Rationale.**
1. **Local Timezone Consistency (`dd/MM/yyyy HH:mm`).** Human users expect timestamps displayed by the CLI to match the local machine timezone seen in the desktop interface. Datetime formatting is centralized in `output::format_datetime_local` in `noteit-cli`, while `noteit-core` models remain strictly typed in UTC (`DateTime<Utc>`).
2. **Comprehensive Untrusted Input Sanitization.** All variable or external inputs rendered to stdout or stderr are sanitized via `output::sanitize_for_terminal` before styling or output. This includes search queries in headers, note selectors in error messages, Clap argument contexts in usage errors, and custom XDG paths in `noteit status`.
3. **Pure, Decoupled Core Warning Model.** `noteit-core` must not print directly to stdout or stderr with `println!` or `eprintln!`. Read methods return `ReadBatch<T>` containing both parsed items and typed `ReadWarning` structures (`note_id`, `kind`, `message`). The CLI adapter formats these warnings to stderr in Portuguese, while future JSON or MCP adapters can project them into structured error payloads.
4. **Faithful Task Comment Parsing.** Task completion comments `<!-- note-it:completed_at=... -->` are matched anywhere on the task line without requiring them to be the first HTML comment. Only the Note-it metadata comment is stripped from `TaskEntry.text`, preserving user-authored HTML comments. Unchecked tasks drop any completion timestamps.

## ADR-037: Read Pipeline Purity, Search Warning Unification, and Domain Query Separation

**Decision.** Unify the Core search pipeline to guarantee identical warning and loading policies across filtered and unfiltered searches over the complete eligible universe, remove all direct stderr prints from Core read paths, separate domain search queries from presentation sanitization, and enforce strict token matching on task metadata comments.

**Rationale.**
1. **Unified Search Pipeline.** Both `noteit buscar X` and `noteit buscar X --tag Y` use the same `load_note` and `ReadWarning` collection pipeline in `NoteItCore::search_notes_filtered`. Unfiltered search no longer bypasses warning generation, and corrupted notes consistently emit structured warnings in both modes without aborting the scan.
2. **Scanning the Full Eligible Universe.** Search queries scan all eligible notes before applying the user-specified result limit (`--limite`), ensuring matches in older or lower-recency notes are not missed.
3. **Eradication of Direct Prints in Read Paths.** Removed the remaining `eprintln!` in `StorageManager::read_bodies`. All Core read methods return pure data, errors, or typed warnings.
4. **Domain Query Separation.** The raw user query is provided directly to `noteit-core` without alteration, ensuring search logic operates on the intended search term. Terminal sanitization is applied strictly during presentation rendering.
5. **Strict Task Comment Regex Matching.** Task metadata extraction validates that `<!-- note-it:completed_at=... -->` contains exactly one non-whitespace timestamp token. Comments with trailing garbage (e.g. `<!-- note-it:completed_at=2026-08-27T11:32:00Z lixo -->`) do not match the Note-it metadata regex and are left unmodified in the note text.

## ADR-038: One Note-it Writer per Store, and the Barrier That Makes It True

**Decision.** Exactly one Note-it process may write a store at a time, enforced by an advisory
`flock` on a lock file in the runtime directory. The desktop instance takes that lease at startup and
holds it until the process ends; the CLI takes it for the length of one command when it is free, and
when it is not, sends the change to whoever holds it over a private Unix domain socket rather than
writing the file itself. A note that is open in a window is changed only after its editor has been
frozen and its live text collected, and everything the page sends afterwards carries a runtime
generation the host checks.

**Rationale.**

1. **An atomic write is not enough.** `write_atomic` keeps a *file* whole; it says nothing about two
   processes that each read a note, each change their own copy and each write it back. Both writes
   succeed, both files are intact, and one person's edit is gone — and nothing in the storage layer
   can see it happen, because from where it stands both writes were correct. The exclusion therefore
   has to live above the file, and it has to be the same mechanism in both adapters or it is not
   exclusion at all.

2. **A lock, not a file.** The lease is `flock` on a lock file, never the existence of that file. A
   process that crashes releases it immediately, because the kernel closes its descriptors; a lock
   file left behind by a dead process blocks nobody. No PID is trusted, no timestamp is compared and
   no staleness is guessed — all three are ways of being wrong about whether anyone is there. Rust's
   standard library provides this since 1.89, so it costs no dependency.

3. **Keyed by store, not by machine.** An isolated test store and the real store are two different
   stores with two legitimate writers at the same time. The coordination directory is named after a
   deterministic digest of the notes directory, so they never contend — and a test can never deadlock
   against the application its author is using.

4. **Runtime, not store.** A lock and a socket describe this boot. They are meaningless after a
   restart, must never be backed up, and have no business sitting next to the notes.
   `$XDG_RUNTIME_DIR` is the directory the specification defines for exactly this. Both directories
   are created `0700` and refused if they are a symlink or belong to another user; the socket is
   `0600` inside them.

5. **The desktop instance is the authority because only it can be.** A note open in a window may hold
   a paragraph the file does not have yet. The only process that can safely write that note is the one
   that can ask the window for it, so while Note-it is running everything goes through it. The lease
   is held for the whole session and released only when the process ends, because that is the moment
   it stops being able to save.

6. **A flush is not a barrier.** Asking the page for its text and then writing has a gap: the reader
   keeps typing, the answer is already out of date when it arrives, and the character typed in
   between is written over. So the page stops being editable *first* and reads its own text second.
   Freezing is at the transaction, not at editability alone — editability stops the reader, and the
   page itself changes documents through commands that do not care about it.

7. **A generation, so nothing in flight can undo a commit.** Each committed external write moves a
   runtime counter. Every message from the page that carries content quotes the generation it was
   composed against, and the host refuses anything older. Without it, an autosave that left the page
   before the commit would land after it and put the previous body back.

8. **Refused and committed are never confused.** A write that failed before the commit point changed
   nothing and may be repeated. A write that committed but could not refresh the window is *not* a
   failure, and reporting it as one would have someone append the same paragraph twice. A connection
   that dropped after the request went out is neither, and is reported as unknown — because guessing
   either way is how a note ends up with duplicated text.

9. **Task references are optimistic snapshots, not identity.** Phase 4.0D deliberately gave tasks no
   persistent identifier and this does not smuggle one in: no sidecar, no database, nothing written
   into the Markdown. A reference is recomputed from the note at the moment of the write and refused
   if it no longer names exactly one task. Being told to list the tasks again is a far better outcome
   than quietly ticking off a different one.

10. **Private, and staying that way.** The control protocol is a local Unix socket carrying
    length-prefixed JSON. There is no TCP, no HTTP, no port and no localhost server, and a request
    cannot carry a filesystem path because there is no field to put one in. It is an implementation
    detail of the handover — not the machine-readable interface Phase 4.0F is reserved for — and
    nothing outside this repository may depend on it.

## ADR-039: The Desktop Instance Owns the Store or Does Not Start, and Adoption Is Something the Page Says

**Decision.** A Note-it desktop instance that cannot take the writer lease *and* open its control
socket refuses to start, rather than running without being the store's authority. A committed
document is considered to have reached the window only when the page itself sends
`ExternalWriteApplied`; evaluating the script that carried it is not treated as proof. And once the
page has handed over its snapshot, it never releases the document on a deadline of its own — only
`ApplyExternalDocument` or `AbortExternalWrite` unfreezes it.

**Rationale.**

1. **ADR-038's invariant was not actually enforced.** The first implementation held the authority as
   an `Option` and carried on when it was `None`: an instance that failed to take the lease still
   opened windows, still autosaved, still wrote notes. That is a second writer, produced by the code
   meant to prevent one. "Exactly one writer per store" is either a property of the system or it is a
   comment, and an optional field made it a comment.

   It is now a type. `AppContext` holds `WriteAuthority` by value, the only way to obtain one is a
   complete `claim`, and the claim happens before any window, document or autosave exists. A running,
   editable Note-it that does not own its store is not a state this program can describe.

2. **A lease without a socket is not authority either.** If the control socket cannot be opened,
   `noteit` finds the store held and its holder unreachable — so it correctly refuses every write, and
   the desktop instance has locked everyone else out of a store it alone can change. Startup fails and
   the lease is released on the way out, which is strictly better than one process quietly becoming
   the only writer that works.

3. **There is no read-only mode, deliberately.** It would be a third state to reason about, and the
   honest answer to "something else owns your notes" is a sentence, not a degraded application.

4. **`evaluate_javascript` returning `Ok` proves the script ran.** It does not prove the message was
   routed to a listener, that the listener matched the request, or that the document was adopted — and
   the page catches its own listener errors, so a failure inside one still reports a successful
   evaluation. Treating delivery as adoption meant a window showing pre-commit text could be reported
   as synchronised. The page now says so itself, naming the note, the request and the generation it
   took, and only after it has adopted the document and resumed editing. Delivery failure is still
   used, but only to fail fast: a script that could not be evaluated certainly did not update anything.

5. **A rejected adoption is answered, not left silent.** `ExternalWriteApplyFailed` carries the note
   and the request and nothing else — no reason, no stack, no note content, because the host acts on
   whether and never on why. It costs one message and saves the host waiting out a timeout to learn
   something the page already knew.

6. **A page that could not adopt keeps the old generation.** It is showing text the file no longer
   has, so it must not be able to save that over the change that was just committed. Staying on the
   superseded generation is what makes the host refuse it. *(Amended by ADR-040: such a page is also
   never released. Keeping the old generation stops the stale text reaching the file, but on its own
   it left the reader typing into an editor that silently discarded everything.)*

7. **After the snapshot, there is no safe time to guess.** The old client-side timeout released the
   document fifteen seconds after `ExternalWriteReady`, at which point the host may be part-way
   through writing a temp file, syncing it or renaming it. The reader would then be typing against a
   document about to be replaced — exactly the race the barrier exists to remove, reintroduced by the
   barrier's own safety net. A slow commit is allowed to be slow; the honest response is to say so.
   The indicator now escalates to "Sincronização demorando…" and nothing else happens.

8. **There is no orphan to rescue.** The WebView belongs to the same process as the host. If the host
   dies the page dies with it, so a self-release could never save a page from a vanished host — it
   could only take integrity away from one that was still working.

9. **A commit stays committed.** The acknowledgement runs entirely after the commit point, so it can
   only decide whether the answer carries `ui_sync_warning`. Missing, refused, or undeliverable, the
   write happened, the command succeeds, and nothing invites a retry that would append twice.

**Known limit, recorded for Phase 4.0R.** The store key is the digest of the notes directory path *as
each process resolved it*. Two processes given the same XDG environment agree, which is what the
exclusion needs — but two different spellings of one store (`/srv/./notes`, a symlinked home) produce
two keys and therefore two simultaneous authorities over the same files. Fixing it means canonicalising
a directory that may not exist yet, which is a larger change than it looks and is not worth improvising
inside a correctness fix.

## ADR-040: A Window That Could Not Take the Committed Document Stays Shut

**Decision.** When a commit has happened and the page fails to adopt the committed document, the
document is **not** released: the editor stays frozen, the queued document actions stay queued, the
generation stays where it was, no positive acknowledgement is sent, and the note says it is out of
step until it is reopened. The write itself remains committed and is still reported as committed with
a `ui_sync_warning`.

**Rationale.**

1. **ADR-039 got this one wrong, for a plausible reason.** It released the editor after a failed
   adoption, arguing that the file was already correct and that a frozen note would be unusable and
   unclosable. Both halves of that are true and the conclusion still does not follow. The released
   editor is on a generation the host has already moved past, so every autosave it sends is correctly
   refused — the reader types into something that looks completely normal and loses all of it, with
   nothing on screen to say so. Keeping the old generation protected the *file*; it did nothing for
   the person.

2. **A visible inconsistency beats an invisible one.** A note that is held and says
   "A alteração foi gravada, mas esta janela não conseguiu acompanhá-la. Reabra a nota." is an
   inconsistency someone can see and act on. An editor that accepts every keystroke and stores none is
   one they find out about later, if ever. Between a note that will not take input and a note that
   eats it, only one of them is recoverable.

3. **The queue follows the same rule.** A held capture, image or metadata save is not discarded — and
   not run either. Running it would apply a mutation to a document the store has already moved past,
   which is the same failure wearing different clothes. It stays held; reopening the note is what ends
   the situation.

4. **`release` was doing more than its name suggested.** It clears the active request, cancels the
   indicator, thaws, *and* drains the queue. That combination is only safe when the page has a
   document worth editing on. Calling it unconditionally read like bookkeeping and was in fact the
   decision. It is now called on exactly two paths: an abort before the commit, and an adoption that
   succeeded.

5. **Nothing later can unlock it.** A repeated `ApplyExternalDocument`, an abort, or a message for a
   different request all leave a failed page exactly where it is. There is no sequence of messages
   that talks the page back into editing text the store no longer has.

6. **A blocked note blocks further external writes, and that is correct.** The page cannot produce a
   trustworthy snapshot, so the barrier never answers, the host times out before committing anything,
   and `noteit` is told the store is busy and nothing was changed. A refusal is the right answer;
   writing against a snapshot nobody can vouch for is not.

7. **Reopening is the recovery, and it is enough.** The file holds the committed content, so
   restarting the application — or a future, deliberate reload of a single note — brings the window
   back onto it exactly, with no duplication and nothing lost. That is verified end to end in the
   isolated environment rather than assumed.

**Not done here, on purpose.** No automatic reload, no reconciliation, no merge, no background retry,
no content hash in the acknowledgement. A safe per-note reload is the obvious next step and is
recorded as a recommendation, not smuggled into a correctness fix.

**Amendment (4.0E.2R): terminal had to be made true, not just stated.** Two ways out of the terminal
state survived the original fix, both found by audit rather than by a failing test.

The first: the slow-notice timer was left armed, and its only guard asked whether the request was
still the active one — which, after a failed adoption, it deliberately is. So four seconds later the
page replaced "this window could not keep up, reopen the note" with "synchronisation is taking a
while", which was not merely cosmetic: it described a write still in progress when there was none,
and pointed away from the only recovery there is. The timer is now cancelled on that path, through a
helper that does *only* that — reaching for `release` to cancel a timer is what caused the original
4.0E.2 bug, because it also thaws and drains.

Cancelling is not the guarantee, though. A callback can already be queued when its timer is
cancelled, so the phase itself is now the gate: the page holds one `SyncState`, every transition asks
what state it is in, and a late callback finds a state it may not act on. `unsynchronised` has no
outgoing edge at all — not from a timer, a repeated apply, an abort, a message for another request,
or a `LoadNote` generation. The same guard fixes the symmetric case nobody had reported: a stale
notice arriving after a *successful* write, which would have made a finished write look slow.

The second: adopting a document briefly lifts the editor's transaction lock — it is the one change
the lock exists to let through — and the restore sat after the call rather than in a `finally`. An
adoption that threw part-way therefore left the lock off, which is exactly the moment every command
the page can run must be refused. It is now restored in a `finally`.
