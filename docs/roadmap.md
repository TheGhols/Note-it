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

## Phase 3: Editor, UX & Antinote-inspired Features (In Progress)

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

### Phase 3.4: Paper & Themes (Completed)
- [x] Five paper types per note — Liso, Pautado, Pontilhado, Quadriculado pequeno, Quadriculado
      grande — as one parameterised CSS system rather than five implementations.
- [x] Pattern intensity per note (Suave / Normal / Forte), affecting the pattern's opacity only.
- [x] Pattern ink chosen from the paper colour, so it stays visible on all seven, including the
      dark one, without ever competing with the text.
- [x] Paper type and intensity persisted in the note's front matter, without touching the content
      or the note's modification date; notes predating the fields open as plain paper.
- [x] Pattern spacing fixed in pixels, so the view zoom scales the text and leaves it alone.
- [x] Interface theme (Sistema / Claro / Escuro) stored once in `config.toml` and broadcast to
      every open note, dressing the chrome and never a note's own colour.
- [x] `--ui-*` token set separating the application's chrome from the note's paper, so a menu is
      legible over a black note and a yellow one in either theme.

### Phase 3.4R: `updated_at` Integrity (Completed)
- [x] `updated_at` moves only when the note's persisted content actually changes. Opening and
      closing, summoning, hiding, showing and quitting without editing all leave it alone.
- [x] The comparison lives in the one path every content save funnels through — autosave, the
      flush before hide and quit, and save-and-close — rather than in each caller.
- [x] A note whose content is unchanged is not rewritten at all: no temp file, no rename, no fsync.
- [x] Close and flush still report success on an identical save, so the lifecycle never stalls.
- [x] Recency, which decides the note a summon brings back, now follows the last edit rather than
      the last close. See the note under Phase 4 below.

### Phase 3.4R.1: Persistence Transactional Integrity (Completed)
- [x] A content or appearance change is prepared on a copy and adopted in memory only once
      `save_note_atomic` has confirmed the write, so the document always describes the note on disk.
- [x] A save that fails leaves the stored note and the in-memory note untouched, and the same
      payload arriving again is written for real rather than answered by the identical-content
      shortcut.
- [x] Save-and-close never finalises a close over a failed save, and closes normally once the
      retry succeeds.
- [x] The flushes before hide and quit report a failed write as a failure rather than as success.
- [x] Appearance saves — paper colour, type, intensity, font size — take the same route, so a
      failed one is not masked by the content no-op that follows a close.
- [x] A failed save removes its own temporary file instead of leaving `.tmp.*` debris behind.
- [x] Everything Phase 3.4R established is unchanged: identical persisted content writes nothing,
      `updated_at` moves only on a real edit, `created_at` never moves, and an untouched note keeps
      its file's modification time.

### Phase 3.4R.2: Commit Point (Completed)
- [x] The rename is the commit point: a save reports failure for anything before or at it, and
      success from it onwards.
- [x] A directory sync that fails after the rename is a durability warning, not a failed save, so
      memory and file can never end up describing opposite versions of a note.
- [x] Nothing tracks a missed sync: a directory sync flushes every pending entry, so the next
      successful save makes the earlier rename durable too.
- [x] What is not guaranteed is written down rather than implied — the sync is not retried and a
      save whose sync failed is not guaranteed durable.
- [x] Everything Phases 3.4R and 3.4R.1 established is unchanged.

### Phase 3.5: Smart Blocks (Completed)
- [x] Code blocks whose language survives the Markdown round trip exactly as written, including a
      fence with no language and one nothing here can highlight.
- [x] Syntax highlighting for sixteen grammars and their aliases, as editor decorations only, with
      no guessing and nothing written into the file.
- [x] Callouts in GitHub's alert syntax — NOTE, TIP, IMPORTANT, WARNING, CAUTION — holding several
      paragraphs, lists and nested blocks, and degrading to a plain blockquote when the kind is not
      recognised.
- [x] Blockquotes as their own structure, presented properly and never promoted into callouts.
- [x] Comments stored as `<!-- ... -->`, editable in the editor, and never part of the note's text.
- [x] All four reachable from the existing note menu, under one **Blocos** section rather than a
      second toolbar.
- [x] No block architecture was extracted. The four have almost nothing in common — see ADR-021.

