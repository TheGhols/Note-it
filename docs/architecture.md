# Note-it Architecture

## Architectural Overview

Note-it has one headless domain/persistence authority and adapters around it. The desktop adapter
adds native system integration and embeds the TypeScript editor; future CLI and MCP adapters must
call the same Core instead of recreating its rules.

```text
                         ┌───────────────────────────────┐
                         │ noteit-core (headless crate)  │
                         │ domain + XDG persistence      │
                         └───────▲───────────────▲───────┘
                                 │               │
                     desktop adapter calls Core  │ headless CLI calls Core
                                 │               │
 ┌───────────────────────────────┴────────┐     ┌┴──────────────────────────────┐
 │ note-it GUI: GTK4 + layer-shell + WebKit│     │ noteit CLI: headless binary   │
 │ single instance, lifecycle, windows    │     │ pure terminal / script / agent│
 └───────────────────────────────▲────────┘     └───────────────────────────────┘
                                 │ JSON messages
 ┌───────────────────────────────▼────────┐
 │ TypeScript WebView: Vite + Tiptap      │
 │ editor, Markdown serializer, sanitizer │
 └────────────────────────────────────────┘
```

The dependency direction is enforced by Cargo: both the desktop package (`note-it`) and the CLI
package (`noteit-cli`) depend on `noteit-core`, while `noteit-core` has zero desktop or CLI
dependencies. `scripts/check-core-boundary` and `scripts/check-cli-boundary` prevent GUI libraries
(GTK, GDK, WebKitGTK, layer-shell, Wayland, Niri) from entering either headless component.

## Core Components (`noteit-core`, Rust)

`NoteItCore` is the small adapter-facing facade. It currently exposes canonical operations for
listing, reading and searching live notes, deriving metadata catalogs, listing trash, loading Study state,
and purely resolving store paths (`StorePaths`). Its write and lifecycle consumers use the same
`StorageManager` held by that facade, so there is still one implementation of atomic writes, recency,
trash, backup and Study persistence.

- `noteit-core/src/model.rs`: Note data models, metadata parsing, and `NoteSummary` projection. `split_front_matter` and
  `body_of` are shared with search, so "the note's body" means the same thing everywhere.
- `noteit-core/src/filter.rs`: typed `NoteFilter` with tag/property AND matching via `semantic_identity`, and safe `NoteSelectorError`.
- `noteit-core/src/task.rs`: one task scanner shared by reading and writing — checkbox states, depth
  hierarchy, fenced-code exclusion, ISO 8601 `completed_at` extraction, the optimistic `TaskRef`, and
  the line rewrite that completes or reopens a task. A fake task inside a fence is invisible to both,
  because there is only one scanner.
- `noteit-core/src/write.rs`: every mutation as a typed domain operation — `WriteOperation`,
  `NoteMutation`, `WriteOutcome`, `WriteError` — plus `apply_over_live_body`, the rule for applying a
  mutation on top of text an editor is holding but has not saved. Both adapters run this one
  implementation.
- `noteit-core/src/coordination.rs`: the writer lease. One advisory `flock` per store, in a runtime
  directory named after that store, with ownership and permission checks that fail closed.
- `noteit-core/src/control.rs`: the private control protocol — length-prefixed JSON over a local Unix
  socket, versioned and bounded. **Not a public interface**; see ADR-038.
- `noteit-core/src/hashing.rs`: one deterministic, documented digest (FNV-1a 64) for the store key and
  the task reference. Never `DefaultHasher`, whose stability is not promised.
- `noteit-core/src/warning.rs`: typed, structured non-fatal read anomalies (`ReadWarning`, `ReadBatch<T>`) returned by Core operations without terminal printing.
- `noteit-core/src/metadata.rs`: validated Tags and textual Properties, semantic identity shared
  with search folding, deterministic colour buckets and typed catalog entries. Adapters never need
  `serde_yaml::Value`.
- `noteit-core/src/storage.rs`: pure XDG directory resolution (`StorePaths`), strictly read-only store opening (`open_read_only`),
  Markdown disk I/O, atomic saving and the storage operations used by GUI and CLI adapters.
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

Core tests use only temporary synthetic stores. The canonical headless gates are:

```bash
env -u DISPLAY -u WAYLAND_DISPLAY cargo test -p noteit-core
scripts/check-core-boundary
```

## CLI Adapter Components (`noteit-cli`, Rust)

- `main.rs`: Entry point for the `noteit` binary, dispatching arguments and mapping standard exit codes.
- `cli.rs`: Command line parsing using Clap with PT-BR primary commands and international aliases (`listar`/`list`, `ler`/`read`, `buscar`/`search`, `tags`, `propriedades`/`properties`, `tarefas`/`tasks`, `lixeira`/`trash`, `status`, `ajuda`/`help`, `versao`/`version`).
- `output.rs`: Terminal presentation, ANSI styling, NO_COLOR/non-TTY detection, and terminal security sanitization (`sanitize_for_terminal`).
- `authority.rs`: the decision of who writes. Takes the writer lease when it is free and writes
  through the Core; when it is held, sends the change to whoever holds it over the private socket;
  when it is held and unreachable, fails closed and changes nothing. Never falls back to writing
  around another writer.
- `lib.rs`: Programmatic interface (`run_with_args`), filter parsing, Core dispatch, standard exit
  codes, and standard input handling for `--stdin`.

The CLI binary has zero graphical dependencies and is tested headless:

