#!/usr/bin/env bash
set -euo pipefail

echo "==> Building Note-it Frontend..."
cd ui
if command -v pnpm &> /dev/null; then
  pnpm install
  pnpm build
else
  npm install
  npm run build
fi
cd ..

echo "==> Building Note-it Rust Native Binary..."
cargo build --release

echo "==> Build complete! Output: target/release/note-it"
