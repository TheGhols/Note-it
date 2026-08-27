# Development Guide

## Build Prerequisites

Ensure all required system packages are installed on your Linux distribution:

```bash
# Arch Linux
sudo pacman -S --needed gtk4 gtk4-layer-shell webkitgtk-6.0 rust nodejs pnpm pkgconf base-devel
```

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
scripts/note-it-isolated              # fresh temporary tree, removed on exit
scripts/note-it-isolated --keep       # keep the tree for inspection
scripts/note-it-isolated -- new       # pass arguments through to note-it
```

The helper sets **all four** XDG variables together — `XDG_CONFIG_HOME`, `XDG_DATA_HOME`,
`XDG_STATE_HOME` and `XDG_CACHE_HOME` — and prints them before launching. Overriding only some of
them leaves the rest resolving to the real store, which is how a stray note once ended up in it.

Before creating anything it resolves the real XDG base directories using the same rules the
application uses, and aborts with exit code 90 if any configured directory is, or sits inside, one
of them, or is the home directory. Nothing personal is hard-coded.

A cold `XDG_CACHE_HOME` makes GTK rebuild its compose table, which produces the one-off
`Can't handle >16bit keyvals` warning burst described in ADR-006. It is expected on the first run
against a fresh tree and disappears on the next one.

