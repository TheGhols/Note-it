# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **Phase 3.14 Study System & Spaced Repetition.** The deck is now every flashcard in every live
  note, including closed notes, with trash excluded and restored notes returning with their prior
  schedule. One on-demand Tiptap editor parses the host's document catalog through the existing
  ProseMirror extractor; Rust never learns the `::` syntax.
  - Each review direction receives a SHA-256 identity derived from note UUID, semantic front/back,
    direction and duplicate ordinal. Formatting, image width/alignment and document position are
    presentation and do not reset progress; semantic text, managed asset or direction changes do.
  - `study.json` version 1 lives in `$XDG_DATA_HOME/note-it/`, separate from Markdown and
    `state.json`. It contains only opaque review keys, Ladder-v1 schedules and daily counters, is
    committed atomically, and fails closed without replacing corrupt or newer data.
  - Difficult, Medium and Easy use the fixed 10-minute through 240-day ladder. The Rust host owns
    the clock and local civil day; the panel advances and updates activity only after the atomic
    write is acknowledged, and a failed write leaves the card and persisted state unchanged.
  - The internal Study Hub provides Review Now, All and Current Note, a compact global list, seven
    useful counts, a fixed-scale accessible 365-day heatmap, current/longest streaks and the same
    safe FlashcardPanel renderer with source-note labels, interval previews and a minimal summary.
  - The header adds a one-click deck, Zoom −/+, and a recoverable-trash shortcut immediately beside
    Close. Zoom reuses `zoom_changed`; trash can only open the existing confirmation. Measured
    breakpoints hide optional shortcuts before they can displace Menu, active Timer/AutoPaste or X.
  - Backup manifest version 3 adds optional `study.json`. Versions 1 and 2 remain readable; an
    existing study file that cannot be copied fails the snapshot before its commit point.

- **Phase 3.13 Flashcards Core.** Cards are projections of the note itself: write
  `Pergunta :: Resposta` for one direction or `Termo ::: Definição` for both, inline with spaces or
  as a top-level marker between two structural blocks.
  - Extraction walks the ProseMirror document rather than matching Markdown. Code, URLs, times,
    namespaces, image attributes, long colon runs and ambiguous lines stay ordinary content, while
    rich marks, headings, lists, tasks, quotes, callouts and managed images remain intact.
  - The editor keeps `::` and `:::` visible under a quiet decoration and reports both source-card
    and review-item counts live. Detection and decoration dispatch no transaction and write no
    hidden identity, metadata, database or sidecar file.
  - *☰ › Estudo* opens a read-only panel in the current WebView with progress, reveal, previous,
    next, deterministic-testable shuffle, keyboard navigation, accessible names, focus restoration
    and an internal scroll for long cards. A note with no cards says so and opens nothing.
  - Each sitting snapshots the review items when it opens. Editing and AutoPaste continue without
    rearranging it; reopening takes the new snapshot. Timer/Pomodoro continues while its popover is
    closed, and collapsing the note ends the sitting.
  - Images reuse the Phase 3.12 `noteItImage`, stored reference and `note-it-asset:` route. Study
    serializes safe document fragments, copies no asset and exposes no editing controls.
  - Open, reveal, navigation, shuffle and close leave Markdown, `updated_at`, undo history and
    persisted application state untouched. Scheduling and spaced repetition remain outside 3.13.

### Fixed
- **Phase 3.12R — a snapshot now holds the pictures too.** Phase 3.12 put a note's images in
  `assets/<note-uuid>/<asset-uuid>.<ext>` and the backup still copied only `notes/`, `trash/`,
  `config.toml` and `state.json`. A snapshot taken in between restores a note's Markdown and not the
  file its `![](../assets/…)` points at — half a note, from something whose whole promise is that it
  holds everything recoverable.
  - `assets/` is part of every snapshot now, automatic and manual alike, in the same shape it has in
    the store and byte for byte. No recompression, no conversion, no renaming: a backup copies bytes.
  - Copied strictly and fail-closed. Two known levels and never a general recursive descent; no
    symbolic link is followed at either; and anything that is not `<note-uuid>/<asset-uuid>.<ext>`
    stops the snapshot rather than being quietly left out of one reported as complete. `assets/` is
    written by Note-it and by nothing else, so an oddity there means the store is not in the state it
    is believed to be. Scratch left by an interrupted import is skipped, as it is for the notes.
  - Each name is validated by the same parser the `note-it-asset:` scheme uses, so a snapshot holds
    exactly the files the application can serve and the two cannot come to disagree.
  - An image no note points at any more is copied too. Phase 3.12 chose not to collect orphans, and
    a backup is not the place to start doing it by omission.
  - A failure copying an image fails the whole snapshot before the commit point: nothing is renamed
    into place, the scratch directory is removed, and retention does not run — an old backup is never
    deleted to make room for one that did not happen.
  - `manifest.json` is version 2 and records how many images the snapshot holds. Version 1 snapshots
    stay listable and readable, and read back as the zero images they genuinely held.
  - A store written before images existed has no `assets/` at all, and backs up unchanged.
  - `docs/storage.md` now includes `assets/` in the manual restore procedure.

