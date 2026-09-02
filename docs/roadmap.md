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
- [x] View zoom (75–300%) persisted per note, independent of the document.
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
      snippets in tens of milliseconds, which is faster than anything a person can notice and cheaper than
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
- [x] Paste URL on Selection: pasting a URL over selected text makes that text the link, judged by
      the link allowlist the application already had, with no network, no metadata lookup and one
      undo step.
- [x] Compact link rendering evaluated and deliberately deferred: shortening a URL hides where it
      leads, which is a security regression sold as tidiness. Recorded in ADR-027 rather than
      quietly skipped.

### Phase 3.8R: Search Refinement (Completed)

Four things Phase 3.8 said that were not quite what it did. No new feature, no fuzzy search, no
index, no threads — the smallest correct change to each, and a test for each. See ADR-027.1.

- [x] "Every note" now means every note. The scan stopped at 5 000, so the 5 001st note was
      unfindable and nothing would have said so. The scan reads the whole store; the **result**
      list is still capped at 100, because a hundred rows is what a person reads and the reader can
      see there are a hundred. A test puts a note at position 5 001 and finds it.
- [x] The empty-query listing keeps its cap: it shows at most a hundred notes, so reading past them
      would answer no question.
- [x] The search palette drops any answer to a question that is no longer being asked. Numbering
      caught a slow reply arriving *after* a fast one and missed the opposite order — `bio`
      answering while `biopsia` is still in flight. Only the outstanding request's answer may
      change the list.
- [x] The limits are described as what they are: ceilings on the query and on the answer, not on
      the note. Search reads a note to its end, because a word at the end has to be findable. The
      cost of a large note is measured — a 2 MB note is searched correctly, with its accents
      intact and without writing — rather than claimed to be bounded. No asynchronous machinery was
      introduced to make a sentence true; the sentence was corrected.
- [x] "Most recent" is the note's own `updated_at`, not the file's `mtime`. Appearance — colour,
      paper, pattern intensity, font size — rewrites the file without being an edit, so ordering by
      `mtime` made repainting a note count as writing in it. A note with no readable `updated_at`
      falls back to `mtime`, ties are broken by identifier, and listing still writes nothing.
- [x] No regression in Search, Quick Switcher, Find, Replace, Paste URL on Selection, the shared
      layer or the lifecycle; `updated_at` and the undo history are untouched.

### Phase 3.9: Reliability (Completed)

No new productivity surface. One question only: can any action Note-it offers turn a recoverable
mistake into lost text? See ADR-028 and ADR-029.

- [x] **Recoverable trash.** *Dados › Mover esta nota para a lixeira* moves `notes/<uuid>.md` to
      `trash/<uuid>.md`, with a confirmation that says the deletion can be undone. `×` and `Ctrl+W`
      still mean close, as they always have.
- [x] The order is flush → move → state → surface, and the move is the commit point. A note whose
      latest text could not be written is never moved and never disappears; past the move the note
      *is* in the trash, and neither the state write nor the window teardown may report otherwise.
- [x] A note in the trash is not a note: not searched, not in the quick switcher, not summoned, not
      reopened on restart — because all of those read `notes/`, and the file is not there.
- [x] Restoring puts the file back with the same identifier and the same bytes. `hard_link` refuses
      an existing name, so a live note carrying that identifier is never overwritten — a property of
      the syscall, not of a check that could be raced.
- [x] Neither deleting nor restoring is an edit: `updated_at` does not move, so a recovered note
      returns to its place in the quick switcher instead of jumping to the top. Its geometry comes
      back too.
- [x] The deletion date lives in a `<uuid>.json` sidecar, never in the Markdown, so a note whose
      front matter is damaged still goes to the trash and still comes back byte for byte.
- [x] **Local automatic backup.** `backups/<timestamp>/` holding `notes/`, `trash/`, `config.toml`,
      `state.json` and a manifest — ordinary directories of ordinary files, recoverable with `cp`.
