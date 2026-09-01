# Storage and XDG Directories

Note-it adheres to the XDG Base Directory Specification:

| Path | Purpose | Example Fallback |
| --- | --- | --- |
| `$XDG_DATA_HOME/note-it/notes/` | Persisted Markdown note files (`<uuid>.md`) | `~/.local/share/note-it/notes/` |
| `$XDG_DATA_HOME/note-it/trash/` | Deleted notes, waiting to be restored | `~/.local/share/note-it/trash/` |
| `$XDG_DATA_HOME/note-it/assets/` | Images the notes hold, one directory per note | `~/.local/share/note-it/assets/` |
| `$XDG_DATA_HOME/note-it/backups/` | Local snapshots of the recoverable store | `~/.local/share/note-it/backups/` |
| `$XDG_DATA_HOME/note-it/study.json` | Versioned schedules and aggregate study activity | `~/.local/share/note-it/study.json` |
| `$XDG_CONFIG_HOME/note-it/config.toml` | User configuration options | `~/.config/note-it/config.toml` |
| `$XDG_STATE_HOME/note-it/state.json` | Window geometry, active mode, and transient UI state | `~/.local/state/note-it/state.json` |
| `$XDG_RUNTIME_DIR/note-it/<store>/` | Writer lease and control socket for one store | `/run/user/<uid>/note-it/<store>/` |

`study.json` contains only opaque SHA-256 review keys, levels, absolute UTC timestamps, ratings and
daily counters keyed by local civil date. Questions, answers, Markdown, titles, HTML, image bytes and
absolute paths never enter it. Missing means an empty history; corrupt or newer data is left byte for
byte in place and makes Study unavailable rather than being replaced. Each rating builds a next
state and commits it with the same atomic-write primitive as notes before the application adopts it.

## Write Coordination Runtime

Exactly one Note-it process may write a store at a time. The claim is an advisory `flock` on a lock
file in the runtime directory, never the existence of a file: a process that crashes releases it the
moment the kernel closes its descriptors, and a lock file left behind by a dead process blocks
nobody.

```text
$XDG_RUNTIME_DIR/note-it/            0700
  <store key>/                       0700
    store                            0600   the notes directory this key stands for
    writer.lock                      0600   the lease
    control.sock                     0600   the authority's private socket
```

`<store key>` is the FNV-1a 64 digest of the notes directory path, written as sixteen lowercase
hexadecimal characters. Keying by store is what lets an isolated test store and the real store have
one legitimate writer each at the same time, without either waiting for the other.

Nothing here belongs to the store. It describes this boot, is meaningless after a restart, and is
never backed up. When the session has no `$XDG_RUNTIME_DIR` at all the fallback is
`/tmp/note-it-<uid>`, scoped to the user rather than a name anyone could take first — and either way
both directories are refused if they are a symlink, belong to another user, or are reachable by one.

The desktop instance takes the lease before it can save anything and holds it until the process ends.
`noteit` takes it for the length of one command when it is free; when it is held it sends the change
to the holder over `control.sock`; when it is held and unreachable it changes nothing and says so.
See ADR-038.

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
| `zoom_percent` | View scale of the note content, 75–300, default 100 |

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

## The Trash

Deleting a note moves its file out of the active store:

```text
notes/<uuid>.md   →   trash/<uuid>.md
                      trash/<uuid>.json   (when it was deleted)
```

A note in `trash/` is not a note. It is not listed, not searched, not offered by
the quick switcher, not restored on startup and not brought back by a summon —
not because each of those excludes it, but because every one of them reads
`notes/`, and the file is not there any more.

**The move is the commit point.** The sequence is:

```text
flush the note   →   move the file   →   update the window state   →   close the window
```

Everything before the move can fail with the note still open, live and
editable — including, in particular, a flush that could not write the latest
text. A note whose text is not safe is never made to disappear. From the move
onwards the note *is* in the trash, so nothing afterwards reports otherwise: the
window state write is best effort, and the window closes either way.

**The file is not read, parsed or rewritten.** Moving to the trash is a
`rename`, and restoring is a `hard_link` plus a `remove_file`; both preserve the
note byte for byte, front matter, appearance, tasks, links and calculations
included. A note whose front matter is damaged — one Note-it cannot even open —
still goes to the trash and still comes back unchanged.

**Restoring never overwrites a live note.** The restore creates the name in
`notes/` with `hard_link`, which fails if the name already exists. That is a
property of the syscall, not a check that could be raced: if a note carrying the
same identifier is already live, neither file is touched and the reader is told
so.

**Neither is an edit.** `updated_at` does not move when a note is deleted or
restored, so a recovered note returns to the position in the quick switcher it
had rather than pretending to have just been written in. Its window state entry
is kept and marked closed, so it also comes back the size and place it was.

**When it was deleted lives beside the note, never inside it.** The
`<uuid>.json` sidecar holds `deleted_at` and nothing else. If it is missing or
unreadable, the trash listing falls back to the file's own modification time;
nothing is written to repair it. Anything in `trash/` that is not a `<uuid>.md`
is ignored by the listing.

**There is no permanent delete and no "empty the trash".** The trash grows until
you remove files from it yourself, which is a deliberate choice for a phase
about recovery — and possible with any file manager, because a note in the trash
is an ordinary `.md` on disk.

## Local Backups

A snapshot is a directory of ordinary files:

```text
backups/2026-08-29T09-30-00Z/
  manifest.json
  notes/<uuid>.md …
  trash/<uuid>.md, <uuid>.json …
  assets/<note-uuid>/<asset-uuid>.<ext> …
  config.toml
  state.json
  study.json
```