### Added
- **Phase 3.12R.1 — a paperclip in the header.** Putting a picture in a note is the commonest thing
  anyone does with the Mídia section, and it took opening the menu and walking into a submenu first.
  A paperclip now sits in the bar between **Buscar** and the timer and opens the file chooser on the
  first click.
  - The same chooser, the same import, the same `assets/<note-uuid>/<asset-uuid>.<ext>` and the same
    relative reference in the Markdown. Both triggers run one function and send the one existing
    `insert_image_requested` message: a second door into the room, never a second room.
  - *☰ › Mídia › Inserir imagem…* is untouched and keeps working, as do paste and drop.
  - Hidden while the note is collapsed, like the six quick actions, and hidden on an expanded note
    narrower than 300 px — the bar's budget at `MIN_NOTE_WIDTH` has to give somewhere, and the
    paperclip is the only control there whose job the menu still does in full.
  - Its drawing is inline SVG written into the page at build time from the icon collection, like
    every other icon in the bar. Nothing is fetched, so nothing comes out blank under the page's
    own `default-src 'self'`.
  - No new IPC message, no new chooser, no new import path, no new keyboard shortcut, and no change
    to `assets`, `backup`, `storage`, `search`, `timer` or `autopaste`.

- **Phase 3.12 Images & Rich Layout.** A picture in a note, kept as a file rather than smuggled into
  the text. Paste one, drop one on the note, or choose one from *☰ › Mídia › Inserir imagem…*.
  - PNG, JPEG, WebP and GIF, decided by the first few bytes and never by a filename — so a PNG
    called `.txt` is a PNG and something called `.png` that is not an image is refused. **SVG is
    not accepted**: it is a document format that can carry script. A refusal says so in a line at
    the foot of the note and leaves nothing behind.
  - **Never base64 in the Markdown.** The bytes go to
    `~/.local/share/note-it/assets/<note-id>/<asset-id>.<ext>`, beside `notes/` and `trash/`, and
    the note stores a path relative to `notes/`. One screenshot would otherwise turn a note you can
    read into a megabyte you cannot, and do the same to every backup and every diff.
  - That relative form is why a note reaches the trash and comes back byte for byte: `notes/` and
    `trash/` are siblings, so `../assets/…` resolves the same from either and nothing is rewritten.
    No absolute path from the reader's machine is ever written into a note.
  - **The page never spells a filesystem path.** It loads `note-it-asset:/<note>/<asset>.<ext>`,
    which the host serves after parsing both halves as `Uuid`s — a `..`, an absolute path or an
    encoded separator does not resolve to a file, it does not parse. The page's
    Content-Security-Policy was widened by that scheme and nothing else. See ADR-032.
  - Plain `![](…)` while there is nothing to say beyond where the picture is, and a canonical
    `<img src alt data-note-it-width data-note-it-align>` once a width or an alignment is chosen —
    always those attributes, always in that order, only the ones set. Anything else in such a tag is
    dropped: an `onerror`, a `style`, a `srcset`, or a source that is not one of this store's assets.
  - Resize by dragging either handle, with proportions kept because only the width is ever stored.
    A picture can be made as wide as the note and no wider. The whole drag is one entry in the
    history, so `Ctrl+Z` returns the width you started from.
  - Left, centre and right, with the text running down the other side of a picture aligned left or
    right — around it, never under it. Quotes, comments and code blocks sit beside a float rather
    than beneath it.
  - Every change to a picture is an ordinary edit: the Markdown changes, `updated_at` moves and the
    existing autosave writes it. Selecting one, opening its controls, cancelling the file chooser or
    choosing the alignment it already has change nothing at all.
  - **A picture is not text.** Nothing about how one is stored reaches the collapsed title, a search
    snippet, the trash label or `visibleText`: searching an identifier, a width, an alignment or
    `assets` finds nothing, and a note holding one picture and no words is still *Nota sem título*.
  - Nothing is fetched. There is no way to insert an image by URL, and a remote one somebody typed
    is drawn with no source at all, so opening a note reaches the network for nothing.
  - Removing a picture takes it out of the note and **leaves the file**. There is no automatic
    collection of orphaned assets, deliberately: deciding a file is unused is a guess, and acting on
    that guess destroys something.
  - No dependency was added.

### Changed
- **Roadmap reordered.** 3.12 is Images & Rich Layout; Flashcards Core stays next at 3.13; Capture &
  Export — text export, PDF and the offline-OCR evaluation — moves back to 3.14.