- [x] At most one automatic snapshot per 24 hours, taken **before** the first eligible change after
      that window rather than after it, so the state captured is the one worth going back to. No
      timer, no thread, no polling: an idle daemon does no work at all.
- [x] *Dados › Fazer backup agora* for a snapshot on demand, reported in a line at the foot of the
      note rather than a dialog over it.
- [x] Built in `.tmp.…` and renamed into place: a snapshot is valid or it does not exist. Scratch
      left by a crash is swept by the next backup, and only directories carrying that prefix are
      ever removed.
- [x] Seven kept, pruned **only after** a new snapshot has been committed. A backup that fails never
      costs the protection already on disk, and never blocks a note save.
- [x] Snapshots never contain snapshots, temp files, or anything reached through a symlink.
- [x] Recovery is proved rather than promised: a snapshot is copied into a second, empty XDG tree
      and opened, and the notes, identifiers, Markdown, trash, configuration and window state all
      come back. The manual procedure is in `docs/storage.md`.
- [x] Reliability audit over fifteen failure cases — a note that vanished, one that cannot be read,
      a trash entry removed externally, a restore onto a live identifier, a backups directory that
      cannot be created, a store that cannot be read, a commit that cannot land, scratch left by a
      crash, stale state, missing state, damaged front matter, an absent configuration, and a flush
      that fails with several notes open.
- [x] Terminology: what Phase 3.8 called "AutoPaste" is **Paste URL on Selection**
      (`ui/src/editor/linkPaste.ts`). Behaviour unchanged; the name is freed for the real Clipboard
      AutoPaste in Phase 3.11.

**Deliberately not in this phase:** permanent delete, empty-the-trash, and a one-click restore of a
whole store. The first two are irreversible controls in the phase whose subject is reversibility;
the third is a multi-file transaction that deserves its own design rather than a menu entry.

### Phase 3.10: Timer & Pomodoro (Complete)

- [x] A countdown on the note, reached from a ⏱ in the header bar and shown in a small panel under
      it. No second window, no permanent strip taken from the note.
- [x] Presets at 5, 10, 15, 25, 30, 45 and 60 minutes, and a field for anything else from 1 to 600
      whole minutes. A duration outside that — zero, negative, fractional, `NaN`, absurd — is
      refused and said so, never rounded into range.
- [x] Pomodoro 25/5/15: four focus sessions to a cycle, the fourth followed by the long break, then
      the count begins again. The phase is an explicit model, not behaviour spread across handlers.
- [x] Start, pause, continue, cancel and reset, with only the controls that apply on show. Skip
      moves to the next Pomodoro step without waiting for this one.
- [x] Nothing starts on its own. A finished phase is marked finished and **offers** the next one;
      the reader begins it.
- [x] **The truth is an instant, not a counter.** A running timer is stored as the wall-clock moment
      it ends, and every reading is `deadline - now`. Nothing decrements, so nothing drifts and
      nothing is lost to a throttled WebView, a busy machine or a suspended laptop.
- [x] Pausing discards the instant and freezes the remainder, so paused time cannot be spent —
      through a hide, through a restart, through any number of pause/resume cycles.
- [x] The run survives the note being collapsed, hidden, or the application being closed: it comes
      back with the time that really passed taken off, and one whose end has gone by comes back
      **finished** rather than counting through zero.
- [x] A collapsed note keeps the clock on its bar beside the note's name; a note too narrow for both
      gives up the digits and never the name or the close control.
- [x] Completion happens exactly once, guarded by the state transition itself rather than by a flag:
      one line at the foot of the note and one desktop notification, however long the note sits at
      zero.
- [x] Notifications carry nothing from the note — no title, no text. The page reports *which* kind
      of run ended, from a closed set, and the host owns the words.
- [x] **Not content.** The timer is never written into the Markdown in any form. Starting, pausing,
      finishing and cancelling leave the note file byte for byte as it was and leave `updated_at`
      where it was; search, the collapsed title and the trash never see it. It lives beside the
      window geometry in `state.json`, written only on a semantic change and never on a tick.
- [x] One countdown per note, keyed by the note's identifier, so two notes cannot mix their timers
      and there is no global timer manager.

