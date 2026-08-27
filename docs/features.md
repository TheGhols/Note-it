# Features

## Window & Layer Modes

Note-it leverages Wayland Layer Shell to provide three distinct surface modes:

1. **Desktop Mode (`bottom` layer):**
   - Post-it surfaces remain pinned above the desktop wallpaper but behind application windows.
   - Non-intrusive keyboard mode to avoid stealing focus during normal window navigation.

2. **Overlay Mode (`overlay` layer):**
   - Post-it surfaces surface above all active applications, including maximized and fullscreen windows.
   - Interactive focus is enabled for swift editing.

3. **Hidden Mode:**
   - Surfaces are detached/hidden while the background daemon remains ready for instant activation.

## Note Header

- **Settings Menu (`☰`):**
  - A three-line button on the left of the header opens a small popover anchored to the bar.
  - Entries: **Cor da nota**, **Tipo de papel**, **Intensidade**, **Tamanho do texto**,
    **Cor do texto**, **Marca-texto**, **Zoom**, **Tema**, **Camada**, and **Recolher nota** /
    **Expandir nota**.
  - The menu shows the current paper, intensity, zoom, theme and layer on their own rows, so none
    of them depends on opening a submenu or knowing a shortcut.
  - Closes on outside click, `Escape`, or selecting an entry; only one popover exists per note.
  - The button and the popover sit outside the drag region, so using them never moves the note.
- **Note Information Tooltip:**
  - Resting the cursor on the free area of the header for ~450 ms shows the note's creation and
    modification dates in pt-BR `dd/MM/aaaa HH:mm`.
  - The tooltip never takes the pointer (`pointer-events: none`) and is dismissed by leaving the
    bar, clicking, starting a drag, or opening the menu.
- **Collapse / Expand:**
  - Collapsing reduces the note to its header bar; the editor is hidden, never unmounted, so the
    content, formatting and the Tiptap instance are preserved.
  - The expanded width and height are recorded before collapsing and restored on expand, at
    whatever position the collapsed bar was left.
  - A collapsed note can still be dragged; resizing is unavailable until it is expanded again.
  - The collapsed state is persisted, so a note left collapsed reopens collapsed.

## Paper

Each note carries its own paper, independently of every other note.

- **Cor da nota:** the seven colours — Amarelo, Azul, Verde, Rosa, Roxo, Cinza, Preto.
- **Tipo de papel:** **Liso**, **Pautado**, **Pontilhado**, **Quadriculado pequeno**,
  **Quadriculado grande**. Plain paper is the original look and draws no pattern at all.
- **Intensidade:** **Suave**, **Normal**, **Forte** — the opacity the pattern is drawn with, and
  nothing else. It never changes the paper colour, the text, or the note's geometry. Plain paper
  keeps whatever intensity it was given; it simply has no pattern to act on.
- The pattern is pure CSS: one parameterised system where the type picks a pattern and its
  spacing, the intensity picks the opacity, and the paper colour picks the ink — dark ink on the
  pale papers, light ink on the dark one, so it stays visible on all seven.
- Spacing is in pixels, so zoom scales the text while the pattern stays put. Ruled paper is spaced
  to the note's default line box, but it is a background, not a layout grid: lines are not pinned
  to individual lines of text.
- The pattern is painted on the scrolling surface, so it travels with the text, and the note's own
  colour still fills the window underneath — a fast resize exposes paper, never an unpainted strip.
- A collapsed note's bar shows its colour without the pattern; expanding brings the pattern back.
- Paper type and intensity are properties of the note, stored in its front matter beside the
  colour. Changing either saves the note without touching its content or its modification date.

## Theme

The theme is the appearance of the **application**, not of a note.

- **Sistema**, **Claro**, **Escuro**, chosen from any note's menu and shared by every note. The
  preference is global and lives in `config.toml`.
- **Sistema** follows the desktop's colour scheme and keeps following it, so switching the desktop
  to dark reaches open notes without a restart.
- It dresses only the chrome: menus, popovers, borders, shadows, hover and focus states, and
  auxiliary text. Everything drawn on the paper — the note's text, checkboxes, highlights, the
  header buttons — keeps taking its colour from the paper.