- **Phase 3.11 Clipboard AutoPaste.** Copy something anywhere on the machine and it lands at the end
  of a note you chose. No window appears, no key is pressed for you, and nothing takes your cursor.
  Distinct from *Paste URL on Selection*, which Phase 3.8 shipped and which is untouched.
  - **Off by default, and off means no listener.** While AutoPaste is off there is no clipboard
    handler connected at all, so nothing is observed, read, hashed, stored, logged or sent. Measured
    on a real Niri session: three copies with the mode off produced zero clipboard events of any
    kind. See ADR-031.
  - **The mode is never written down.** Not in the Markdown, not in `state.json`, not in
    `config.toml`. A restart, a logout, a crash or an update leaves it off and the reader decides
    again — there is no field on the protocol that could switch it back on.
  - Switched on in *☰ › Captura*, with one line saying exactly what it will do. While it is on the
    note keeps its bar out with a 📋 beside the other controls, on a collapsed note too, and pressing
    that opens the panel that switches it off.
  - **One target for the whole application**, because the system clipboard is one thing. Arming a
    second note releases the first in the same step, and the released note's bar and menu stop
    claiming it.
  - Event-driven through GDK's own `changed` signal — no polling, no interval, no
    `navigator.clipboard`, and no new dependency.
  - Text only: an image, a file list or an unknown format is declined from the offered formats
    without a byte of it being transferred. An empty or blank copy files nothing at all.
  - **Whatever was on the clipboard before the switch is never captured.** Connecting the handler
    reads nothing, so only a change after that moment is a capture.
  - Captures are appended to the **end** of the note as one transaction: no focus taken, no
    selection moved, no scroll, no window raised, no layer changed. One capture is one `Ctrl+Z`.
  - Text goes in as text, with the same meaning a `Ctrl+V` has here: `**isso é literal**` stays
    asterisks, `<script>alert(1)</script>` stays eleven characters, a URL stays a URL and nothing is
    fetched. Accents, emoji, 日本語 and multi-line copies survive unchanged.
  - Three delimiters — **Linha**, **Linha em branco** (default) and **Separador** — applied exactly
    once between each pair and never in front of the first capture into an empty note. Changing the
    preference applies to the next capture and rewrites nothing already written.
  - **Loop protection from the toolkit, not from a comparison.** A copy or cut inside Note-it makes
    the application the clipboard's owner and GDK says so, and that change is refused before any
    read starts. Content dedupe was rejected deliberately: copying `ABC` twice, in two actions,
    files it twice.
  - A generation on every armed run, revalidated when each asynchronous read returns, so a read
    still in the air when the mode is switched off, the target changes, the note closes or the
    application hides delivers nothing. Reads are serialised, so A, B, C arrive as A, B, C.
  - Switched off **before** the flush on close, hide, quit and trash, so no stale callback can reach
    a document that is about to be written out and destroyed. Collapsing, changing layer and moving
    to another application all leave it on.
  - A capture is a real edit — the Markdown changes, `updated_at` moves, the existing autosave
    writes it and search finds the text. Switching the mode on or off and changing the delimiter
    change none of those, and put no marker of their own into the note.
  - Note-it never takes ownership of the clipboard: after a capture, what you copied still pastes
    normally into any other application.
- **Phase 3.10 Timer & Pomodoro.** A countdown on the note you are working in, reached from a ⏱ in
  the header bar and shown in a small panel under it. No second window, and no strip permanently
  taken from the note.
  - **Timer** with presets at 5, 10, 15, 25, 30, 45 and 60 minutes and a field for anything else
    from 1 to 600 whole minutes. Zero, a negative, a fraction, `NaN` or something past the ceiling
    is refused and said so; nothing is rounded into range, because a timer that quietly ran for a
    duration nobody chose is worse than one that declined to start.
  - **Pomodoro 25/5/15**: four focus sessions to a cycle, the fourth followed by the long break,
    then the count begins again. The phase is an explicit model rather than behaviour spread across
    event handlers, and the panel shows which phase, which session of the four, and the cycle.
  - Start, pause, continue, cancel, reset and skip, with only the controls that apply on show —
    no Pause on a paused timer, no Continue on one that never started.
  - **Nothing starts by itself.** A phase that runs out is marked finished and *offers* the next one
    on the button; the reader begins it. A break that started on its own mid-sentence would be a
    Pomodoro nobody agreed to.
  - **The truth is an instant, not a counter.** A running run is stored as the wall-clock moment it
    ends and every reading is `deadline - now`, so nothing drifts and nothing is lost to a throttled
    WebView, a busy machine or a suspended laptop. Pausing discards the instant and freezes the
    remainder, so paused time cannot be spent — through a hide, through a restart, or through any
    number of pause/resume cycles. See ADR-030.
  - The run survives the note being collapsed, hidden, or the application closed and reopened: it
    comes back with the time that really passed already taken off, and one whose end has gone by
    comes back **finished** rather than counting through zero. It does not ring for a run that ended
    while nothing was there to hear it; the finished state is on the bar instead.
  - A collapsed note keeps the clock on its bar beside the note's name, so a running countdown never
    needs the note expanded to be trusted. A note too narrow for both gives up the digits and keeps
    the icon; the name and the close control never give way.
  - Completion happens **exactly once**, guarded by the state transition itself rather than by a
    flag: one line at the foot of the note and one desktop notification, however long the note sits
    at zero. The notification carries nothing from the note — the page reports which kind of run
    ended, from a closed set of four, and the host owns the words.
  - **A timer is not part of the note.** It is never written into the Markdown in any form.
    Starting, pausing, finishing and cancelling leave the note file byte for byte as it was and
    leave `updated_at` where it was, so a note with a timer does not jump to the top of the quick
    switcher; search, the collapsed title and the trash never see it. Searching `25:00` will not
    find a note merely because it has a 25-minute Pomodoro running. The state lives beside the
    window geometry in `state.json`, written only on a semantic change and never on a tick, so a
    running countdown costs no disk traffic and no IPC at all.
  - One countdown per note, keyed by the note's identifier: two notes cannot mix their timers, and
    there is no global timer manager.