**Deliberately not in this phase:** the stopwatch and named timers this entry once listed. A
stopwatch counts *up* and has no deadline, which is a second temporal model rather than a second
button on this one; naming a timer is a label with nowhere to be read from — the note is already the
name. Both belong to whatever asks for them with a reason, not to the phase whose subject is a
countdown that can be trusted.

### Phase 3.11: Clipboard AutoPaste (Complete)

The real one, in the sense Antinote uses the word: a capture mode, not the URL-over-selection paste
Phase 3.8 shipped under that name. That one is still there, still called Paste URL on Selection, and
untouched by this.

- [x] An explicit capture mode, off by default, switched on in *☰ › Captura* with one line saying
      exactly what it will do.
- [x] **Off means off, as a property rather than a promise.** While AutoPaste is off there is no
      handler connected to the clipboard at all, so nothing is observed, read, hashed, stored or
      sent. Measured on a real Niri session: three copies with the mode off produced zero clipboard
      events of any kind.
- [x] Event-driven through GDK's own `changed` signal. No polling, no interval, no
      `navigator.clipboard`, and no new dependency: the toolkit already in the process answers this.
- [x] **The mode is session-only and is never written down.** Not in the Markdown, not in
      `state.json`, not in `config.toml`. A restart, a crash or an update leaves it off, and the
      reader decides again.
- [x] One target for the whole application, because the system clipboard is one thing. Arming a
      second note releases the first in the same step, and the released note's bar and menu say so.
- [x] Text only. An image, a file list or an unknown format is declined from the offered formats
      without a byte of it being transferred.
- [x] The clipboard as it was *before* the switch is never captured: connecting the handler reads
      nothing, so only a change after that moment is a capture.
- [x] Captures are appended to the **end** of the note, as one transaction, with no focus taken, no
      selection moved, no scroll and no window raised — the reader is in another application, which
      is the whole point.
- [x] One capture is one `Ctrl+Z`.
- [x] Three delimiters — Linha, Linha em branco (default) and Separador — applied between the
      existing content and the capture, exactly one each time, and never in front of the first
      capture into an empty note. Changing the preference never rewrites what is already there.
- [x] **Loop protection from the toolkit, not from a guess.** A copy or cut inside Note-it makes the
      application the clipboard's owner, and GDK says so; that change is refused before any read
      starts. Comparing text was rejected deliberately: two deliberate copies of the same words are
      two captures.
- [x] A generation on every armed run, checked again when each asynchronous read returns, so a read
      still in the air when the mode is switched off, the target changes, the note closes or the
      application hides delivers nothing.
- [x] One read at a time, so A, B, C arrive as A, B, C.
- [x] Disarmed **before** the flush on close, hide, quit and trash, so no stale callback can reach a
      document that is about to be written out and destroyed.
- [x] A capture is a real edit: the Markdown changes, `updated_at` moves, the ordinary autosave
      writes it, and search finds it. Switching the mode on or off and changing the delimiter change
      none of those.
- [x] Nothing about the mode is content. No marker in the Markdown, nothing in the title, the
      snippet, the trash label or the search index.
- [x] No clipboard content in any log, at any level.
- [x] Note-it never takes ownership of the clipboard: after a capture, the copied text still pastes
      normally into any other application.

### Phase 3.12: Images & Rich Layout (Complete)

Reordered deliberately: what a note was missing was a picture in it, not a way out of it. Capture &
Export moves back, and Flashcards — which needs images to be worth building — moves next.

- [x] Local images in a note: pasted, dropped, or chosen from *☰ › Mídia › Inserir imagem…*.
- [x] **Never base64 in the Markdown.** The bytes go into `assets/<note-uuid>/<asset-uuid>.<ext>`
      beside `notes/` and `trash/`, and the note refers to them by a path relative to `notes/`.
- [x] That relative path is why a note reaches the trash and comes back without a byte being
      rewritten: `notes/` and `trash/` are siblings, so `../assets/…` resolves the same from either.
      No absolute path from the reader's machine is ever written into a note.
