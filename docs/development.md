# Development Guide

## Build Prerequisites

Ensure all required system packages are installed on your Linux distribution:

```bash
# Arch Linux
sudo pacman -S --needed gtk4 gtk4-layer-shell webkitgtk-6.0 rust nodejs pnpm pkgconf base-devel dbus
```

`dbus` provides `dbus-daemon` and `dbus-send`, which the isolated test harness needs to give a test
run a session bus of its own. See **Running Against a Throwaway Store** below.

## Building the Project

1. **Build the Frontend Assets:**
   ```bash
   cd ui
   pnpm install
   pnpm build
   cd ..
   ```

2. **Build the Rust Native Binary:**
   ```bash
   cargo build
   ```

3. **Run Tests:**
   ```bash
   cargo test
   cd ui && pnpm test
   ```

4. **Code Quality Checks:**
   ```bash
   cargo fmt --check
   cargo clippy -- -D warnings
   cd ui && pnpm lint
   ```

## Running Against a Throwaway Store

Any experimental or integration run must go through the isolation helper rather than a hand-written
set of environment variables:

```bash
scripts/note-it-isolated                          # throwaway tree, removed on exit
scripts/note-it-isolated --keep                   # keep the tree for inspection
scripts/note-it-isolated -- new                   # pass arguments through to note-it

# A session that outlives one command, which is what a single-instance test needs:
scripts/note-it-isolated --root /tmp/t -- --background &
scripts/note-it-isolated --root /tmp/t -- new     # reaches that same instance
scripts/note-it-isolated --root /tmp/t --verify   # assert it is on the private bus
scripts/note-it-isolated --root /tmp/t --stop     # quit it and stop the bus
```

### Isolating XDG is not enough

Note-it is a single-instance `GApplication`, and single-instance is a well-known name on the
**session bus**. The second process to start finds the name already owned, hands its command line
to the owner over D-Bus, and exits — and the owner then does the work, in whatever store *it* was
started with.

So overriding the four XDG variables configures only the process the helper launches. **If a
Note-it daemon is already running on the real session bus, that process never opens a store at
all**: it forwards the command and quits, and the real daemon writes to the real store. The XDG
isolation is real and completely beside the point.

That is not hypothetical. During Phase 3.7 physical testing a daemon was already running, every
isolated command was forwarded to it, and a test note was created in the user's own notes
directory. Phase 3.7R is the fix.

The helper therefore isolates **both**:

- **XDG** — all four of `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_STATE_HOME` and `XDG_CACHE_HOME`,
  set together. Overriding only some leaves the rest resolving to the real store.
- **D-Bus** — a private `dbus-daemon` of its own, with `DBUS_SESSION_BUS_ADDRESS` pointing at it and
  `DBUS_STARTER_ADDRESS`/`DBUS_STARTER_BUS_TYPE` cleared so GIO cannot fall back to the real
  session. On that bus the well-known name is unowned, so the isolated process becomes the primary
  instance and does its own work in its own store.

The real daemon never has to be stopped, and never notices.

`XDG_RUNTIME_DIR` is deliberately **not** overridden: `WAYLAND_DISPLAY` resolves inside it, so
replacing it would break the display connection. Setting `DBUS_SESSION_BUS_ADDRESS` is what decides
the bus, and it always wins over the runtime directory's socket.

### Fail-closed

Every check runs *before* Note-it is started, and there is no path that falls back to "well, at
least XDG is isolated":

| exit | meaning |
| --- | --- |
| 90 | a configured directory is, or sits inside, a real XDG base directory or the home directory |
| 91 | no `note-it` binary; run `cargo build` |
| 92 | the private bus could not be started, could not be reached, or turned out to be the real one |
| 93 | the launched process does not carry the isolated environment |

Exit 93 is read back from the kernel: the process is started, `/proc/<pid>/environ` is checked for
the four XDG variables and the private bus address, and the process is killed if any of them is not
the isolated one.

### Persistent sessions

With `--root DIR` the private bus is recorded under `DIR/session` and **reused** by every later
invocation naming the same `DIR`, so a daemon started by one command and a `new` sent by the next
land on the same instance. End it with `--stop`, which quits the isolated instance on its own bus
and stops that bus; a caller-supplied `--root` is never deleted. Without `--root`, everything is
torn down when the command returns.

### The regression test

`scripts/test-isolation` reproduces the Phase 3.7 incident and asserts it cannot happen: it stands
up a session of its own — bus, store and, where there is a display, a genuine `note-it --background`
daemon owning the real well-known name — fingerprints that store to the nanosecond, runs the harness
against it, and checks that the note landed only in the throwaway store and that nothing in the
ambient one moved. It runs as part of `cargo test` via `tests/isolation.rs`, and needs `dbus-daemon`
and `dbus-send`. The daemon half is skipped, out loud, where there is no display.

Running it locally will briefly open a real note window: that is the point of the fidelity half, and
it is pointed at a throwaway store the whole time.

### Measuring search rather than guessing at it

The claim that Note-it needs no search index is a test, not a memory:

```bash
cargo test --release searching_a_thousand_notes -- --nocapture
```

It builds a thousand notes in a temporary directory, runs four queries — one matching a few notes,
one matching all of them, one matching none, one with accents — end to end through listing,
reading, folding, matching and snippets, prints each timing and asserts the notes' modification
times did not move. On the development machine the whole scan is around 26–40 ms per query in
release and under 200 ms in debug.

Phase 3.8R roughly doubled that from the 18–20 ms it was, and the cause is not the removed scan
ceiling: it is ordering by each note's own `updated_at`, which means opening every note's header
and parsing it. About half the added time is the reads and half is the YAML. It buys "most recent"
meaning the same thing everywhere — a repainted note is not a written-in note — and 40 ms is still
well inside the 120 ms the palette waits before asking at all.

That is the number ADR-027 rests on. If it stops being comfortable, the evidence for adding an
index will be in the test output, which is where it should be — not in a hunch.

### Inspecting a backup

A snapshot is a directory of ordinary files, which is the whole reason it is one:

```bash
ls ~/.local/share/note-it/backups/
cat ~/.local/share/note-it/backups/*/manifest.json
diff -r ~/.local/share/note-it/backups/<data>/notes ~/.local/share/note-it/notes
```

The recovery procedure — including recovering a single note rather than the whole store — is in
[docs/storage.md](storage.md#recovering-from-a-snapshot). It is `cp`, with the application closed.
There is no one-click restore in the application, and
`a_snapshot_round_trips_into_a_fresh_isolated_store` is what proves the procedure works: it copies a
snapshot into an empty XDG tree exactly that way and opens the result.

To exercise the twenty-four hour rule against a running daemon without waiting a day, age the newest
snapshot — the store's own record of when it was last backed up is that snapshot's manifest — and
restart:

```bash
scripts/note-it-isolated --root /tmp/t --stop
# rename the snapshot directory and set created_at in its manifest.json to > 24 h ago
scripts/note-it-isolated --root /tmp/t -- --background &
scripts/note-it-isolated --root /tmp/t -- new     # the next change takes a fresh snapshot
```

### GTK's compose table

A cold `XDG_CACHE_HOME` makes GTK rebuild its compose table, which produces the one-off
`Can't handle >16bit keyvals` warning burst described in ADR-006. It is expected on the first run
against a fresh tree and disappears on the next one.