- **Phase 3.9UX header ergonomics.** The existing header now recedes on expanded notes and returns
  on hover/focus, while a collapsed note keeps it visible with a presentation-only title derived
  from the first useful Markdown line. Colour and inline text size moved out of `☰` into exactly two
  quick actions that open their existing panels and pipelines. A menu taller than the note is capped
  to the WebView and scrolls vertically, including every submenu; larger notes keep the natural menu.
  The two shipped icons are the reviewed `palette-round` and `larger-text` SVGs from
  `IconesNote-it/`; the rest of the supplied collection remains local and ignored.
- **Recoverable trash.** Deleting a note now exists, and it can be undone.
  - *☰ › Dados › Mover esta nota para a lixeira* asks first, and the question says the deletion is
    recoverable rather than just "Excluir?". Cancel is what the panel focuses. The `×` button and
    `Ctrl+W` still mean **close the window**, exactly as they always have.
  - The order is flush → move → state → surface, and the move of the file is the commit point.
    A note whose latest text could not be written is **not** moved: it stays open, the failure is
    reported, and the reader can try again. Past the move the note is in the trash, so neither the
    window-state write nor the surface teardown may report otherwise.
  - `notes/<uuid>.md` becomes `trash/<uuid>.md`, byte for byte — front matter, colour, paper, tasks,
    links, calculations and comments all travel with it. Nothing reads, parses or rewrites the note,
    so a note whose front matter is damaged is deleted and recovered unchanged too.
  - A note in the trash is not a note: `Ctrl+K` does not find it, the empty-query list does not offer
    it, a summon does not bring it back, and a restart does not reopen it — because all of those read
    `notes/`, and the file is no longer there.
  - *Dados › Lixeira* lists what can be recovered, newest first, with each note's first line, a
    preview and when it was deleted. Arrows walk the list, `Enter` restores, `Esc` closes; every row
    also has a named **Restaurar** button.
  - Restoring returns the same file with the same identifier, and **never overwrites a live note**:
    the name is created with `hard_link`, which refuses an existing one atomically, so a clash leaves
    both files untouched and says so.
  - Neither deleting nor restoring is an edit. `updated_at` does not move, so a recovered note
    returns to its place in the quick switcher instead of jumping to the top; its geometry comes back
    too.
  - The deletion date is a `<uuid>.json` sidecar beside the note, never written into the Markdown. A
    missing or unreadable one costs that entry its exact date and nothing else.
- **Local automatic backup.** Snapshots of everything recoverable, on the same machine and nowhere
  else.
  - `~/.local/share/note-it/backups/<data-e-hora>/` holding `notes/`, `trash/`, `config.toml`,
    `state.json` and a `manifest.json`. Ordinary directories of ordinary files: readable with `ls`,
    recoverable with `cp`, with no archive format and no database in the way.
  - At most one automatic snapshot per 24 hours, taken **before** the first eligible change after
    that window rather than after it — the state worth being able to return to is the one before the
    edit. There is no timer and no thread: an idle daemon does no work at all, and one left open for
    days takes its snapshot the moment its owner starts typing again. "When was the last backup" is
    read from the newest snapshot's own manifest, so there is no bookkeeping file to go stale.
  - *Dados › Fazer backup agora* takes one immediately and reports success or failure in a line at
    the foot of the note rather than a dialog over it.
  - A snapshot is built in `backups/.tmp.…` and renamed into place: the rename is the commit point,
    so a half-written backup can never be listed as a valid one. Scratch left by a crash is swept by
    the next backup, and only directories carrying that prefix are ever removed — never a snapshot,
    never a file someone put there.
  - Seven snapshots are kept, and retention runs **only after** a new one has been committed, so a
    backup that fails never costs the protection already on disk.
  - A snapshot never contains previous snapshots, temporary files, or anything reached through a
    symbolic link — only regular files from the directories it was asked to copy.
  - A backup that fails never blocks a save. The error is reported and the note is written normally.
  - Recovery is proved rather than promised: `a_snapshot_round_trips_into_a_fresh_isolated_store`
    copies a snapshot into a second, empty XDG tree and opens it. The manual procedure, including
    recovering a single note, is in `docs/storage.md`.
  - **A local backup is not disaster recovery.** These snapshots sit on the same disk as the notes
    and are not encrypted. They protect against an accidental deletion, a logical corruption, an edit
    to undo or a version to go back to — and against none of a dead drive, a lost machine or a stolen
    one.

### Changed
- What Phase 3.8 shipped as "AutoPaste" is now called **Paste URL on Selection**
  (`ui/src/editor/linkPaste.ts`, `handleLinkPaste`, `ui/tests/link_paste.test.ts`). The behaviour is
  byte-for-byte the same; only the name changed, so "Clipboard AutoPaste" is free for the clipboard
  capture mode planned for Phase 3.11, which is a different feature entirely.

