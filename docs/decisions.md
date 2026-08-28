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