`manifest.json` records the version, when the snapshot was taken, whether it was
automatic or manual, how many notes, trash entries and images it holds, and
whether the configuration, window state and study history were present. A directory in
`backups/` counts as a snapshot only if it is a real directory, its name does
not begin with `.`, and it holds a readable manifest.

Manifest **version 3** is version 2 plus the optional study-history flag; version 2 is version 1
plus the image count. Older snapshots remain valid because both later fields default to absent/zero.

**What goes in:** `notes/`, `trash/`, `assets/`, `config.toml`, `state.json`, and `study.json` when it
exists. An existing study file is recoverable data: if it cannot be copied as a regular file, the
snapshot is not committed as complete.

A note that says `![](../assets/…)` is only half a note without the file that
reference points at, so `assets/` is copied with the same guarantees as the
notes themselves: the same shape, one directory per note, byte for byte, and a
snapshot that could not copy one is not committed at all. An image no note
points at any more is copied too — a backup is a snapshot of the managed store,
not a decision about which of its files are still wanted.

`assets/` is copied more strictly than `notes/` is, and deliberately. A person
may reasonably have put something of their own in `notes/`, so an oddity there
is skipped with a warning; `assets/` is written by Note-it and by nothing else,
so anything that is not `<note-uuid>/<asset-uuid>.<ext>` means the store is not
in the state Note-it believes it to be, and the backup fails rather than
quietly omitting managed content while reporting success. A store written
before images existed has no `assets/` at all, and that is a store with no
pictures rather than a broken one.

**What never goes in:** `backups/` itself, so a snapshot can never contain
snapshots; anything whose name begins with `.`, which is what keeps a `.tmp.…`
from an interrupted save out of a snapshot; anything that is not a regular file;
and anything reached through a symbolic link, which is never followed — a
crafted entry in the store cannot make the backup copy `/etc` or a home
directory.

**When it happens.** At most one automatic snapshot per 24 hours, taken **before
the first eligible change** after that window has passed — a note save or a move
to the trash. Taking it first is the point: what a backup is for is going back to
how things were, so the moment worth capturing is the one before an edit. There
is no timer and no thread; a daemon nobody is using does no work at all, and a
daemon left open for days takes its snapshot the moment its owner starts typing
again. "When was the last backup" is answered by the newest snapshot's own
manifest, so there is no bookkeeping file that could disagree with the disk.

**Manual backup.** *Dados › Fazer backup agora* takes one immediately, is never
skipped, and always reports success or failure. It satisfies the 24-hour rule
like any other snapshot.

**Atomicity.** A snapshot is built in `backups/.tmp.<pid>.<n>/` and renamed into
place whole; the rename is the commit point. A process killed halfway leaves a
`.tmp.…` directory, which is not a snapshot — wrong name, no manifest — and the
next backup removes it. Only directories carrying that prefix are ever swept.

**Retention.** Seven snapshots are kept, in one pool whatever made them, and
retention runs **only after a new snapshot has been committed**. An old backup is
never deleted to make room for one that might then fail. A snapshot that cannot
be removed is reported and the new backup still stands.

**Failure.** A snapshot that cannot be made never blocks a save: the error goes
to `stderr` and the note is written normally, and the attempt is retried at the
next eligible change rather than on every keystroke.

### Recovering From a Snapshot

There is deliberately no one-click "restore everything" in the application:
putting a snapshot back over a live store is a multi-file transaction, and a
button for it would be the most destructive control Note-it has. The manual
procedure, with the application closed, is:

```bash
note-it quit                       # nothing may be running

SNAP=~/.local/share/note-it/backups/2026-08-29T09-30-00Z
cat "$SNAP/manifest.json"          # check it is the snapshot you want

# Keep what is there now, so this step is itself reversible.
mv ~/.local/share/note-it/notes  ~/.local/share/note-it/notes.antes
mv ~/.local/share/note-it/trash  ~/.local/share/note-it/trash.antes
mv ~/.local/share/note-it/assets ~/.local/share/note-it/assets.antes
mv ~/.local/share/note-it/study.json ~/.local/share/note-it/study.json.antes  # if present

cp -a "$SNAP/notes"  ~/.local/share/note-it/notes
cp -a "$SNAP/trash"  ~/.local/share/note-it/trash
cp -a "$SNAP/assets" ~/.local/share/note-it/assets            # if present
cp -a "$SNAP/config.toml" ~/.config/note-it/config.toml       # if present
cp -a "$SNAP/state.json"  ~/.local/state/note-it/state.json   # if present
cp -a "$SNAP/study.json"  ~/.local/share/note-it/study.json   # if present
```

To recover a **single** note, copy just that `<uuid>.md` out of the snapshot's
`notes/` directory — and, if it holds images, the matching
`assets/<note-uuid>/` directory beside it. The note refers to its pictures by a
path relative to `notes/`, so the two travel together and neither needs
editing.

That the result is readable is not a hope: `a_snapshot_round_trips_into_a_fresh_isolated_store`
copies a snapshot into an empty XDG tree exactly this way, opens it, and checks
the notes, identifiers, Markdown, trash, configuration, window state and study schedule all came
back.

### What a Local Backup Does and Does Not Protect Against

It protects against: an accidental deletion, a logical corruption, an edit you
want to undo, a version you want to go back to.

It does **not** protect against a dead disk, a lost or stolen machine, or a
filesystem that fails as a whole — the snapshots are on the same disk as the
notes. It is not encrypted. Anyone who needs protection from hardware failure
needs a copy on other hardware, and Note-it does not make one.

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
