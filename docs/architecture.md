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
- `trash.rs`: recoverable deletion. Moves a note file between `notes/` and `trash/` and lists what
  is in there; it never reads, parses or rewrites a note, which is why a note with damaged front
  matter can still be deleted and recovered. See ADR-028.
- `backup.rs`: local snapshots. Copies `notes/`, `trash/`, `assets/`, `config.toml` and
  `state.json`, plus optional `study.json`, into `backups/<timestamp>/`, atomically, with retention. Pure functions decide when
  one is owed, so the 24-hour rule is tested without waiting a day. `assets/` is a tree rather than
  a flat directory, so it has a copy of its own — strict and fail-closed, validated by the same
  parser the asset scheme uses, and never a general recursive descent. See ADR-029 and ADR-032.
- `study.rs`: the versioned `study.json` model and Ladder-v1 scheduler. It accepts only opaque
  SHA-256 review keys and closed ratings, chooses due timestamps and local activity dates on the
  host clock, and returns a new live state only after the atomic write succeeds. It contains no
  flashcard parser and no note content.
- `state.rs`: Window geometry persistence (`$XDG_STATE_HOME/note-it/state.json`). Each note's entry
  also carries its Timer/Pomodoro, for the same reason it carries the zoom: it is state of the
  application, not of the document.
- `assets.rs`: images a note owns. Where their bytes live, what they are called, and what a request
  for one is allowed to resolve to. The format is decided by the bytes rather than by a filename, the
  name is minted here rather than taken from the reader's disk, and a request is two `Uuid`s parsed
  as `Uuid`s — so nothing outside `assets/<note>/` is nameable. See ADR-032.
- `autopaste.rs`: the clipboard capture policy — armed or not, whose note, which generation, which
  read is stale. No GDK, no clipboard and no text: it decides, and `app.rs` owns the `GdkClipboard`
  and carries the words straight from the read callback to the target note. Testable without a
  graphical session, which is why the rules live here rather than in the signal handler. See
  ADR-031.
- `timer.rs`: the timer's stored shape and the words of its notification. The host keeps the record
  and rings the bell; it does not run the countdown. Everything arriving from `state.json` or from
  the page goes through `NoteTimerState::sanitize`, so a state that claims to be running with no
  instant to run to comes back idle. See ADR-030.
- `settings.rs`: Application configuration (`$XDG_CONFIG_HOME/note-it/config.toml`).
- `layer_shell.rs`: Wayland Layer Shell initialization, anchors, layers, and focus management.
- `note_window.rs`: GTK4 window wrapper embedding WebKitGTK 6.0 webviews.
- `webview_bridge.rs`: Bidirectional messaging between Rust host and TypeScript webview.

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
