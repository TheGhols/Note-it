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
