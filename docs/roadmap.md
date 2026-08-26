# Note-it Roadmap

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
