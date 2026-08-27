# Note-it Roadmap

## Phase 0: Public Foundation (Completed)
- [x] Repository initialization, `.gitignore`, licensing, and documentation.
- [x] Rust and TypeScript build scaffolding.
- [x] Project architecture and storage specification.

## Phase 1: Vertical Slice & Markdown Integrity (Completed)
- [x] Working GTK4 + `gtk4-layer-shell` + WebKitGTK 6.0 note window.
- [x] Bidirectional IPC bridge between native host and webview editor.
- [x] Load and atomic autosave of `.md` files with YAML front matter.
- [x] ProseMirror / Tiptap 3 Markdown round-trip serializer and sanitizer.
- [x] Native Markdown code preservation (fenced blocks, inline spans, and literal syntax).
- [x] GitHub Actions CI pipeline running natively in Arch Linux container environment.

## Phase 2: Shell, Lifecycle, Layers & Geometry (Completed with Phase 2R)
- [x] Strict distinction between on-disk `.md`, `is_open` state, instantiated WebViews, and visible surfaces.
- [x] Lazy daemon lifecycle: `--background` starts with 0 WebViews created (idle ~0% CPU).
- [x] Wayland Layer Shell modes: Desktop (`bottom`), Overlay (`overlay`), and Hidden.
- [x] Dynamic single-instance CLI dispatcher (`new`, `toggle`, `show`, `hide`, `quit`).
- [x] Window drag handle (header `.drag-region`) and discrete resize handle (`.resize-handle`).
- [x] Window geometry persistence in `$XDG_STATE_HOME/note-it/state.json` (persisted only on drag/resize end).
- [x] Safe geometry clamping, cascade positioning, and multi-monitor connector fallback.
- [x] Canonical autolink policy (`https`, `http`, `mailto`) with safe non-destructive escaping.
- [x] Transactional flush protocol before `hide` and `quit` to prevent data loss from debounced edits.
- [x] End-to-end testing and validation on Niri compositor.

## Phase 3: Editor Enhancements & User Experience (In Progress)

### Phase 3.0R.1: Editor & Geometry Stabilisation (Completed)
- [x] Physical pt-BR keyboard, dead keys, and IME composition preserved inside the WebView.
- [x] Markdown formatting shortcuts including `Ctrl+R` strikethrough.
- [x] Sub-pixel accurate drag and resize with the final `pointerup` delta applied.
- [x] Window geometry persisted on gesture end and restored on reopen.

### Phase 3.1: Note Chrome, Settings Menu, Collapse & Information (Completed)
- [x] Header `☰` settings popover replacing the direct colour dot.
- [x] Paper colour palette moved inside the menu, with persistence preserved.
- [x] Collapse / expand reducing the note to its header bar, with the expanded geometry restored.
- [x] Collapsed state persisted across restarts, with backward-compatible state migration.
- [x] Note creation and modification dates on header hover, formatted in pt-BR.
- [x] Pointer gesture lifecycle hardened: one captured pointer per gesture, no geometry change
      without an active gesture.

### Phase 3.2: Tasks, View Controls & Inline Formatting (Completed)
- [x] Host surface backed with the note's paper colour, so a fast resize no longer exposes a dark
      strip before the WebView repaints.
- [x] Markdown task lists with square checkboxes, nesting, and automatic strikethrough.
- [x] Per-task completion timestamps that travel with their task and are never invented.
- [x] View zoom (75–200%) persisted per note, independent of the document.
- [x] Inline text size, text colour and highlight, applied from the settings menu.
- [x] `Ctrl+Shift+M` collapse, `Ctrl+Shift+Space` layer switch, `Ctrl+Shift+>` / `Ctrl+Shift+<`
      text size, all routed through the single keyboard controller.

### Phase 3.2R: Summon, Reopen & Typography (Completed)
- [x] `note-it` summons the running instance from any focused application, raising a desktop-layer
      note temporarily without losing the stored preference.
- [x] Closing the last note no longer strands it: the note used last is reopened on the next summon.
- [x] Typing `->` produces a real `→`, outside code.

### Phase 3.3: Multi-note Collapse & UX Refinements (Completed)
- [x] `note-it toggle-collapse-all` for every note, with `Ctrl+Shift+M` still per-note.
- [x] A collapsed note expands when clicked, and `☰` expands and opens the menu in one click.
- [x] The settings menu is no longer clipped on a collapsed note.
- [x] `->` produces the heavier `➜`, readable at every text size.
- [x] Highlighted text is readable on every paper colour, including the dark one.

### Phase 3.4: Editor Enhancements (Planned)
- [ ] Contextual floating bubble toolbar for formatting.
- [ ] Visual polish, paper textures, and typography adjustments.

## Phase 4: Packaging & Distribution (Planned)
- [ ] Arch Linux PKGBUILD for AUR.
- [ ] Release automation and binary artifacts.
- [ ] v0.1.0 release.
