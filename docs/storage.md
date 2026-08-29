# Storage and XDG Directories

Note-it adheres to the XDG Base Directory Specification:

| Path | Purpose | Example Fallback |
| --- | --- | --- |
| `$XDG_DATA_HOME/note-it/notes/` | Persisted Markdown note files (`<uuid>.md`) | `~/.local/share/note-it/notes/` |
| `$XDG_CONFIG_HOME/note-it/config.toml` | User configuration options | `~/.config/note-it/config.toml` |
| `$XDG_STATE_HOME/note-it/state.json` | Window geometry, active mode, and transient UI state | `~/.local/state/note-it/state.json` |
| `$XDG_RUNTIME_DIR/note-it/` | Unix domain sockets / IPC runtime files | `/run/user/<uid>/note-it/` |

## Note Appearance Fields

| Field | Meaning | Default when absent |
| --- | --- | --- |
| `color` | Paper colour: `yellow`, `blue`, `green`, `pink`, `purple`, `gray`, `black` | `yellow` |
| `paper_type` | Background pattern: `blank`, `lined`, `dotted`, `grid-small`, `grid-large` | `blank` |
| `paper_intensity` | How strongly that pattern is drawn: `subtle`, `normal`, `strong` | `normal` |
| `font_size` | Base text size of the note | `15` |

These describe how the note is displayed, so they live in the front matter beside the note rather
than in `state.json`, and they travel with the file. Changing any of them saves the note without
touching its content or its `updated_at`, and — like a content save — the change is adopted in
memory only once it has been written, so one that fails is not left behind as though it had been
stored.

Each is stored as a plain string and resolved against the supported set on read, so a value written
by a newer version — or by hand — degrades to the default instead of failing the parse and taking
the note down with it. A note written before these fields existed opens as plain paper at normal
intensity, and gains the fields the next time it is saved.

`paper_intensity` is kept even for `blank`, where it has no pattern to act on, so switching paper
back and forth never loses the choice.

## Application Configuration

`config.toml` holds preferences shared by every note:

| Field | Meaning | Default |
| --- | --- | --- |
| `default_color` | Paper colour given to a new note | `yellow` |
| `default_font_size` | Base text size given to a new note | `15` |
| `default_width`, `default_height` | Size given to a new note | `360`, `300` |
| `autosave_interval_ms` | Debounce before an edit is written | `300` |
| `theme` | Interface theme: `system`, `light`, `dark` | `system` |

The theme is the appearance of the application's chrome — menus, popovers, borders, focus states —
and is deliberately **not** per note: a note keeps the colour and paper it was given whatever the
theme is. `system` follows the desktop's colour scheme, and keeps following it while the
application runs.

## Note Front Matter Timestamps

`created_at` records when the note was created and never changes afterwards.
`updated_at` records the last change to the note's **content**.

Content means the Markdown that is persisted. If that text differs from what is
already stored, the change is recorded — whether it came from typing, a
heading, a list, a task, bold, italic, strikethrough, a text colour, a
highlight, or an inline size, since all of those are written into the note.

Everything else deliberately leaves `updated_at` alone:

- appearance: paper colour, paper type, pattern intensity, font size;
- the interface theme, which is not stored in the note at all;
- window and view state: drag, resize, zoom, collapse/expand, layer mode;
- opening the menu, or hovering the header;
- **and visiting the note.** Opening and closing it, summoning, hiding,
  showing or quitting without editing all leave it untouched.

That last point is enforced rather than assumed. Closing and flushing both send
whatever the editor holds, edited or not, so the single path all content saves
funnel through compares the incoming text with what is already stored and does
nothing when they match. An unchanged note is not rewritten at all: no temp
file, no rename, no fsync, and the file keeps its own modification time.

That comparison is only sound while the note held in memory is the note that
is on disk, so it is kept that way: a change is prepared on a copy, written,
and adopted in memory only once the write has succeeded. A save that fails
therefore leaves the note describing exactly what is stored, and the same text
arriving again — which is what every one of those paths resends — is still a
difference and is written for real. A payload is never treated as stored
because it matches a state that came from a write that never landed, and
save-and-close never finalises a close over a save that failed.

