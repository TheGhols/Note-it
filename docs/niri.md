# Niri Compositor Integration

Note-it is designed and tested for the [Niri](https://github.com/YaLTeR/niri) scrollable-tiling Wayland compositor.

## Layer Shell Setup

Note-it registers windows with the Wayland Layer Shell namespace `note-it`.

- **Desktop Layer:** Notes attach to layer `bottom` with `exclusive_zone = 0`, keeping them on the background behind active Niri tiles.
- **Overlay Layer:** Notes switch to layer `overlay`, appearing over active workspaces for immediate editing.

## Calling Note-it From Anywhere

Note-it's in-application shortcuts are ordinary key events inside the note's
WebView. A Wayland client only receives key events while it holds keyboard
focus, so those shortcuts work when the note itself is focused and do nothing
while the browser or the terminal is in front. Summoning the note therefore has
to come from the compositor.

The authoritative layer toggle is a compositor binding. It activates the
`toggle-layer` GApplication action on the already-running Note-it process, so
it does not depend on a note, GTK window or WebView holding focus.

```text
Ctrl+Shift+Space
    ↓
Niri global binding (repeat=false, allow-inhibiting=false)
    ↓  gapplication action io.github.theghols.NoteIt toggle-layer
the already-running Note-it instance
    ↓
one shared live layer decision applied to every note surface
```

## Recommended Keybindings

Add the following to the configuration Niri actually loads. That may be
`~/.config/niri/config.kdl`, a file included from it such as `binds.kdl`, or the
path selected by `NIRI_CONFIG`. Run `niri validate` after editing it.

```kdl
// Spawn background daemon on compositor startup
spawn-at-startup "note-it" "--background"

binds {
    // Authoritative global Desktop ↔ Overlay toggle. `Space` is the XKB name.
    Ctrl+Shift+Space repeat=false allow-inhibiting=false {
        spawn "gapplication" "action" "io.github.theghols.NoteIt" "toggle-layer"
    }

    // Summon Note-it from any application: restores the notes and brings
    // them to the front. This is the binding to reach for.
    Mod+Shift+N { spawn "note-it"; }

    // Collapse every note to its bar, or expand them all again
    Mod+Shift+M repeat=false { spawn "note-it" "toggle-collapse-all"; }

    // Quick create new note
    Mod+Alt+N { spawn "note-it" "new"; }
}
```

Keep an existing `Mod+Shift+D` alias if it already belongs to Note-it and does
not conflict with another application. It may continue to call `note-it
toggle`; it is not needed for the core workflow.

The desktop entry `io.github.theghols.NoteIt.desktop` must be installed in an
XDG applications directory so `gapplication` can resolve the application ID:

```bash
install -Dm644 resources/io.github.theghols.NoteIt.desktop \
    ~/.local/share/applications/io.github.theghols.NoteIt.desktop
update-desktop-database ~/.local/share/applications
```

`note-it toggle` remains the CLI fallback and reaches the same shared
transition, but launching a second GTK process makes it slower than the direct
application action.

## What a Summon Does to the Layer

A `bottom` layer surface is always painted below ordinary windows; there is no
way to raise it while keeping it on that layer. A note left on the desktop
therefore cannot be made visible over the browser without moving it to the
`overlay` layer.

Summoning raises the notes to `overlay` **without rewriting the stored
preference**. The note is genuinely visible, and `note-it toggle`,
`Ctrl+Shift+Space` and the next restart all still reflect the layer the user
chose. The elevation lasts until the next explicit layer change or restart.

`note-it show` is different on purpose: it is an explicit request to put the
notes in overlay mode, and it does store that as the preference.

### Coming Back from the Desktop Layer

The Niri binding above is the real `Ctrl+Shift+Space` workflow. It works when a
browser, terminal or editor is focused, when the note is completely covered,
and when the note has never been clicked since it moved to the desktop.

```kdl
Ctrl+Shift+Space repeat=false allow-inhibiting=false {
    spawn "gapplication" "action" "io.github.theghols.NoteIt" "toggle-layer"
}
```

The WebView still handles `Ctrl+Shift+Space` as a local fallback when the note
already owns focus. It is useful, but it is not authoritative and cannot make
a covered `bottom` surface receive keyboard input.

On Niri 26.04 with layer-shell protocol version 4, changing `bottom` to
`overlay` does not inherently recreate the surface. An occluded bottom surface
can nevertheless wait for a frame before its layer request is committed. To
make promotion immediate, Note-it deliberately remaps only that direction,
with keyboard interactivity temporarily disabled so the browser keeps focus.
`overlay` to `bottom` uses the live protocol transition directly. Neither live
path blindly calls `present()`.

## Collapsing One Note or All of Them

`Ctrl+Shift+M` inside a note collapses that note alone; it is a key event in
the note's own WebView and only reaches the note holding keyboard focus.

Collapsing every note is a compositor keybinding for the same reason a summon
is: no note may be focused when the user wants them all out of the way. It runs
`note-it toggle-collapse-all`, which collapses everything still expanded, and
expands everything once they are all collapsed. Each note keeps its own
`collapsed` flag and its own expanded size in `state.json`.

The command has to be reachable from the compositor, which spawns it with a
plain environment. Installing the binary somewhere on `PATH` — or a launcher
pointing at the build — is part of setting the keybinding up; a bind naming a
command that does not resolve fails silently.

Because a spawned invocation is handed to the running instance through the
single-instance dispatcher, the environment it is spawned with does not matter:
the instance that already owns the notes is the one that acts on them.

## The System Theme and the Desktop

**Tema → Sistema** follows the desktop's colour scheme, read inside the WebView through
`prefers-color-scheme`. WebKitGTK derives that from the GTK settings of the session the
application was launched in, so a Wayland session that reports no preference simply resolves to
the light theme — the notes are always fully styled either way.

The preference is watched while the application runs, so switching the desktop between light and
dark reaches open notes without a restart. **Claro** and **Escuro** are explicit choices and
ignore the desktop entirely.

The theme is global and lives in `config.toml`. It dresses the application's menus and popovers;
each note keeps the paper colour and pattern it was given, so a yellow note stays yellow on a dark
desktop.
