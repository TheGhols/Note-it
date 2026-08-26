# Note-it Roadmap

## Mandatory window-management requirements for Phase 2

Phase 1.1 intentionally does not implement the broader window/layer lifecycle. Phase 2 must:

- distinguish a note existing on disk, an instantiated `NoteWindow`, a visible note, and a closed note;
- set `is_open=false` when Ctrl+W closes a note;
- make `restore_saved_notes` respect `is_open` correctly;
- define `note-it toggle` semantics for closed and hidden notes;
- avoid creating a WebView for every `.md` file when `--background` starts, especially for large note collections;
- define Desktop, Overlay, and Hidden lifecycle behavior definitively.

## Phase 0: Public Foundation (Current)
- [x] Repository initialization, `.gitignore`, licensing, and documentation.
- [x] Rust and TypeScript build scaffolding.
- [x] Project architecture and storage specification.

## Phase 1: Vertical Slice
- [ ] Working GTK4 + `gtk4-layer-shell` + WebKitGTK 6.0 single note window.
- [ ] Bidirectional IPC bridge between native host and webview editor.
- [ ] End-to-end load and atomic autosave of `.md` file.

## Phase 2: Rich WYSIWYG Editor
- [ ] Full formatting support (bold, italic, underline, colors, highlight, lists, checklist).
- [ ] Contextual floating bubble toolbar.
- [ ] Markdown round-trip serializer tests.

## Phase 3: Multi-Note & Geometry Management
- [ ] Multiple concurrent post-it windows.
- [ ] 7 paper color themes.
- [ ] Window drag, resize, and geometry state persistence.
- [ ] Note deletion and safe archiving.

## Phase 4: Niri & Layer Shell Workflow
- [ ] Single-instance daemon and CLI command dispatch (`toggle`, `new`, `show`, `hide`, `quit`).
- [ ] Seamless transitions between Desktop (`bottom`) and Overlay (`overlay`) layers.
- [ ] Tested and verified on Niri compositor.

## Phase 5: Packaging & Distribution
- [ ] Arch Linux PKGBUILD.
- [ ] GitHub Actions CI pipeline.
- [ ] v0.1.0 release preparation.
