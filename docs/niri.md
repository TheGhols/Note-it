# Niri Compositor Integration

Note-it is designed and tested for the [Niri](https://github.com/YaLTeR/niri) scrollable-tiling Wayland compositor.

## Layer Shell Setup

Note-it registers windows with the Wayland Layer Shell namespace `note-it`.

- **Desktop Layer:** Notes attach to layer `bottom` with `exclusive_zone = 0`, keeping them on the background behind active Niri tiles.
- **Overlay Layer:** Notes switch to layer `overlay`, appearing over active workspaces for immediate editing.

## Recommended Keybindings (`~/.config/niri/config.kdl`)

Add the following to your Niri configuration:

```kdl
// Spawn background daemon on compositor startup
spawn-at-startup "note-it" "--background"

binds {
    // Toggle notes overlay
    Mod+Shift+N { spawn "note-it" "toggle"; }

    // Quick create new note
    Mod+Alt+N { spawn "note-it" "new"; }
}
```