### Fixed
- Search now does what it says it does. Four corrections, no new behaviour:
  - **Every note is searched.** The scan stopped at 5 000 notes, so a store one note larger held a
    note that could never be found and nothing would have reported it skipped. The scan now reads
    the whole store; the **result** list is still capped at 100. The empty-query listing keeps its
    cap, because it shows at most a hundred notes.
  - **The palette drops any answer to a question it is no longer asking.** Numbering caught a slow
    reply arriving after a fast one, but not the other order: the answer to `bio` arriving while
    `biopsia` was still in flight was older than the current question and newer than anything
    accepted, so it was shown. Only the outstanding request's answer can change the list.
  - **"Most recent" is the note's own `updated_at`, not the file's date.** Changing a note's
    colour, paper, pattern intensity or font size rewrites the file without being an edit, so
    ordering by the file's modification time made repainting a note count as writing in it — in
    the quick switcher and in which note a summon brought back. A note with no readable
    `updated_at` falls back to the file's date, exactly as before, and ties are broken by
    identifier. Listing still writes nothing.
  - **The documented limits now say what they bound.** 512 characters of query, 100 results and
    ~240 characters of snippet are ceilings on the question and on the answer; they never bounded
    the size of a note, and search reads a note to its end because a word at the end has to be
    findable. The cost of a large note is measured — a 2 MB note is searched correctly, accents
    intact, writing nothing — rather than described as bounded.
- The isolated test harness now isolates the **session bus** as well as the XDG directories.
  Note-it is a single-instance `GApplication`: with a daemon already running on the real bus, an
  "isolated" command was handed to that daemon over D-Bus and the real store did the writing, so
  overriding `XDG_*` protected nothing. `scripts/note-it-isolated` now starts a private
  `dbus-daemon` for each test session, points `DBUS_SESSION_BUS_ADDRESS` at it and clears the D-Bus
  starter variables, so the isolated process becomes the primary instance and works in its own
  store — with the real daemon left running and untouched.
  - Fail-closed: the bus is started, proved distinct from the real one and proved reachable before
    Note-it is launched, and the launched process's environment is read back from `/proc`. Exit
    codes 90–93 name the guarantee that could not be met.
  - `--root DIR` keeps the private session alive across invocations, `--verify` asserts the instance
    is on it, and `--stop` ends it — synchronously, and reading process liveness from `/proc` rather
    than from `kill -0`, because where nothing reaps orphans a stopped daemon lingers as a zombie
    that `kill -0` still reports as alive.
  - `scripts/test-isolation` reproduces the incident and runs under `cargo test`; against the old
    harness it fails with the stray note in the ambient store, and against the new one it passes.
  - No application code changed: the defect was in the harness.

### Added
- Search across every note, and the ways of getting to what it finds:
  - `Ctrl+K` opens a search palette inside the note you are already in — no second window, no
    second application. Case-insensitive and accent-insensitive, so `biopsia` finds `Biópsia` and
    `coracao` finds `Coração`.
  - An empty query lists the most recently written notes, so the same control is also a quick
    switcher.
  - One note is one result, with a label derived from its first non-empty line, a snippet around
    the first match and a count when there are several. Snippets are rendered as text, never as
    markup.
  - `Enter` opens the chosen result: a note already open is activated, a closed one is opened, a
    collapsed one is expanded, and the match is scrolled to and highlighted. None of that touches
    `updated_at`, and none of it changes the Desktop/Overlay layer.
  - Results are addressed by `note_id`. The WebView cannot name a path, so it cannot ask for one.
  - Explicit limits: 512 characters of query, 100 results, ~240 characters of snippet. Typing is
    debounced by 120 ms and every request is numbered, so an answer to `bio` can never replace a
    newer answer to `biopsia`.
  - Searching writes nothing: no flush, no save, no index file, no `state.json` entry.
- Find and replace inside the current note:
  - `Ctrl+F` finds, with a live count, `Enter`/`Shift+Enter` to walk the occurrences and wrapping at
    both ends; `Esc` closes and hands the keyboard back to the editor. Opening it with a short
    single-line selection seeds the field from it.
  - `Ctrl+H` adds replace: one occurrence, or all of them. An `Aa` toggle makes the search
    case-sensitive.
  - `Replace All` is a single ProseMirror transaction applied last-to-first, so twenty
    replacements come back with one `Ctrl+Z`. Marks, lists, headings and code blocks survive,
    because the document is edited rather than re-serialised.
  - Unlike global search, find and replace is accent-**sensitive**: replacing is destructive, and
    `saude` must not overwrite `saúde`. A result chosen from the palette therefore carries the
    spelling that actually matched, so `biopsia` still lands on `Biópsia`.
  - Highlighting is a decoration: finding 7 occurrences creates no transaction, no undo step and no
    write.
- Pasting a URL over selected text turns that text into a link — select `site oficial`, paste
  `https://example.com`, and the note holds `[site oficial](https://example.com)`.
  - It reuses `safeLinkUrl`, the allowlist the rest of the application already used, so there is
    exactly one opinion about what a URL is. Tiptap's own `linkOnPaste` is switched off, because it
    uses `linkifyjs` and accepted schemes this application does not.
  - Nothing is fetched: no title, no favicon, no preview, no network.
  - Inline code, code blocks and selections spanning two blocks are left as an ordinary paste, and
    the whole thing is one undo step.
