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

The path is the command line: every invocation reaches the already running
instance through Note-it's single-instance dispatcher, so a keybinding never
starts a second application.

```text
Niri keybinding
    ↓  spawn "note-it"
single-instance dispatcher
    ↓
the instance already running
    ↓
notes restored and made visible
```

## Recommended Keybindings (`~/.config/niri/config.kdl`)

Add the following to your Niri configuration:

```kdl
// Spawn background daemon on compositor startup
spawn-at-startup "note-it" "--background"

binds {
    // Summon Note-it from any application: restores the notes and brings
    // them to the front. This is the binding to reach for.
    Mod+Shift+N { spawn "note-it"; }

    // Switch between "always on top" and "on the desktop"
    Mod+Shift+D { spawn "note-it" "toggle"; }

    // Collapse every note to its bar, or expand them all again
    Mod+Shift+M { spawn "note-it" "toggle-collapse-all"; }

    // Quick create new note
    Mod+Alt+N { spawn "note-it" "new"; }
}
```

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

`Ctrl+Shift+Space` is a key event inside the note's own WebView, so it only
reaches the note that holds keyboard focus. A layer surface asks for
`on-demand` keyboard interactivity, which means the compositor grants focus
when the surface is clicked — and changing layer re-maps the surface, so the
note gives that focus up each time it moves.

Going **to** the desktop layer therefore works from the keyboard. Coming
**back** needs the note to be reachable: click it where it shows through, and
the shortcut works again. If another window covers it there is nothing to
click, and no key can reach a surface the compositor is not sending keys to.

That is a property of the layer, not a bug to fix in the note: a `bottom`
surface is behind everything by definition. The way back that always works is
the compositor keybinding, which does not depend on focus at all:

```kdl
Mod+Shift+D { spawn "note-it" "toggle"; }
```

Binding it is recommended for exactly this reason.

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