- A note keeps the colour it was given: a yellow note stays yellow under the dark theme, and a
  black one stays black under the light theme.

## Window Positioning & Interactions

- **Drag & Resize:**
  - Header drag region (`.drag-region`) for moving post-its freely across the workspace.
  - Discrete bottom-right resize handle (`.resize-handle`) with min-dimension limits (`220x160` px).
  - A gesture emits geometry deltas only while exactly one pointer is captured; `pointerup`,
    `pointercancel`, a lost pointer capture, or a move reporting no button held all end it
    completely, and a frame left over from before the end cannot move the window.
  - Geometry persisted to `$XDG_STATE_HOME/note-it/state.json` exclusively on gesture end (zero disk I/O during active dragging/resizing).
- **Safe Geometry Clamping & Monitor Fallback:**
  - Clamping guarantees notes stay visible on-screen even after monitor resolution changes.
  - Multi-monitor connector detection with graceful fallback if a display is disconnected.
- **Smart Cascade Placement:**
  - New notes cascade incrementally across the screen grid.

## Note Lifecycle

- **Closing keeps the note:** the `×` button saves the note, records it as closed, and destroys only
  the window. The Markdown file, its geometry, colour, zoom and collapsed state all stay on disk.
- **Summoning brings it back:** running `note-it` restores the notes and makes them visible. With
  every note closed, the one used last is reopened instead of a blank note being created.
- **One instance:** a second invocation reaches the running instance through the single-instance
  dispatcher and exits; it never starts a second application.
- **`note-it new`** is the explicit way to create an additional note.

## Tasks

- **Markdown Task Lists:**
  - Typing `- [ ] ` creates a task; `- [x] ` or `- [X] ` creates a completed one.
  - Real editor nodes with square checkboxes, not text characters, nested up to any depth with
    `Tab` / `Shift+Tab`.
- **Completion:**
  - Completing a task ticks the box, strikes the text through, and records the moment, shown
    discreetly as `Concluído dd/MM/aaaa HH:mm`.
  - Reopening a task clears the date; completing it again records a new one.
  - A task written elsewhere as `- [x]` loads as completed with no date invented for it.

## Smart Blocks

Four block kinds, all stored as ordinary Markdown and all reachable from the
**Blocos** section of the note's own menu — no second toolbar was introduced.

- **Bloco de código:** a fenced block whose language survives every round trip
  untouched, including one nothing here can highlight. Sixteen grammars are
  loaded: `plaintext`, `bash`, `javascript`, `typescript`, `json`, `html`/`xml`,
  `css`, `markdown`, `python`, `rust`, `c`, `cpp`, `java`, `sql`, `yaml` and
  `toml`, plus the aliases each already answers to (`js`, `ts`, `py`, `sh`,
  `cpp`…). The language is chosen from **Blocos → Linguagem**, which shows the
  current one and is offered only where it means something.
- **Callout:** `> [!NOTE]`, `> [!TIP]`, `> [!IMPORTANT]`, `> [!WARNING]` and
  `> [!CAUTION]` — GitHub's alert syntax, which Obsidian reads too. A callout is
  a blockquote carrying a kind, so it holds several paragraphs, lists and nested
  blocks without a content model of its own. An unrecognised kind is left as the
  blockquote it already is, with its text untouched.
- **Citação:** the plain blockquote, which stays independent of callouts and is
  never promoted into one. Indented, ruled down the side, set in the note's own
  text colour rather than dimmed and italicised.
- **Comentário:** an `<!-- ... -->` kept in the file and shown as a small
  labelled block. It is editable — a comment the window never showed would be one
  nobody could remove — but it is not part of the note's text.