- Unit conversions, written the way the rest of the engine is and shown the same way:
  - `= 10 km em m` shows `10000 m` beside the line. `em` is the conversion keyword, and the only
    one.
  - Eight dimensions, every spelling listed in `docs/features.md`: **comprimento** (`mm`, `cm`,
    `m`, `km`, `in`, `ft`, `yd`, `mi`), **massa** (`mg`, `g`, `kg`, `t`, `oz`, `lb`), **volume**
    (`mL`, `cL`, `dL`, `L`, `cm³`, `m³`), **temperatura** (`°C`, `°F`, `K`), **tempo** (`ms`, `s`,
    `min`, `h`, `dia`, `semana`), **área** (`mm²`, `cm²`, `m²`, `km²`, `ha`), **dados digitais**
    (`B`, `KB`, `MB`, `GB`, `TB`, `KiB`, `MiB`, `GiB`, `TiB`) and **velocidade** (`m/s`, `km/h`,
    `mph`), each with ASCII and Portuguese aliases.
  - The left-hand side is a full math-engine expression, so `= (10 + 5) km em m`,
    `= distancia km em m` and `= x * 2 km em m` all read. The unit applies to the whole expression.
  - Temperature converts as scales with different zeroes rather than as a factor: `= 0 C em F` is
    `32 °F` and `= 0 C em K` is `273,15 K`. Area is its own unit rather than a length with an
    exponent, so `= 1 m2 em cm2` is `10000 cm²`.
  - SI and IEC prefixes stay apart: `= 1 GB em MB` is `1000 MB` and `= 1 GiB em MiB` is `1024 MiB`.
  - `= 10 banana em m` says *unidade desconhecida*, `= 10 kg em km` says *unidades incompatíveis*
    and `= -300 C em K` says *conversão inválida* — quietly, beside the line, and never in the file.
  - A converted quantity ends an aggregation block, because `sum`, `avg` and `count` add up plain
    numbers and know nothing about units.
  - Conversions are read exactly where calculations are: plain paragraphs only.
- Every conversion is local, offline and deterministic, and the factors are the defined ones — an
  inch is exactly 0.0254 m, a pound exactly 453.59237 g. Nothing whose value depends on which
  definition the reader had in mind was included, which is why there is no `cup` and no `alqueire`.
- Currencies were deliberately **not** implemented and no rate was hardcoded. The boundary a future
  rate source has to sit behind is written down in `ui/src/units/convert.ts` and ADR-025, and a test
  asserts that nothing in the engine can reach the network.
- A math engine. A note calculates as it is written, with nothing to press and no mode to enter:
  - `= 2 + 2` shows `4` beside the line; `+`, `-`, `*`, `/` and parentheses, with the usual
    precedence. Decimals may be written `10.5` or `10,5`; a number with two separators is refused
    rather than read as a thousands grouping, and results are printed without one so they can
    always be read back.
  - `preco := 120` declares a value the lines below it can use. Names are ASCII, variables are
    local to the note and resolved top-down, so a variable exists from its declaration downwards
    and a cycle cannot be written.
  - Percentages in the forms people write: `10% de 200` → `20`, `200 + 10%` → `220`,
    `200 - 10%` → `180`, and `taxa := 10%` followed by `= taxa * 200` → `20`. The contextual
    reading belongs to a `%` written on the line, never to a value that once came from one.
  - `sum`, `avg` and `count` over the block of consecutive calculation lines directly above them.
    Prose, a heading, a declaration or a failed line ends the block, so a number sitting in a
    sentence is never added to anything.
  - Results are **reactive**: the whole note is re-evaluated on every change, so editing one
    declaration moves every result under it at once, with no dependency tracking to go stale.
  - A calculation that cannot answer says so in four words beside the line — *divisão por zero*,
    *variável desconhecida*, *expressão inválida*, *nome inválido* — with no dialog, no popup and
    nothing written to the file.
  - Calculation is read from plain paragraphs only. Inside a code block, an inline code span, a
    comment, a heading, a list, a task, a quote or a callout, `= 2 + 2` is the text it is.
- Results are ProseMirror decorations and never content, so the stored `.md` holds exactly what was
  typed: no result reaches the file, `updated_at` does not move for a recalculation, opening a note
  is not an edit, undo and redo operate on the text alone, and reopening recomputes everything.
- The expression parser has no evaluator behind it — no `eval`, no `Function`, no property access,
  no call syntax, and no new dependency. `= window.location` and `= constructor.constructor(...)`
  are unspellable in the grammar rather than filtered out of it, and variables live in a `Map`, so
  no note can reach an inherited JavaScript property.
- An authoritative global Niri `Ctrl+Shift+Space` binding backed by the running application's
  `toggle-layer` GAction; the focused WebView shortcut remains available as a local fallback.
- Smart blocks, all four reachable from a **Blocos** section of the note's existing menu:
  - **Code blocks** whose language survives the Markdown round trip exactly as written. A fence
    with no language stays without one, an unknown language keeps its spelling and simply goes
    unhighlighted, and an alias stays an alias. Syntax highlighting covers sixteen grammars —
    plaintext, bash, javascript, typescript, json, html/xml, css, markdown, python, rust, c, cpp,
    java, sql, yaml, toml — and the aliases each already answers to. It is drawn as editor
    decorations, so the stored note is a plain fence with no markup in it, and it is never guessed
    for a block whose language is missing or unrecognised.
  - **Callouts** in GitHub's alert syntax, which Obsidian reads too: `NOTE`, `TIP`, `IMPORTANT`,
    `WARNING` and `CAUTION`. A callout holds several paragraphs, lists and nested blocks, and a kind
    that is not one of the five is left as the blockquote it already is, with its text intact.
  - **Comments** stored as `<!-- ... -->`, shown as a small labelled block that can be read, edited
    and removed, and never part of what the note says.
