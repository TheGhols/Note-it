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