### Phase 3.5R: Regression Audit & Stabilization (Completed)
- [x] `Ctrl+Shift+Space` toggles the layer again. The break was host-side focus, not the shortcut:
      a layer-shell window is mapped with no focus widget, so GDK received keys and dropped them
      before WebKit, and a layer change cleared the focus again. The WebView is now focused whenever
      the window is active. Isolating the three entry points is what located it — the menu and
      `note-it toggle` both worked, the keyboard did not.
- [x] Every in-note shortcut benefits: `Ctrl+N`, `Ctrl+W`, `Ctrl+R`, `Ctrl+=`/`-`/`0` and
      `Ctrl+Shift+M` were dead for the same reason whenever the note had not been clicked.
- [x] The shortcut never types a space into the note, is ignored during pt-BR composition, and
      leaves AltGr — reported as `Ctrl+Alt` — to the editor.
- [x] A note is compared and stored in one canonical spelling, so neither the newline a file is
      terminated with nor the blank line the serializer puts after a trailing block is mistaken for
      an edit. Everything Phase 3.4R established still holds: a real edit still moves `updated_at`.
- [x] A note created during a summon elevation opens on the layer the other notes are on rather
      than on the stored preference.
- [x] `state.json` and `config.toml` are written under the same commit-point rule as a note, in one
      shared atomic write: the rename commits, a directory sync failing after it is a durability
      warning, and a configuration is replaced whole or not at all.
- [x] Audited without finding a defect: the lifecycle coordinator and flush batching, the URL
      allowlist and the Markdown/HTML sanitizers, the smart blocks and their round trips, geometry
      clamping and collapse, and the summon/hide/show/restart layer transitions.

### Phase 3.5R.1: Global Layer Toggle Refinement
- [x] Niri owns the authoritative `Ctrl+Shift+Space`; the WebView shortcut is a local fallback.
- [x] The direct `toggle-layer` GAction reaches one shared layer decision without launching a
      second GTK process.
- [x] Desktop-to-Overlay promotion forces a timely Wayland commit without stealing focus from the
      normal application; the reverse transition stays live and does not re-present the surface.
- [x] Layer persistence is debounced and reads current state, while lifecycle commits retain the
      existing atomic durability guarantees.
- [x] Auto-repeat is suppressed for discrete note commands and for the Niri binding.

### Phase 3.6: Math Engine (Completed)
- [x] Contextual inline calculation, evaluated as the note is written: a line beginning with `=`
      shows its result beside it, and a `nome := expressão` line declares a value the lines below
      it can use.
- [x] Percentages in the forms people actually write — `10% de 200`, `200 + 10%`, `200 - 10%` —
      with the contextual reading tied to a `%` written on the line rather than to a value that
      once came from one.
- [x] Variables local to the note, resolved top-down, so a variable exists from its declaration
      downwards and cycles are impossible without a resolver to prevent them.
- [x] Reactive results: the whole note is re-evaluated on every document change, so changing one
      declaration moves every result under it with no dependency tracking to go stale.
- [x] `sum`, `avg` and `count` over the block of consecutive calculation lines directly above them.
- [x] Results are ProseMirror decorations and never content: nothing is written into the `.md`,
      `updated_at` does not move for a recalculation, and reopening a note recomputes it.
- [x] A parser with no evaluator behind it — no `eval`, no `Function`, no property access, no call
      syntax — and no new dependency of any kind.

### Phase 3.7: Conversions (Completed)
- [x] Unit conversions written as `= 10 km em m`, evaluated as the note is written and shown as a
      decoration beside the line, exactly as a calculation is.
- [x] Eight dimensions, all deterministic and offline: length, mass, volume, temperature, time,
      area, digital data and speed. Every spelling is listed in `docs/features.md`.
- [x] The left-hand side is a full math-engine expression, so parentheses, arithmetic and variables
      all feed a conversion.
- [x] Temperature as scales with different zeroes rather than a factor, and area as its own unit
      rather than a length with an exponent.
- [x] SI and IEC prefixes kept apart: `1 GB` is 1000 MB and `1 GiB` is 1024 MiB.
- [x] Unknown units, incompatible dimensions and impossible conversions each reported in their own
      words, discreetly, with nothing written to the file.
- [x] Nothing new in the file format, nothing new in the visual mechanism, and no new dependency:
      the unit table is data and the result is the decoration the math engine already draws.
- [x] Currencies deliberately **not** implemented, and no rate hardcoded. The boundary a future
      source has to sit behind is written down in `ui/src/units/convert.ts` and ADR-025.