- Fenced code blocks now close with a fence longer than the longest run of backticks inside them,
  so a note containing a Markdown example is written back whole instead of being cut at the example.
- Paper types per note: **Liso**, **Pautado**, **Pontilhado**, **Quadriculado pequeno** and
  **Quadriculado grande**, chosen from the settings menu and applied at once. Plain paper is the
  original look and draws nothing.
- Pattern intensity per note — **Suave**, **Normal**, **Forte** — which changes the pattern's
  opacity and nothing else: not the paper colour, the text, the content, or the geometry.
- The pattern's ink follows the paper colour, so it stays visible on all seven papers, including
  the dark one, without competing with the note's text. Its spacing is fixed in pixels, so zoom
  scales the text and leaves the background alone.
- Interface theme: **Sistema**, **Claro** and **Escuro**, chosen from any note's menu and shared by
  every note. **Sistema** follows the desktop's colour scheme while the application runs. The theme
  dresses the application's menus, popovers, borders and focus states; a note keeps the colour and
  paper it was given, so a yellow note stays yellow under the dark theme.
- `note-it toggle-collapse-all` collapses every note still expanded, and expands them all once they
  are all collapsed. `Ctrl+Shift+M` continues to apply to the focused note alone.
- Clicking a collapsed note expands it back to its previous size, and the `☰` button expands the
  note and opens its menu in a single click.
- Typing `->` in prose becomes a real `➜`. The note stores the character itself, so it does not
  depend on a font with ligatures, and code spans and code blocks are left exactly as typed.
- Markdown task lists: typing `- [ ] ` or `- [x] ` creates a real task with a square checkbox,
  nested to any depth, with completed tasks struck through automatically.
- Per-task completion timestamps, shown as `Concluído dd/MM/aaaa HH:mm` and stored alongside the
  task in Markdown. Reopening a task clears its date; a task completed outside Note-it keeps none.
- View zoom between 75% and 200% (`Ctrl+=`, `Ctrl+-`, `Ctrl+0`, or the menu), persisted per note
  without touching the document.
- Inline text size, text colour and highlight, applied to a selection or as a stored mark, from
  compact palettes in the settings menu.
- `Ctrl+Shift+M` to collapse or expand a note, and `Ctrl+Shift+Space` to switch between
  **Sempre no topo** and **Área de trabalho** — both reusing the existing actions.
- `scripts/note-it-isolated`, which runs Note-it against a throwaway XDG tree and refuses to start
  if any directory resolves into the real store.
- Note settings popover opened from a `☰` button in the header, holding the paper colour palette
  and the collapse/expand entry.
- Collapse and expand: a note can be reduced to its header bar and restored to its previous size at
  the position where the collapsed bar was left. The collapsed state is persisted.
- Creation and modification dates shown in pt-BR after resting the cursor on the header bar.
- Project foundation, architecture documentation, and build structure.
- GTK4 + `gtk4-layer-shell` + WebKitGTK 6.0 desktop application shell skeleton.
- Local Markdown storage module with YAML front matter and atomic disk writes.
- TypeScript + Vite + Tiptap WYSIWYG editor scaffold and IPC bridge interface.
- Single-instance lifecycle and command-line interface specification.

### Changed
- Desktop-to-Overlay promotion now commits immediately even when the note is fully covered, keeps
  the focused normal application active, and avoids unconditional `present()` calls. Layer state
  persistence is coalesced for rapid toggles without weakening atomic state writes.
- Blockquotes are presented as quotations rather than dimmed italics: indented, ruled down the side,
  and set in the note's own text colour. Several lines of quoted prose used to be harder to read
  than the paragraph around them.
- HTML comments are no longer deleted by sanitization. A comment is inert data and is now content
  the note keeps, so one written by hand — or by another editor — survives a save instead of
  disappearing on the first one. An unterminated `<!--` is escaped rather than swallowing everything
  after it.
- Because an unchanged note is no longer rewritten, the note a summon brings back when everything
  is closed is the one last written in, rather than the one whose window was closed last.
- The settings menu gained **Tipo de papel**, **Intensidade** and **Tema**, each showing its
  current value on the root row, next to the entries that already did.
- Menus, popovers and focus states are now dressed by the interface theme through a `--ui-*` token
  set, instead of borrowing the note's paper colours. A text colour is previewed on a pale ground
  rather than on the popover's own surface, because the palette is tuned to be read on paper. A popover coloured from the paper could not
  survive a theme: over a yellow note a dark popover would have inherited that paper's dark text.
  Everything drawn on the paper — the note's text, its checkboxes, its highlights and the header
  buttons — still follows the paper.
- Notes gain `paper_type` and `paper_intensity` in their front matter. A note written before this
  release carries neither, opens as plain paper at normal intensity, and gains them when it is next
  saved. Changing either saves the note without touching its content or its modification date.
- `config.toml` gained `theme`. A configuration written before this release loads unchanged and
  follows the system.
- Running `note-it` now summons: it restores the notes and brings them to the front through the
  instance already running. When it is on the desktop layer it is raised so it is genuinely visible,
  without rewriting the stored layer preference.
