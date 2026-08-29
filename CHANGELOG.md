# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