### Phase 3.7R: Test Harness Isolation (Completed)
- [x] `scripts/note-it-isolated` isolates the **session bus** as well as XDG. Note-it is a
      single-instance `GApplication`, so with a daemon already running on the real bus an "isolated"
      command was forwarded to it and the real store did the writing — which is how a test note
      reached the user's own notes directory during Phase 3.7's physical testing.
- [x] A private `dbus-daemon` per test run, with `DBUS_SESSION_BUS_ADDRESS` pointed at it and the
      D-Bus starter variables cleared. The real daemon is never stopped and never notices.
- [x] Fail-closed throughout: the bus is started, proved distinct from the real one and proved
      reachable *before* Note-it is launched, and the launched process's environment is read back
      from `/proc` and checked. Exit codes 90–93 say which guarantee could not be met.
- [x] `--root DIR` keeps the private bus alive across invocations, so a daemon started by one
      command and a `new` sent by the next reach the same instance; `--stop` ends it and `--verify`
      asserts the instance really is on the private bus.
- [x] `scripts/test-isolation` reproduces the incident — an ambient session with its own bus, store
      and, where a display exists, a genuine daemon owning the well-known name — and asserts the
      note lands only in the throwaway store while the ambient one is unchanged to the nanosecond.
      It runs under `cargo test`.
- [x] No application code changed. The defect was in the harness, not in Note-it.

### Phase 3.8: Search & Productivity (Completed)
- [x] Global search across every note, opened with `Ctrl+K` from any note. Case-insensitive and
      accent-insensitive, so `biopsia` finds `Biópsia` — the property Portuguese needs most.
- [x] No persistent index. A thousand notes are listed, read, folded, matched and turned into
      snippets in about 20 ms, which is faster than anything a person can notice and cheaper than
      an index that would have to be invalidated, rebuilt and kept honest. The measurement is a
      test, so the claim keeps being checked — see ADR-027.
- [x] Search lives in `src/search.rs` and `StorageManager`, not in the window or the WebView: it
      needs no GTK, no WebKit and no display, which is what a future CLI will need too.
- [x] An empty query lists the most recently written notes, so the same control is also the way to
      move between them.
- [x] A result is one note, addressed by `note_id` — never by path, and never by the label, which
      two notes may share. Opening one activates it, opens it if it was closed, expands it if it
      was collapsed, and scrolls to the match, all without touching `updated_at`.
- [x] Find inside a note with `Ctrl+F`, replace with `Ctrl+H`. Enter and Shift+Enter walk the
      occurrences and wrap at both ends; `Replace All` is a single ProseMirror transaction, so one
      `Ctrl+Z` puts all of it back.
- [x] Neither search nor find can find what is not in the file: a calculation's `4` and a
      conversion's `10000 m` are decorations, and searching for them finds nothing.
- [x] AutoPaste: pasting a URL over selected text makes that text the link, judged by the link
      allowlist the application already had, with no network, no metadata lookup and one undo step.
- [x] Compact link rendering evaluated and deliberately deferred: shortening a URL hides where it
      leads, which is a security regression sold as tidiness. Recorded in ADR-027 rather than
      quietly skipped.

### Phase 3.9: Reliability (Planned)
- [ ] Recoverable trash, so a deleted note is not gone.
- [ ] Automatic backup.
- [ ] Note reliability work generally.

## Phase 4: Core, CLI & Second Brain (Planned)

Architectural evolution rather than more editor surface. Reserved, not started.

- [ ] A safe, shareable core the CLI and the application both build on.
- [ ] A complete CLI: list, search, read, create, append, and edit tasks and notes.
- [ ] Structured output such as `--json`, so the CLI composes with other tools.
- [ ] Filters over text, dates and tasks.
- [ ] The foundation for AI / second-brain integration on top of that core.

**Recency and the CLI.** Since Phase 3.4R, the file's `mtime` reflects the last real edit rather
than the last close, and it is what decides which note a summon brings back when every note is
closed. If a future phase needs "the note I last had open" as distinct from "the note I last wrote
in", that belongs in `state.json` as explicit state, not in the filesystem's timestamps.

## Phase 5: Packaging & Distribution (Planned)

Moved out of Phase 4 rather than dropped: it follows the core and CLI work above.

- [ ] Arch Linux PKGBUILD for AUR.
- [ ] Release automation and binary artifacts.
- [ ] v0.1.0 release.
