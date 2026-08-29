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
- `model.rs`: Note data models and metadata parsing. `split_front_matter` and `body_of` are shared
  with search, so "the note's body" means the same thing to both.
- `storage.rs`: XDG directory resolution, Markdown disk I/O, and atomic file saving.
  `read_note_bodies_by_recency` is what search reads: note bodies in recency order, front matter
  stripped textually, unreadable notes skipped rather than fatal.
- `search.rs`: the search core. Accent folding, matching, snippets, labels and ordering — pure
  functions over `(Uuid, &str)`, with no GTK, no WebKit and no display, so it is testable without
  starting the application and reusable by the future CLI without moving anything. See ADR-027.
- `state.rs`: Window geometry persistence (`$XDG_STATE_HOME/note-it/state.json`).
- `settings.rs`: Application configuration (`$XDG_CONFIG_HOME/note-it/config.toml`).
- `layer_shell.rs`: Wayland Layer Shell initialization, anchors, layers, and focus management.
- `note_window.rs`: GTK4 window wrapper embedding WebKitGTK 6.0 webviews.
- `webview_bridge.rs`: Bidirectional messaging between Rust host and TypeScript webview.

## Frontend Components (TypeScript / Vite / Tiptap)

- `ui/src/main.ts`: Webview entry point and bridge bootstrap.
- `ui/src/editor/`: Tiptap editor configuration, extensions, keybindings, and toolbar.
- `ui/src/markdown/`: Markdown parser, serializer, and round-trip converters.
- `ui/src/math/`: the math engine, independent of the editor — `lexer.ts`, `parser.ts`,
  `evaluate.ts`, `document.ts` (a note's lines, evaluated top-down) and `format.ts`. It knows
  nothing about ProseMirror; `ui/src/editor/math.ts` is the only thing that joins the two, reading
  lines out of the document and painting results back as decorations.
- `ui/src/units/`: the unit table and the conversion itself — `types.ts`, `registry.ts` and
  `convert.ts`. It knows nothing about parsing, about notes or about the editor: it is data plus
  arithmetic. The dependency runs one way, `math/parser.ts` → `units/registry.ts`, because the
  parser has to know what counts as a unit; nothing in `units/` refers back. That edge is also the
  boundary a future currency source must sit behind — see ADR-025.
- `ui/src/editor/find.ts`: find and replace over the live document — matching per textblock,
  highlight decorations, and `Replace All` as one ProseMirror transaction.
  `ui/src/editor/autoPaste.ts` is the URL-over-selection paste, gated by the application's own
  link allowlist.
- `ui/src/ui/searchPalette.ts` and `ui/src/ui/findBar.ts`: the two panels. Both live in the page
  rather than in a second window, own their keys, and are not part of the document.
- `ui/src/bridge/`: Native message handlers for load, save, theme, and font changes.
- `ui/src/styles/`: Minimalist themes, paper color definitions, and layout styling.

## Where Search Lives

Search is a capability of the domain, not of the interface:

```text
storage (read_note_bodies_by_recency)
   ↓  (Uuid, body) pairs — front matter already gone
search.rs (fold → match → snippet → order → cap)
   ↓  Vec<SearchResult>
webview_bridge (SearchResults { request_id, results })
   ↓
searchPalette.ts (renders, and asks by note_id)
```

Two properties of that arrangement are deliberate.

The frontend never names a file. It receives `note_id` values the host generated and sends one
back; there is no message in the bridge that carries a path, so there is nothing to traverse.

And nothing in the chain needs a display. `search_notes` takes an iterator of `(Uuid, &str)`, so
the CLI Phase 4 will bring can call the same function over the same storage reader without any of
this file moving — which is the whole reason it is not a method on a window.
