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
  - Entries: **Cor da nota**, **Tamanho do texto**, **Cor do texto**, **Marca-texto**, **Zoom**,
    **Camada**, and **Recolher nota** / **Expandir nota**.
  - The menu shows the current zoom and the active layer, so neither depends on knowing a shortcut.
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

## View Controls

- **Zoom (`Ctrl+=` / `Ctrl+-` / `Ctrl+0`):**
  - Scales the note's content between 75% and 200% in 10% steps, without changing the window size,
    the Markdown, or the note's modification date. The header bar keeps its size.
  - Persisted per note in `state.json`; notes without a stored zoom open at 100%.
- **Layer (`Ctrl+Shift+Space`):**
  - Switches between **Sempre no topo** (above other windows) and **Área de trabalho** (behind
    them, still open). This is the same application-wide switch as `note-it toggle`.
- **Collapse (`Ctrl+Shift+M`):**
  - The same action as the menu entry, reducing the note to its header bar and back.

## Editing Experience

- **Rich WYSIWYG Formatting:**
  - Paragraphs and Headings (H1, H2, H3)
  - Bold, Italic, Underline (`<u>`)
  - Semantic text color (`<span data-note-it-color="...">`) from a compact palette
  - Highlight marker (`<mark data-note-it-highlight="...">`) from a compact palette
  - Discrete text sizes (12–32 px) applied to a selection, independent of headings and of the zoom
  - Bullet lists and numbered lists
  - Interactive checklists (`- [ ]` / `- [x]`)
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
  - Note ID, paper color, font size, and timestamps stored cleanly in note headers.
  - `created_at` is fixed at creation; `updated_at` follows content edits only, not appearance or
    window changes. A note without timestamps still opens and reports them as unknown.
