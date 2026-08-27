# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
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
- The paper colour is now chosen from the settings menu instead of a colour dot that cycled through
  the palette on click.
- `updated_at` now tracks content edits only. Changing the paper colour, the font size, the window
  geometry, or the collapsed state no longer marks the note as modified.

### Fixed
- Pointer gestures emit geometry deltas only while exactly one pointer is captured. A lost pointer
  capture or a move reporting no button held now ends the gesture, and an animation frame left over
  from a finished gesture can no longer move the window.
- Notes whose front matter omits `created_at` / `updated_at` keep opening; the unknown date is
  reported as unknown instead of being replaced by a fabricated one.