Markdown typed by hand still works: `` ``` `` opens a code block and `> ` opens a
quote, exactly as before.

Highlighting is **presentation only**: editor decorations over the same
characters, never markup in the file. It is not applied to a block with no
language, and never guessed for one whose language is unknown — an unhighlighted
block is the honest answer, not a colour scheme picked by resemblance. Typing
outside a code block does not re-run it, so a note full of code stays as light
to edit as any other.

Every colour a smart block paints — seven syntax tokens and five callout accents
— is defined once for the pale papers and once for the dark one, and each clears
4.5:1 against the paper it is actually drawn on. The grounds are tinted from the
paper rather than being surfaces of their own, so a note keeps its colour under
every block.

## View Controls

- **Zoom (`Ctrl+=` / `Ctrl+-` / `Ctrl+0`):**
  - Scales the note's content between 75% and 200% in 10% steps, without changing the window size,
    the Markdown, or the note's modification date. The header bar keeps its size.
  - Persisted per note in `state.json`; notes without a stored zoom open at 100%.
- **Tema (menu):**
  - Sistema / Claro / Escuro, applied at once to every open note and persisted globally.
- **Layer (`Ctrl+Shift+Space`):**
  - Switches between **Sempre no topo** (above other windows) and **Área de trabalho** (behind
    them, still open). This is the same application-wide switch as `note-it toggle`.
- **Collapse (`Ctrl+Shift+M`):**
  - The same action as the menu entry, reducing the note to its header bar and back. It applies to
    the focused note alone.
- **Collapse everything (`note-it toggle-collapse-all`):**
  - Collapses every note still expanded, and expands them all once they are all collapsed. Each
    note keeps its own collapsed flag and expanded size.
- **A collapsed note expands when clicked:**
  - Clicking anywhere on the bar restores the previous size in place. The close button still
    closes, dragging the bar still moves it, and the `☰` button expands the note and opens its menu
    in a single click.

## Editing Experience

- **Rich WYSIWYG Formatting:**
  - Paragraphs and Headings (H1, H2, H3)
  - Bold, Italic, Underline (`<u>`)
  - Semantic text color (`<span data-note-it-color="...">`) from a compact palette
  - Highlight marker (`<mark data-note-it-highlight="...">`) from a compact palette, always drawn
    with a dark foreground so highlighted text stays readable on every paper colour
  - Discrete text sizes (12–32 px) applied to a selection, independent of headings and of the zoom
  - Bullet lists and numbered lists
  - Interactive checklists (`- [ ]` / `- [x]`)
  - Typing `->` becomes a real `➜`, stored as the character itself rather than relying on a font
    with ligatures, and left untouched inside inline code and code blocks
  - Blockquotes and inline code / code blocks
- **Font Scaling:**
  - The note's base font size is stored in its front matter and applied when the note loads.
    `Ctrl+=` / `Ctrl+-` drive the view zoom rather than this base size.
- **Paper Themes:**
  - 7 curated soft pastel paper colors: Yellow, Blue, Green, Pink, Purple, Gray, Black (with high-contrast light text).
- **Keyboard Shortcuts:**
  - `Ctrl+N` to create a new note in cascade.
  - `Ctrl+W` to save and dismiss current note.

## Storage & Reliability

- **Atomic Autosave:**
  - Debounced write (300 ms) via temporary file replacement and directory sync to prevent data corruption.
  - Close and `Ctrl+W` send the latest editor content in one save-and-close request; the window closes only after persistence succeeds.
- **Transactional Flush on Hide and Quit:**
  - `note-it hide` and `note-it quit` explicitly request latest buffer content from all active WebViews, cancel debounces, and await atomic write confirmation for every note before destroying surfaces or exiting.
  - A missing, expired, or invalid WebView response is a flush failure; the host never substitutes its potentially stale in-memory document as a successful confirmation.
  - If any note fails to confirm or save, the operation aborts: hide keeps every surface open in the previous mode, and quit keeps the daemon running. Without confirmation of current WebView content, neither operation destroys surfaces or exits.
- **Standard YAML Front Matter:**
  - Note ID, paper colour, paper type, pattern intensity, font size, and timestamps stored cleanly
    in note headers.
  - `created_at` is fixed at creation; `updated_at` follows content edits only, not appearance or
    window changes. A note without timestamps still opens and reports them as unknown.
  - Visiting a note is not editing it: opening and closing, summoning, hiding, showing or quitting
    without changing the text leaves `updated_at` alone, and the file is not rewritten at all.
