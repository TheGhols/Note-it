# Contributing to Note-it

Thank you for your interest in contributing to Note-it!

## Guiding Principles

1. **Local-First & Private:** Never introduce telemetry, cloud sync lock-in, tracking, or network dependencies into the core note workflow.
2. **Wayland & Performance First:** Maintain high responsiveness, low idle CPU/RAM usage, and correct Layer Shell protocol usage.
3. **True WYSIWYG:** Preserve clean Markdown on disk while presenting formatted text in the editor without raw syntax markers interfering with editing.
4. **Clean Code & Commits:** Keep pull requests focused, write comprehensive tests, and use conventional commit messages.

## Development Workflow

1. Ensure all system dependencies are installed:
   - `gtk4`, `gtk4-layer-shell`, `webkitgtk-6.0`
   - Rust toolchain
   - Node.js & pnpm
2. Run formatters and linters before submitting changes:
   ```bash
   cargo fmt --check
   cargo clippy -- -D warnings
   cargo test
   cd ui && pnpm lint && pnpm test && pnpm build
   ```
3. Never include developer-specific paths, credentials, or personal configuration files in commits.

## Commit Guidelines

Use professional and clear commit messages following Conventional Commits:

- `feat: add markdown note storage`
- `fix: prevent focus loss on overlay transition`
- `test: add markdown round-trip coverage`
- `docs: update niri keybinding examples`
- `chore: update dependencies`
