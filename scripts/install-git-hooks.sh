#!/usr/bin/env bash
# Install this repo's versioned git hooks.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

git -C "$ROOT" config core.hooksPath .githooks
chmod +x "$ROOT/.githooks/post-commit" "$ROOT/scripts/release.sh"

echo "Git hooks installed."
echo "  post-commit will build local artifacts and upload them to GitHub Releases."
echo "  To skip once: YT2O_SKIP_RELEASE_HOOK=1 git commit ..."
