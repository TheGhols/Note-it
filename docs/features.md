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

## Editing Experience

- **Rich WYSIWYG Formatting:**
  - Paragraphs and Headings (H1, H2, H3)
  - Bold, Italic, Underline (`<u>`)
  - Semantic text color (`<span data-note-it-color="...">`)
  - Highlight marker (`<mark data-note-it-highlight="...">`)
  - Bullet lists and numbered lists
  - Interactive checklists (`- [ ]` / `- [x]`)
  - Blockquotes and inline code / code blocks
- **Contextual Bubble Toolbar:**
  - Floats on text selection with quick actions for styling, colors, and highlights.
- **Font Scaling:**
  - `Ctrl++` and `Ctrl+-` scale the active note's base font size (persisted in front matter).
- **Paper Themes:**
  - 7 curated soft pastel paper colors: Yellow, Blue, Green, Pink, Purple, Gray, Black (with high-contrast light text).

## Storage & Reliability

- **Atomic Autosave:**
  - Debounced write (300 ms) via temporary file replacement to prevent data corruption.
  - Close and Ctrl+W send the latest editor content in one save-and-close request; the window closes only after persistence succeeds.
- **Standard YAML Front Matter:**
  - Note ID, paper color, font size, and timestamps stored cleanly in note headers.