- [x] The page never spells a filesystem path. It loads `note-it-asset:/<note>/<asset>.<ext>`, which
      the host serves after parsing both halves as `Uuid`s — so a `..`, an absolute path or an
      encoded separator does not resolve to a file, it does not parse. See ADR-032.
- [x] PNG, JPEG, WebP and GIF, decided by the bytes and never by the filename. **SVG is refused**:
      it is a document that can carry script, and a note is not a place that needs one.
- [x] Plain `![](…)` while there is nothing else to say, and a canonical `<img>` carrying exactly
      `src`, `alt`, `data-note-it-width` and `data-note-it-align` once a width or an alignment is
      chosen. The sanitizer rewrites the tag to that form or drops it entirely.
- [x] Resize by dragging a handle, proportions kept because only the width is ever stored. One drag
      is one entry in the history, not five hundred.
- [x] Left, centre and right, with the text wrapping around a picture aligned left or right.
- [x] Every change to a picture is an ordinary edit through the ordinary autosave: the Markdown
      changes, `updated_at` moves, search finds the words around it. Selecting one, opening its
      controls, cancelling the chooser or choosing the alignment it already has change nothing.
- [x] A picture is not text. Nothing about how one is stored reaches the collapsed title, a search
      snippet, the trash label or `visibleText` — searching an asset's identifier, a width or an
      alignment finds nothing, and a note holding one picture and no words is still unnamed.
- [x] Nothing is fetched. A remote image round-trips as the text it is and is drawn with no source
      at all, so displaying a note reaches the network for nothing.
- [x] No dependency was added.

**Deliberately not in this phase:** cropping, rotation, filters, captions, galleries, lightboxes,
images by URL, and **automatic collection of orphaned assets** — removing a picture takes it out of
the note and leaves the file, because deleting bytes on a guess is worse than keeping them.

#### 3.12R: the backup learns about images

Shipped in 3.12 and caught by the audit that followed: the pictures went into `assets/`, and the
snapshot still copied only `notes/`, `trash/`, `config.toml` and `state.json`. A backup taken in
between restores a note's Markdown and not the file its `![](../assets/…)` points at, which is not
what a backup promises.

- [x] `assets/` is part of a snapshot, in the same shape and byte for byte, for automatic and
      manual backups alike — one routine serves both.
- [x] Copied strictly and fail-closed: two known levels, never a general recursive descent, never a
      symbolic link followed, and anything that is not `<note-uuid>/<asset-uuid>.<ext>` stops the
      snapshot rather than being silently omitted from one reported as complete. Scratch left by an
      interrupted import is skipped, as it is for the notes.
- [x] An image no note points at any more is copied too. A backup is not garbage collection.
- [x] A failure copying an image fails the whole snapshot before the commit point: nothing is
      renamed into place, and retention does not run — an old backup is never deleted to make room
      for one that did not happen.
- [x] Manifest version 2 records the image count. Version 1 snapshots stay listable and readable.
- [x] Proved by restoring into a second, empty store with the original deleted: both notes come
      back, both images render through `note-it-asset:`, and every file is byte-identical.

#### 3.12R.1: the clipper

A refinement of the same phase, not a phase of its own: the way in was three clicks deep for the
thing people do most.

- [x] A paperclip in the header, between **Buscar** and the timer, opening the file chooser on the
      first click — no panel in between.
- [x] One function, two triggers. The button and *☰ › Mídia › Inserir imagem…* call the same handler
      and send the same `insert_image_requested`; no second chooser, importer, asset path or
      serializer exists to drift from the first.
- [x] The menu entry stays, and so do paste and drop.
- [x] Hidden on a collapsed note like the six quick actions, and hidden on an expanded note narrower
      than 300 px: the bar's budget at `MIN_NOTE_WIDTH` has to give somewhere, and the paperclip is
      the only control on it whose job the menu still does in full.
- [x] The drawing is inline SVG from the icon collection, written into the page at build time — the
      pipeline that survives the page's `default-src 'self'`.
