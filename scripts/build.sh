#!/usr/bin/env bash
# Build the Rust backend and the SwiftUI app in release mode.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo "▸ Building Rust backend (release)…"
( cd "$ROOT/backend" && cargo build --release )

echo "▸ Building SwiftUI app (release)…"
( cd "$ROOT/app" && swift build -c release --jobs $(sysctl -n hw.ncpu))

echo "✓ Build complete."
echo "  backend: $ROOT/backend/target/release/yt2overview-backend"
# echo "  app:     $(cd "$ROOT/app" && swift build -c release --show-bin-path)/yt2overview"
