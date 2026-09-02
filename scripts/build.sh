#!/usr/bin/env bash
#
# A release build of the whole project, from any directory.
#
# Builds and nothing else: it does not install anything, does not put binaries on
# the PATH, does not copy into `~/.local/bin`, does not run a system package
# manager or `sudo`, and does not touch a Note-it store. The project's own
# artefacts go where the repository already ignores them — `ui/node_modules`,
# `ui/dist`, `target`.
#
# That is not a claim that nothing is written anywhere else: Cargo and pnpm use
# their normal development infrastructure — the Cargo registry and git caches,
# the pnpm store — exactly as they do for any other project. That is expected,
# and it is not an installation of Note-it.
#
# It says "pronto" only after checking that both binaries are actually there
# and actually executable. A build script that announces success without
# looking is how a missing binary is discovered by the person who tried to run
# it.
#
# Usage: scripts/build.sh
#
set -euo pipefail

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
readonly REPO_ROOT
cd "$REPO_ROOT"

fail() {
  printf 'scripts/build.sh: %s\n' "$*" >&2
  exit 1
}

for required in pnpm cargo; do
  command -v "$required" >/dev/null 2>&1 || fail "comando obrigatório ausente: $required"
done

printf '==> Frontend do Note-it\n'
(
  cd ui
  # `--frozen-lockfile` is the point: a build that is allowed to resolve a
  # different dependency tree than the lockfile records is not reproducible,
  # and it is the tree the CI never saw. There is deliberately no fallback to
  # npm — a different package manager would resolve a different tree for the
  # same reason.
  pnpm install --frozen-lockfile
  pnpm run build
)

printf '\n==> Binários Rust do Note-it (release, workspace inteiro)\n'
cargo build --release --workspace

printf '\n==> Conferindo os binários\n'
readonly BINARIES=(
  target/release/note-it
  target/release/noteit
)
for binary in "${BINARIES[@]}"; do
  [[ -f "$binary" ]] || fail "esperado mas não encontrado: $binary"
  [[ -x "$binary" ]] || fail "encontrado mas não executável: $binary"
  printf '  [ok] %s\n' "$binary"
done

printf '\nBuild pronto. Os binários ficam em target/release/ e não foram instalados.\n'