- [x] No new keyboard shortcut, no new bridge message, and nothing touched in `assets`, `backup`,
      `storage`, `search`, `timer` or `autopaste`.

### Phase 3.13: Flashcards Core (Complete)

- [x] `Pergunta :: Resposta` produces one source card and one review item; `Termo ::: Definição`
      produces one source and two adjacent directions, in document order and without deduplication.
- [x] Inline syntax requires whitespace, matches `:::` as a whole before `::`, and declines code,
      URLs, times, namespaces, technical attributes, `::::` and ambiguous multiple delimiters.
- [x] A top-level marker paragraph takes exactly the structural block before and after it. Headings,
      hard breaks, bullet and numbered lists, tasks, quotes, callouts, images and image-plus-text are
      preserved as the side the note already holds.
- [x] Extraction reads the ProseMirror tree. Markdown remains the source of truth; there is no
      flashcard file, database, persistent identifier, metadata or backend protocol.
- [x] Managed pictures keep the Phase 3.12 node and `note-it-asset:` route. Study creates no copy,
      thumbnail or second asset and draws no resize or alignment controls.
- [x] A read-only ProseMirror decoration keeps the delimiter visible and marks recognised cards
      without a transaction, save, timestamp change or undo entry. Source/review counts update with
      the live document.
- [x] *☰ › Estudo* gives a zero-card explanation and opens an internal Study panel only when there
      is something to study — no permanent toolbar button and no second GTK window.
- [x] Study has progress, front, reveal, answer, previous, next, shuffle and close. The ends do not
      wrap; navigation and shuffle hide the answer, and shuffle uses an injectable Fisher-Yates RNG
      over review items.
- [x] The sitting is an ephemeral snapshot. Editing and AutoPaste continue underneath, while the
      current list stays fixed until Study is closed and reopened.
- [x] Keyboard and focus stay scoped to the panel: `Escape`, `Space`/`Enter`, arrows, named buttons,
      no double action on a focused button, and focus returned to the invoker on close.
- [x] The panel excludes menu, search, find, trash and timer popovers, closes on collapse, fits the
      220–900 px note range and scrolls a long card internally. Closing the timer popover does not
      stop its countdown.
- [x] Study holds no editor and renders safe ProseMirror fragments with the note's `DOMSerializer`.
      Open, reveal, move, shuffle and close leave Markdown and `updated_at` unchanged.

### Phase 3.14: Study System & Spaced Repetition (Complete)

- [x] Versioned, atomic `$XDG_DATA_HOME/note-it/study.json`, separate from notes and `state.json`,
      with corrupt/newer data preserved and Study failing closed.
- [x] SHA-256 review identity from note UUID, semantic sides, direction and duplicate ordinal;
      moving or presentation-only formatting/resize/alignment does not reset a card.
- [x] Deterministic Ladder-v1 with Difficult/Medium/Easy, exact integer intervals, host-owned clock,
      commit-before-advance, independent reversible directions and one rating per item per sitting.
- [x] On-demand catalog of every live note, including closed notes and excluding trash, parsed in
      the WebView by the same Tiptap schema and extractor as the visible editor.
- [x] Internal Study Hub with Review Now, All, Current Note, source labels, compact list, due/new
      status, compact useful counts, 365-day fixed-scale heatmap and current/longest streak.
- [x] Existing FlashcardPanel evolved for ratings, interval previews, persisted ACK/error handling,
      source note and completion summary; safe rich content and managed images remain one renderer.
- [x] Toolbar deck, recoverable-trash shortcut beside X and Zoom −/+ through existing actions, with
      measured responsive fallback and collapsed-note protection.
- [x] Backup manifest v3 carries optional `study.json`; v1/v2 remain readable and an incomplete
      study copy cannot be committed as a complete snapshot.

Capture & Export, OCR and PDF are postponed. They are not part of Phase 3.14.

### Phase 3.14R.1: Interface Polish & Visual Accessibility (Complete)

- [x] Study Hub distinguishes source cards from review directions, including the explicit 2-card /
      3-review reversible corpus.