- `Ctrl+=` and `Ctrl+-` now drive the view zoom rather than the note's base font size. The base size
  is still read from the note's front matter when it loads.
- The paper colour is now chosen from the settings menu instead of a colour dot that cycled through
  the palette on click.
- `updated_at` now tracks content edits only. Changing the paper colour, the font size, the window
  geometry, or the collapsed state no longer marks the note as modified.

### Fixed
- Keyboard shortcuts work inside a note again, and `Ctrl+Shift+Space` switches between **Área de
  trabalho** and **Sempre no topo** as it should. A layer-shell window is mapped with no focus
  widget at all, so GDK received every key press and dropped it before WebKit: nothing reached the
  page, and every in-note shortcut was dead until a click happened to focus the WebView by accident.
  Switching layer re-maps the surface and cleared that focus again, which is why the shortcut worked
  once and then stopped. The page is now made the window's focus widget whenever the surface holds
  keyboard focus, so a note is keyboard-ready as soon as the compositor gives it focus and stays
  that way across a layer change. The menu entry and `note-it toggle` were never affected — see
  "Coming back from the desktop layer" in `docs/niri.md`.
- Opening a note written by another editor, or any note ending in a list, callout or code block, no
  longer counts as editing it. Two things put newlines on the end of a note and neither is content:
  the newline a file is terminated with, and the blank line the editor's own serializer puts after a
  document that ends in a block. Comparing those spellings literally made a plain open and close
  rewrite the file and move `updated_at` once. A note is now compared and stored in one canonical
  spelling, and stored files are terminated the way every other tool writes them. A real edit still
  moves `updated_at` exactly as before.
- A note created right after summoning Note-it is no longer filed behind every window. A summon
  lifts the notes to the overlay while deliberately keeping the stored preference as it was, so the
  preference read "desktop" while every surface was on the overlay — and a new note was opened from
  the preference, on the bottom layer, invisible moments after the user asked for Note-it. It now
  opens on the layer its siblings are actually on.
- `state.json` is no longer reported as unsaved when it was in fact written. It never got the commit
  point rule the notes were given in Phase 3.4R.2: a directory sync failing *after* the rename was
  reported as a failed save, and every caller treats that as "nothing was written" — closing a note
  rolled its state back and left the window open, and hiding refused to close the windows — while
  the file already held the new state. Notes, window state and configuration now share one atomic
  write with one commit-point rule.
- `config.toml` is replaced whole or not at all. It was written straight over the real file, which
  truncates it first, so an interrupted write left a half-written configuration — and loading falls
  back to the defaults without a word, silently resetting the theme and every other preference.
- A note whose save *succeeded* is no longer treated as unsaved. The rename that replaces the note
  file is the point at which the change becomes real, and syncing the notes directory happens after
  it. A failure of that sync was being reported as a failed save, so the application kept the old
  note in memory while the file already held the new one — the mirror image of the divergence just
  fixed. The sync's failure is now reported as what it is: the save happened and may not survive a
  power loss, which the next save of any note repairs on its own.
- A note whose save failed is no longer treated as saved. The document held in memory was updated
  before the write was confirmed, so a failed write left memory holding text the file never
  received — and the identical-content check added just before it then compared the next attempt
  against that phantom state and reported success without writing, which could lose the edit
  silently at close. Content and appearance changes are now prepared on a copy and adopted only
  once the file has actually been written, so a failed save leaves the note describing exactly what
  is stored and the next attempt writes for real. A failed save no longer leaves its temporary file
  behind in the notes directory either.
- Opening a note and closing it no longer counts as editing it. Closing and the flushes before hide
  and quit all send whatever the editor holds, edited or not, and every one of them moved
  `updated_at`. The single path they funnel through now compares the incoming text with what is
  already stored: identical content records nothing and does not rewrite the file, while a real
  change is recorded exactly as before. `created_at` was never affected.
- Highlighted text is readable on a dark note. The highlight extension renders an inline
  `color: inherit`, which beat the stylesheet rule meant to darken it, so highlighted text kept
  inheriting the paper's white. The mark now paints its own dark foreground inline. An explicit
  text colour is still recorded in the note and reappears when the highlight is removed.
- The settings menu is no longer clipped on a collapsed note. The note expands first, so the menu
  opens on a surface tall enough to hold it.
- Three text colours were darkened so every one of them stays readable on every highlight and on
  every paper colour.
- Closing the last note no longer makes it unreachable. Running Note-it again reopens the note that
  was used last instead of creating a blank one; the closed note's content was never lost, but there
  had been no way back to it.
- A fast resize no longer exposes a dark strip before the note repaints: the window is backed with
  the note's own paper colour, which is kept in step when the colour changes.
- Typing `- [ ] ` produces a task item instead of a bullet containing the literal `[ ]`.
- Nested inline spans no longer lose the inner mark when a note is reloaded.
- Pointer gestures emit geometry deltas only while exactly one pointer is captured. A lost pointer
  capture or a move reporting no button held now ends the gesture, and an animation frame left over
  from a finished gesture can no longer move the window.
- Notes whose front matter omits `created_at` / `updated_at` keep opening; the unknown date is
  reported as unknown instead of being replaced by a fabricated one.
