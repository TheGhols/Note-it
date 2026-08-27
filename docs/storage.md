# Storage and XDG Directories

Note-it adheres to the XDG Base Directory Specification:

| Path | Purpose | Example Fallback |
| --- | --- | --- |
| `$XDG_DATA_HOME/note-it/notes/` | Persisted Markdown note files (`<uuid>.md`) | `~/.local/share/note-it/notes/` |
| `$XDG_CONFIG_HOME/note-it/config.toml` | User configuration options | `~/.config/note-it/config.toml` |
| `$XDG_STATE_HOME/note-it/state.json` | Window geometry, active mode, and transient UI state | `~/.local/state/note-it/state.json` |
| `$XDG_RUNTIME_DIR/note-it/` | Unix domain sockets / IPC runtime files | `/run/user/<uid>/note-it/` |

## Note Front Matter Timestamps

`created_at` records when the note was created and never changes afterwards.
`updated_at` records the last change to the note's **content**. Appearance and
window state — paper colour, font size, drag, resize, collapse/expand, opening
the menu, hovering the header — deliberately leave `updated_at` alone.

Both fields are optional on read. A note whose front matter omits them still
opens; the missing value is reported as unknown (`—`) rather than replaced by a
fabricated date, and re-saving the note does not invent one either.

## Window State Fields

`state.json` stores one entry per note:

| Field | Meaning |
| --- | --- |
| `x`, `y` | Position of the note on its monitor |
| `width`, `height` | Current surface size; while collapsed, `height` is the header bar height |
| `is_open` | Whether the note is restored on startup |
| `monitor` | Connector name the note belongs to |
| `collapsed` | Whether the note is reduced to its header bar |
| `expanded_width`, `expanded_height` | Size to restore on expand; only meaningful while `collapsed` |

Every field has a default, so a `state.json` written by an earlier version
loads unchanged: absent `collapsed` means expanded, and absent expanded
geometry falls back to the default note size.

## Atomic File Writing

To prevent data corruption during unexpected power loss or process crashes:
1. Write note contents to a temporary file (`.tmp.<uuid>.<nanos>`) in the same directory.
2. Flush and sync data to disk.
3. Atomically rename/replace the destination file using `std::fs::rename`.