- [x] Header actions grouped by purpose with restrained separators, stable IDs/handlers and a
      centred wide/compact search pill that opens the existing SearchPalette.
- [x] Shared short motion tokens for buttons and internal panels, safe content-only collapse motion,
      immediate hidden-state semantics and full `prefers-reduced-motion` fallback.
- [x] Per-note zoom extended to 300% through the same frontend/host/state path.
- [x] Global 90–160% Interface scale in `config.toml`, broadcast to all notes and reflected in real
      chrome and collapsed geometry without affecting document zoom, text sizing or Markdown.
- [x] Central shortcut metadata keeps tooltips, menu hints and `aria-keyshortcuts` aligned without
      inventing shortcuts for actions that have none.
- [x] Responsive budget verified from 220–900 px at 100/120/140/160%, preserving Menu, active
      Timer/AutoPaste and Close before optional shortcuts.

## Phase 4: Programmable Note-it

Architectural evolution from an application into a local programmable platform. GUI, CLI and
future machine interfaces share one domain and persistence authority.

- [x] **Phase 4.0A — Core Boundary.** Dedicated headless `noteit-core` crate; the GUI consumes its
      shared note/search/trash/Study/storage capabilities, with a Cargo dependency gate preventing
      GTK, GDK, WebKitGTK, layer-shell, Wayland and Niri from entering Core.
- [x] **Phase 4.0B — Metadata Foundation: Tags + Properties.** Structured user metadata in each
      note's Markdown front matter, validated and persisted by Core; derived live-note catalogs,
      transactional writes over the current WebView document, responsive pills and one compact
      metadata panel. No sidecar, index, database or CLI command.
- [x] **Phase 4.0D — Read API.** Headless read-only CLI interface backed by `noteit-core` authorities;
      subcommands `listar`/`list`, `ler`/`read`, `buscar`/`search`, `tags`, `propriedades`/`properties`,
      `tarefas`/`tasks`, `lixeira`/`trash`; metadata filtering (`--tag`, `--propriedade`/`--property`,
      `--limite`/`--limit`), task parsing with state filter (`--estado`/`--state`), safe note selector
      resolution, terminal security sanitization, and strictly zero store mutations.
- [x] **Phase 4.0D.1 — Read API Contract & Terminal Hardening.** Standardized local machine timezone
      presentation (`dd/MM/yyyy HH:mm`) matching GUI contracts, universal untrusted input terminal sanitization,
      decoupled typed Core warnings (`ReadBatch<T>`, `ReadWarning`) with zero print statements in Core, and
      faithful TypeScript task metadata comment parsing.
- [x] **Phase 4.0D.2 — Read Pipeline Purity & Warning Completeness.** Unified search loading and warning
      pipeline across filtered and unfiltered searches over the full eligible note universe, eradicated
      direct stderr output in Core storage read paths, separated domain query from presentation sanitization,
      and strictly enforced task comment token validation.
- [x] **Phase 4.0E — Write API + GUI/CLI concurrency.** Exactly one Note-it writer per store, enforced
      by an advisory `flock` lease in a per-store runtime directory: the desktop instance takes it at
      startup and holds it for the whole session, the CLI takes it for one command when it is free,
      and sends the change over a private local Unix socket when it is not — never writing around
      another writer. Typed Core write operations (`WriteOperation`, `NoteMutation`, `WriteOutcome`,
      `WriteError`) shared by both paths; commands `criar`/`create`, `adicionar`/`append`,
      `editar`/`edit`, `tags adicionar|remover`, `propriedades definir|remover`,
      `tarefas concluir|reabrir`, `lixeira restaurar`, with `--stdin` and `--vazio`. A note open on
      screen is changed behind an external-write barrier that freezes the editor *before* reading it,
      so unsaved text is folded into the same commit rather than overwritten; a runtime generation per
      window makes every message still in flight from the previous run refusable. Optimistic
      `TaskRef` snapshot tokens with no sidecar and no persistent task identity. Read API stays
      read-only; note writes never touch `config.toml` or `state.json`.
