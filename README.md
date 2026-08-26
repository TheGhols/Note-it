# Note-it

A minimalist sticky note (post-it) application for Linux Wayland.

> **Status:** Experimental. Under active development.
> Niri is the primary supported compositor in the initial development phase.

---

## Overview

Note-it is a lightweight, local-first, distraction-free desktop note application built natively for Wayland using the `wlr-layer-shell` protocol. It allows you to quickly capture thoughts and keep them pinned to your desktop workspace or bring them into an overlay above all active windows with a single shortcut.

Each note is persisted as a standard Markdown (`.md`) file on disk, combining clean human-readable files with a true WYSIWYG editing experience.

## Key Features

- **Wayland Native:** Built with GTK4, `gtk4-layer-shell`, and WebKitGTK 6.0.
- **Desktop & Overlay Modes:** Notes live on the desktop layer (`bottom`) without obstructing applications, and can toggle instantly into an `overlay` layer.
- **Local-First & Portable:** Every post-it is an individual `.md` file with YAML front matter stored in `$XDG_DATA_HOME/note-it/notes/`.
- **True WYSIWYG:** Edit formatted text without Markdown syntax markers cluttering the cursor.
- **Atomic Autosave:** Changes save safely with debounced atomic disk writes.
- **Keyboard-Centric:** Instant note creation (`Ctrl+N`), quick dismiss (`Ctrl+W`), and text formatting shortcuts.
- **Single-Instance IPC:** Seamless command-line integration for window management and global shortcuts.
- **Privacy by Design:** Zero telemetry, zero analytics, zero external network requests, zero accounts.

## System Requirements

- **Operating System:** Linux with Wayland compositor supporting `wlr-layer-shell` (tested and optimized on Arch Linux with Niri).
- **System Dependencies:**
  - `gtk4`
  - `gtk4-layer-shell`
  - `webkitgtk-6.0`
  - `glib2`
  - `pkgconf`
- **Build Toolchain:**
  - Rust (stable) & Cargo
  - Node.js (>= 20) & npm / pnpm

## Executando o Note-it localmente

No Arch Linux com Niri, instale os pré-requisitos uma vez:

```bash
sudo pacman -S --needed gtk4 gtk4-layer-shell webkitgtk-6.0 rust nodejs pnpm pkgconf base-devel
```

Depois, a partir da raiz do repositório, inicie normalmente com um único comando:

```bash
./scripts/run-note-it
```

O runner prepara o frontend quando necessário, executa o build incremental do host e inicia a instância única do aplicativo. Para iniciar apenas o daemon, sem criar WebViews ou superfícies:

```bash
./scripts/run-note-it --background
```

Use o mesmo comando para controlar a instância em execução:

```bash
./scripts/run-note-it new      # cria uma nota (Ctrl+N também funciona no editor)
./scripts/run-note-it show     # mostra as notas em modo Overlay
./scripts/run-note-it hide     # esconde todas as notas após salvar
./scripts/run-note-it toggle   # alterna entre Desktop e Overlay
./scripts/run-note-it quit     # salva e encerra o aplicativo
```

## Niri Compositor Integration

Add the following to your Niri configuration (`~/.config/niri/config.kdl`):

```kdl
// Spawn Note-it daemon on startup
spawn-at-startup "note-it" "--background"

// Global shortcut to toggle overlay mode
binds {
    Mod+Shift+N { spawn "note-it" "toggle"; }
}
```

## Documentation

Detailed technical documentation is available in the [`docs/`](docs/) directory:

- [Vision & Principles](docs/vision.md)
- [Architecture](docs/architecture.md)
- [Markdown Storage Format](docs/markdown-format.md)
- [Storage & XDG Paths](docs/storage.md)
- [Niri Integration](docs/niri.md)
- [Security & HTML Sanitization](docs/security.md)
- [Development Guide](docs/development.md)
- [Architectural Decisions](docs/decisions.md)
- [Roadmap](docs/roadmap.md)

## License

This project is licensed under the [MIT License](LICENSE).
