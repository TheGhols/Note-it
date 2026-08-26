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

## Window Positioning & Interactions

- **Drag & Resize:**
  - Header drag region (`.drag-region`) for moving post-its freely across the workspace.
  - Discrete bottom-right resize handle (`.resize-handle`) with min-dimension limits (`220x160` px).
  - Geometry persisted to `$XDG_STATE_HOME/note-it/state.json` exclusively on gesture end (zero disk I/O during active dragging/resizing).
- **Safe Geometry Clamping & Monitor Fallback:**
  - Clamping guarantees notes stay visible on-screen even after monitor resolution changes.
  - Multi-monitor connector detection with graceful fallback if a display is disconnected.
- **Smart Cascade Placement:**
  - New notes cascade incrementally across the screen grid.

## Editing Experience

- **Rich WYSIWYG Formatting:**
  - Paragraphs and Headings (H1, H2, H3)
  - Bold, Italic, Underline (`<u>`)
  - Semantic text color (`<span data-note-it-color="...">`)
  - Highlight marker (`<mark data-note-it-highlight="...">`)
  - Bullet lists and numbered lists
  - Interactive checklists (`- [ ]` / `- [x]`)
  - Blockquotes and inline code / code blocks
- **Font Scaling:**
  - `Ctrl++` and `Ctrl+-` scale the active note's base font size (persisted in front matter).
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
  - If any note fails to save, the operation aborts and in-memory contents and windows are preserved.
- **Standard YAML Front Matter:**
  - Note ID, paper color, font size, and timestamps stored cleanly in note headers.