- [x] **Phase 4.0E.1 — Fail-Closed Writer Authority & Confirmed UI Adoption.** Made 4.0E's central
      invariant structural rather than aspirational: the desktop instance holds `WriteAuthority` by
      value and refuses to start without a lease *and* a control socket, so a running, editable
      Note-it that does not own its store is unrepresentable. Adoption of a committed document is
      confirmed by the page (`ExternalWriteApplied`, validated on note, request and generation) rather
      than inferred from a script having evaluated, with a bounded wait that downgrades to
      `ui_sync_warning` and never to failure. Removed the client-side timeout that could release the
      editor while a commit was still in flight; a slow write now says it is slow and stays held.
- [x] **Phase 4.0E.2 — Failed UI Adoption Stays Blocked.** Closed the last post-commit gap: a page
      that could not adopt an already-committed document is no longer released. It keeps the old
      generation *and* stays frozen, keeps its queued document actions unrun, sends only the negative
      acknowledgement, and tells the reader the window is out of step. The write stays committed and
      still reports `ui_sync_warning`. Reopening the note restores it from the committed file exactly,
      verified end to end in the isolated environment.
- [x] **Phase 4.0E.2R — Terminal Unsynchronised State.** Made the terminal state terminal in fact: the
      barrier holds one explicit phase, every transition is guarded by it, and no timer, late
      callback, repeated apply, abort or generation update can return a page that failed to adopt a
      committed document to an editable or seemingly synchronised state. Also closed a transaction
      lock left off when adopting threw part-way.
- [x] **Phase 4.0F — Machine Interface / JSON.** The first stable public contract for machine
      consumers: a global `--json` that emits exactly one versioned JSON document per execution, on
      standard output for a success and standard error for a failure, with the other channel empty
      and no ANSI anywhere. Rendered from the same typed outcome the Portuguese sentences are
      rendered from — `noteit-cli` gained `outcome.rs` (what happened) and `machine.rs` (the public
      schema), and `run_with_args` now returns a `CliResponse` carrying both channels as data, so a
      warning can no longer escape through an `eprint!` in the middle of a command. Canonical command
      names independent of spelling, full UUIDs, RFC 3339 UTC timestamps, real JSON types, raw
      Markdown unmangled by the terminal sanitizer, and stable error codes. The two post-commit
      states that would otherwise be flattened into "it failed" are first class: a committed write
      whose window did not confirm is `status: warning` with `commit_state: committed` and exit `0`,
      and an unknown result is `status: indeterminate` with `commit_state: unknown` — never
      `not_committed`, so no agent can retry it into a duplicated append. Machine mode survives a
      parse error. The private control protocol is not exported. Human output, exit codes and the
      write rules are unchanged; the help gained one line documenting the option. Contract in
      `docs/machine-interface.md`, reasoning in ADR-041.
- [ ] **Phase 4.0G — Interactive CLI / TUI.** Reserved.
- [ ] **Phase 4.0H — Developer & Automation Tools.** Reserved.
- [ ] **Phase 4.0R — Security & Regression Audit.** Reserved.
- [ ] **Phase 4.1 — MCP.** Reserved.
- [ ] **Phase 4.2 — AI / Second Brain.** Reserved.

Capture & Export, OCR and PDF remain postponed and are not pulled into Phase 4.0A or 4.0B.

**Recency and the CLI.** Since Phase 3.8R, "most recent" is the note's own `updated_at` — the last
change to its text — with the file's `mtime` as the fallback for a note that has none. It is what
decides which note a summon brings back when every note is closed, and what search and the quick
switcher order by. If a future phase needs "the note I last had open" as distinct from "the note I
last wrote in", that belongs in `state.json` as explicit state, not in the filesystem's timestamps.

## Phase 5: Packaging & Distribution (Planned)

Moved out of Phase 4 rather than dropped: it follows the core and CLI work above.

- [ ] Arch Linux PKGBUILD for AUR.
- [ ] Release automation and binary artifacts.
- [ ] v0.1.0 release.