```bash
env -u DISPLAY -u WAYLAND_DISPLAY -u DBUS_SESSION_BUS_ADDRESS cargo test -p noteit-cli
scripts/check-cli-boundary
```

## Desktop Adapter Components (`src`, Rust)

- `main.rs`: Entry point and single-instance CLI dispatcher (`gtk::Application`).
- `app.rs`: Application state, lifecycle coordination, and IPC handling.
- `cli.rs`: Command line parsing (`--background`, `new`, `toggle`, `show`, `hide`, `quit`).
- `layer_shell.rs`: Wayland Layer Shell initialization, anchors, layers, and focus management.
- `note_window.rs`: GTK4 window wrapper embedding WebKitGTK 6.0 webviews.
- `webview_bridge.rs`: Bidirectional messaging between Rust host and TypeScript WebView. Message
  types reuse Core domain types, while the actual WebView send path remains desktop-specific.
- `write_authority.rs`: the desktop instance as the store's writer. `claim` takes the lease, binds and
  narrows the socket, and returns a `WriteAuthority` **only on complete success**; `AppContext` holds
  that by value, so a running instance that does not own its store is not a state the program can
  describe. Startup refuses rather than degrading — see ADR-039. `serve` then runs the external-write
  pipeline: freeze the editor, collect its live text, mutate, commit, adopt, move the generation on,
  hand the committed note back, and wait for the page to say it took it.

### The write path when a note is open

```text
noteit adicionar        lease held by the desktop instance
      │                        │
      └── control socket ──────┤
                               ├─ 1. refuse if hiding, quitting or deleting
                               ├─ 2. freeze the editor  ── then ──▶ read its text
                               ├─ 3. fold that text into the committed document
                               ├─ 4. apply the mutation to *that*
                               ├─ 5. commit through the atomic writer
                               ├─ 6. adopt it, generation += 1
                               ├─ 7. hand it back to the page and unfreeze
                               └─ 8. wait for the page to say it adopted it
```

Step 2 is in that order and no other: reading first leaves a gap in which a keystroke lands, and that
keystroke is then written over. Step 6 is what makes every message still in flight from the previous
run refusable. Step 8 is the page's own word — `ExternalWriteApplied`, naming the note, the request
and the generation — because a script having evaluated says nothing about whether a document was
adopted. Everything from step 5 onwards is past the commit point, so step 8 can only decide whether
the answer carries a warning; it can never turn a completed write into a failure. See ADR-038 and
ADR-039.

Before any of it: the store is claimed. There is no window, no document and no autosave until this
process is its one writer, and if it cannot be, it says so and exits.

```text
desktop startup
   → prepare coordination   ─┐
   → acquire writer lease    ├─ any failure: release, explain, exit non-zero
   → bind + narrow socket   ─┘
   → build the application
```

## Frontend Components (TypeScript / Vite / Tiptap)

- `ui/src/main.ts`: Webview entry point and bridge bootstrap.
- `ui/src/bridge/externalWrite.ts`: the page's half of an external write — freeze, snapshot, adopt,
  and a queue that holds every edit arriving meanwhile so none is lost.
- `ui/src/editor/documentLock.ts`: one ProseMirror `filterTransaction` gate. While a write is in
  flight nothing changes the document — not typing, not a command, not a plugin. The document is
  released by the host and only by the host: the page has no timeout that could hand it back while a
  commit is still in flight.

### When the page may edit again

Once the snapshot has gone out, exactly two answers release the document — and there is a third that
does not:

| host says | file | page | outcome |
| --- | --- | --- | --- |
| `AbortExternalWrite` | unchanged | still matches the file | thaw, drain the queue, same generation |
| `ApplyExternalDocument`, adopted | changed | now the committed text | thaw, drain, new generation, `ExternalWriteApplied` |
| `ApplyExternalDocument`, **not** adopted | changed | stale | **stays held**: no thaw, no drain, old generation, `ExternalWriteApplyFailed` |

The third row is the one worth stating plainly. The write is on disk and is reported as committed with
a `ui_sync_warning`; the window is not released, because a released window would be editing against a
generation the host has already moved past and every save it made would be refused — work typed and
silently lost. The note says so, and reopening it is the recovery. See ADR-040.
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
- `ui/src/ui/metadataPanel.ts`: the single Tags/Properties editor and responsive tag strip. It
  handles typed values only, renders with `textContent`/`value`, and adopts a draft only after the
  host acknowledges the Core commit.
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

## Semantic Metadata Flow

```text
metadata panel confirms a typed draft + current editor Markdown
  → UUID-addressed WebView message
  → Core validates NoteMetadata
  → clone the in-memory NoteDocument candidate
  → include pending text (and touch updated_at only if text differs)
  → StorageManager::save_note_atomic (backup → temp → rename commit)
  → adopt the committed candidate
  → acknowledge the exact committed MetadataView
```

`note_it` remains application-owned. `tags`, `properties`, and unknown top-level YAML values live
in the same front matter, but YAML itself never crosses the bridge. Unknown values are retained as
Core persistence detail; comments, anchors and original formatting cannot be represented by
`serde_yaml` and may normalize when an actual content/appearance/metadata save reserializes the
file. An untouched open/close performs no write and therefore stays byte-identical.

Catalogs are derived on demand by scanning `notes/` only. There is no `tags.json`, database or
cache to become stale; trash disappears from a catalog because its file is not live, and restore
makes it return naturally.