Both fields are optional on read. A note whose front matter omits them still
opens; the missing value is reported as unknown (`—`) rather than replaced by a
fabricated date, and re-saving the note does not invent one either.

## Which Note a Summon Brings Back

When every note is closed, the application reopens the most recently written
one, ordered by each note's own `updated_at` — the front matter field that
records the last change to its **text**. Closing a note you did not type in
does not move it to the front, because an unchanged note is never rewritten;
neither does changing its colour, paper, pattern intensity or font size, which
rewrites the file but is not an edit. A note with no readable `updated_at` —
one written before the field existed, one with no front matter, one whose
header cannot be parsed — falls back to the file's own `mtime`, which is what
every note used before there was a field to read. Ties are broken by
identifier, so the same store always lists in the same order.

The same ordering is what search and the quick switcher show, so "most recent"
means one thing throughout the application. Reading it costs a bounded read of
each note's head; nothing is written, and an unreadable header costs that note
its timestamp rather than failing the listing.

This is the intended reading of "the note used last" — the note actually
written in. Reopening, summoning and single-instance dispatch are unaffected.
A future need for "the note I last had open", as something distinct from "the
note I last wrote in", belongs in `state.json` as explicit state rather than in
a filesystem timestamp.

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
| `zoom_percent` | View scale of the note content, 75–200, default 100 |

Every field has a default, so a `state.json` written by an earlier version
loads unchanged: absent `collapsed` means expanded, and absent expanded
geometry falls back to the default note size.

## Inline Formatting in Markdown

Markdown has no syntax for colour, highlight or font size, so these are stored as a small set of
controlled HTML elements. Only Note-it's own attributes are accepted, and only with values from the
corresponding whitelist — anything else is dropped when the note is loaded.

| Formatting | Representation | Accepted values |
| --- | --- | --- |
| Text colour | `<span data-note-it-color="#2563EB">` | `#rgb` / `#rrggbb` |
| Highlight | `<mark data-note-it-highlight="#FDE68A">` | `#rgb` / `#rrggbb` |
| Text size | `<span data-note-it-font-size="22">` | 12, 14, 16, 18, 22, 26, 32 |
| Task completion | `- [x] texto <!-- note-it:completed_at=… -->` | ISO 8601 with an offset or `Z` |

None of these are ever visible as markup in the editor. The task metadata comment is the only HTML
comment the sanitizer preserves; every other comment is still removed.

## Atomic File Writing

To prevent data corruption during unexpected power loss or process crashes:
1. Write note contents to a temporary file (`.tmp.<uuid>.<nanos>`) in the same directory.
2. Flush and sync data to disk.
3. Atomically rename/replace the destination file using `std::fs::rename`.
4. Sync the notes directory, so the rename itself is durable.

**The rename is the commit point.** Either it lands and the note is the new one, or it does not and
the note is still the previous one; there is no state in between, and a reader never sees a torn
file. If anything up to and including the rename fails, the temporary file is removed rather than
left in the notes directory, since nothing else would ever collect it.

A save reports failure for anything before or at the rename, and success from the rename onwards.
That is the rule the in-memory document depends on: it is replaced only by a version that has
actually been written, and it is always replaced by one that has.

Step 4 comes after the commit point. The note's bytes are already on stable storage by then —
step 2 syncs them — so what the directory sync buys is that the **rename** survives a power loss.
If it fails, the save still succeeded and is still reported as such; a warning is printed, because
what is in doubt is durability, not whether the note was written. Calling it a failed save would
leave the application describing a note the file no longer holds.

Nothing tracks a missed sync. Syncing a directory flushes every pending entry in it, not just the
last one, so the next successful save of any note makes the earlier rename durable too.

What this does **not** claim: the sync is not retried, a save whose sync failed is not guaranteed
durable, and the note file is not re-synced after the rename. The guarantee is that a note is never
half-written and never silently reverts while the application is running; a power loss inside that
window can cost the last save, never the file.
