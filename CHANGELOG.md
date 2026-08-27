# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Typing `->` in prose becomes a real `→`. The note stores the character itself, so it does not
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
