# Note-it Architecture

## Architectural Overview

Note-it has one headless domain/persistence authority and adapters around it. The desktop adapter
adds native system integration and embeds the TypeScript editor; future CLI and MCP adapters must
call the same Core instead of recreating its rules.

```text
                    ┌───────────────────────────────┐
                    │ noteit-core (headless crate)  │
                    │ domain + XDG persistence      │
                    └───────────────▲───────────────┘
                                    │
                     the desktop adapter calls Core
                                    │
┌───────────────────────────────────┴───────────────────┐
│ Rust desktop host: GTK4 + layer-shell + WebKitGTK    │
│ single instance, lifecycle, windows and IPC bridge    │
└───────────────────────────▲───────────────────────────┘
                            │ JSON messages
┌───────────────────────────▼───────────────────────────┐
│ TypeScript WebView: Vite + Tiptap / ProseMirror      │
│ editor, Markdown serializer and HTML sanitization     │
└───────────────────────────────────────────────────────┘
```

The dependency direction is enforced by Cargo: the desktop package depends on `noteit-core`, and
`noteit-core/Cargo.toml` contains no desktop dependency. `scripts/check-core-boundary` also fails if
GTK, GDK, WebKitGTK, layer-shell, Wayland or Niri enters the Core dependency tree.

## Core Components (`noteit-core`, Rust)

`NoteItCore` is the small adapter-facing facade. It currently exposes canonical operations for
listing, reading and searching live notes, listing trash, and loading Study state. Its write and
lifecycle consumers use the same `StorageManager` held by that facade, so there is still one
implementation of atomic writes, recency, trash, backup and Study persistence.

- `noteit-core/src/model.rs`: Note data models and metadata parsing. `split_front_matter` and
  `body_of` are shared with search, so "the note's body" means the same thing everywhere.
- `noteit-core/src/storage.rs`: XDG directory resolution, Markdown disk I/O, atomic saving and the
  existing storage operations used by the GUI and future adapters.
- `noteit-core/src/search.rs`: accent folding, matching, snippets, labels and ordering — pure
  functions over `(Uuid, &str)`.
- `noteit-core/src/trash.rs`: recoverable deletion and read-only trash listing. See ADR-028.
- `noteit-core/src/backup.rs`: local snapshots, retention and manifest policy. See ADR-029 and
  ADR-032.
- `noteit-core/src/study.rs`: the versioned `study.json` model and Ladder-v1 scheduler.
- `noteit-core/src/assets.rs`: image validation, identifiers, storage references and import rules.
- `noteit-core/src/autopaste.rs` and `timer.rs`: headless state machines and policies; clipboard and
  notification integration stay in the desktop host.
- `noteit-core/src/settings.rs` and `state.rs`: versioned application configuration and operational
  state, with atomic persistence but no windowing dependency.
- `atomic_file.rs` and `visible_text.rs` are private implementation modules shared by those public
  capabilities.

Core tests use only temporary synthetic stores. The canonical headless gate is:

```bash
env -u DISPLAY -u WAYLAND_DISPLAY cargo test -p noteit-core
```

## Desktop Adapter Components (`src`, Rust)

- `main.rs`: Entry point and single-instance CLI dispatcher (`gtk::Application`).
- `app.rs`: Application state, lifecycle coordination, and IPC handling.
- `cli.rs`: Command line parsing (`--background`, `new`, `toggle`, `show`, `hide`, `quit`).
- `layer_shell.rs`: Wayland Layer Shell initialization, anchors, layers, and focus management.
- `note_window.rs`: GTK4 window wrapper embedding WebKitGTK 6.0 webviews.
- `webview_bridge.rs`: Bidirectional messaging between Rust host and TypeScript WebView. Message
  types reuse Core domain types, while the actual WebView send path remains desktop-specific.

## Frontend Components (TypeScript / Vite / Tiptap)

- `ui/src/main.ts`: Webview entry point and bridge bootstrap.
- `ui/src/editor/`: Tiptap editor configuration, extensions, keybindings, and toolbar.
- `ui/src/markdown/`: Markdown parser, serializer, and round-trip converters.
- `ui/src/flashcards/`: the single ProseMirror flashcard definition and ephemeral review session.
- `ui/src/study/`: semantic SHA-256 identities, one reusable on-demand Tiptap catalog parser, and
  pure due/heatmap/streak projections. `ui/src/ui/studyHub.ts` and the existing
  `flashcardPanel.ts` render the global catalog and scheduled sitting inside the current WebView.
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
  `ui/src/editor/linkPaste.ts` is the URL-over-selection paste, gated by the application's own
  link allowlist.
- `ui/src/ui/searchPalette.ts`, `ui/src/ui/findBar.ts` and `ui/src/ui/trashPanel.ts`: the three
  panels. All live in the page rather than in a second window, own their keys, and are not part of
  the document. `ui/src/ui/status.ts` is the line at the foot of the note that reports what a data
  action did; it is not a dialog and takes nothing from the reader.
- `ui/src/markdown/assetReference.ts`: what a note is allowed to say about a picture — the managed
  reference format, the width limits, the three alignments, and the one function that turns a stored
  reference into something the page may load. A markdown concern rather than an editor one, because
  the sanitizer recognises it on the way in and the editor writes it on the way out.
- `ui/src/editor/image.ts` and `ui/src/editor/imageView.ts`: the image node and its own interface —
  the two stored forms and the round trip between them, and the handles, alignment controls and
  single-transaction resize that never take the focus or move the selection.
- `ui/src/flashcards/`: a projection of the live ProseMirror document. `extract.ts` recognises the
  inline and structural syntax, keeps sides as document fragments and expands reversible sources
  into review items; `session.ts` owns only the ephemeral order, cursor and reveal state. The editor
  plugin in `ui/src/editor/flashcardMark.ts` paints delimiters and holds the live count without a
  document transaction, while `ui/src/ui/flashcardPanel.ts` renders snapshot fragments with the
  note's `DOMSerializer` and never receives an editor or dispatch function.
- `ui/src/capture/autoPaste.ts`: what a note becomes when a capture arrives — the plain-text split
  ProseMirror itself uses for `text/plain`, the three delimiters, and the single transaction that
  appends at the end without taking focus, moving the selection or scrolling. It reads no clipboard:
  the page has no part in observing one.
- `ui/src/timer/`: the countdown itself, independent of the DOM — `engine.ts` (the state machine
  over a deadline, with the clock injected), `format.ts` (`MM:SS` / `H:MM:SS` and the words for each
  state) and `controls.ts` (which button applies in which state, as a value rather than as four
  branches inside a handler). It knows nothing about the header or the popover;
  `ui/src/ui/timerPanel.ts` is the only thing that joins the two, and it owns the single pending
  redraw rather than an interval.
- `ui/src/bridge/`: Native message handlers for load, save, theme, and font changes.
- `ui/src/styles/`: Minimalist themes, paper color definitions, and layout styling.

## Where Search Lives

Search is a capability of the domain, not of the interface:

```text
NoteItCore::search_notes
   ↓ delegates to the existing StorageManager reader
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

And nothing through `Vec<SearchResult>` needs a display. A future CLI calls `NoteItCore::search_notes`
over the same storage and search implementation the GUI calls; GTK and WebKit enter only after the
result reaches the desktop adapter.
