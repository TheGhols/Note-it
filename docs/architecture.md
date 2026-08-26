# Note-it Architecture

## Architectural Overview

Note-it separates native system integration from document editing through a clean, decoupled architecture:

```text
┌────────────────────────────────────────────────────────┐
│                   Rust Native Host                     │
│  (GTK4 + gtk4-layer-shell + WebKitGTK 6.0 + Storage)   │
│                                                        │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │ Single-Inst. │  │ Layer Shell  │  │ XDG Storage  │  │
│  │ IPC / Daemon │  │ Manager      │  │ (MD / State) │  │
│  └──────────────┘  └──────────────┘  └──────────────┘  │
└───────────────────────────▲────────────────────────────┘
                            │ WebKit IPC Bridge
                            │ (JSON Messages)
┌───────────────────────────▼────────────────────────────┐
│                  TypeScript Webview                    │
│            (Vite + Tiptap / ProseMirror)               │
│                                                        │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │ WYSIWYG Doc  │  │ Markdown     │  │ HTML Sanit.  │  │
│  │ Editor       │  │ Serializer   │  │ & Whitelist  │  │
│  └──────────────┘  └──────────────┘  └──────────────┘  │
└────────────────────────────────────────────────────────┘
```

## Backend Components (Rust)

- `main.rs`: Entry point and single-instance CLI dispatcher (`gtk::Application`).
- `app.rs`: Application state, lifecycle coordination, and IPC handling.
- `cli.rs`: Command line parsing (`--background`, `new`, `toggle`, `show`, `hide`, `quit`).
- `model.rs`: Note data models and metadata parsing.
- `storage.rs`: XDG directory resolution, Markdown disk I/O, and atomic file saving.
- `state.rs`: Window geometry persistence (`$XDG_STATE_HOME/note-it/state.json`).
- `settings.rs`: Application configuration (`$XDG_CONFIG_HOME/note-it/config.toml`).
- `layer_shell.rs`: Wayland Layer Shell initialization, anchors, layers, and focus management.
- `note_window.rs`: GTK4 window wrapper embedding WebKitGTK 6.0 webviews.
- `webview_bridge.rs`: Bidirectional messaging between Rust host and TypeScript webview.

## Frontend Components (TypeScript / Vite / Tiptap)

- `ui/src/main.ts`: Webview entry point and bridge bootstrap.
- `ui/src/editor/`: Tiptap editor configuration, extensions, keybindings, and toolbar.
- `ui/src/markdown/`: Markdown parser, serializer, and round-trip converters.
- `ui/src/bridge/`: Native message handlers for load, save, theme, and font changes.
- `ui/src/styles/`: Minimalist themes, paper color definitions, and layout styling.
