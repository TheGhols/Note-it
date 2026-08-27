# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
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
