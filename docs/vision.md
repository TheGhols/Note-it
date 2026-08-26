# Note-it — Product Vision & Principles

## Vision

Note-it is a minimalist, distraction-free desktop post-it note application crafted for Linux Wayland environments.

It does not aim to replace comprehensive knowledge bases like Obsidian or Notion. Instead, it fulfills a single, focused need: **effortlessly creating quick notes that reside naturally on the desktop workspace and can be summoned instantly above active windows when needed.**

## Core Principles

- **Local-First & Offline-First:** All user data remains entirely on the local filesystem.
- **No Cloud, No Accounts:** No registration, login, sync servers, or external services required.
- **Privacy by Design:** Zero analytics, telemetry, crash reporting pings, or background network calls.
- **Wayland Native:** Designed for modern Wayland compositors using the `wlr-layer-shell` protocol, with first-class support for Niri.
- **Standard Storage:** Every note is a standard, portable Markdown (`.md`) file on disk. No proprietary databases for note text.
- **True WYSIWYG:** What you see is formatted text. Markdown syntax markers never clutter the editing flow.
- **Keyboard-Centric:** Instant note creation (`Ctrl+N`), quick dismiss (`Ctrl+W`), and intuitive formatting controls.
- **High Performance:** Minimal resource footprint, near-zero idle CPU usage, and fast startup.
