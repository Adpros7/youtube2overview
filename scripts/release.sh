#!/usr/bin/env bash
# Build local release artifacts for the current commit:
#   - dist/yt2overview.dmg
#   - dist/yt2overview-source.tar.gz
#   - SHA-stamped copies of both artifacts
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="$ROOT/dist"
APP_NAME="yt2overview"

if ! git -C "$ROOT" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    echo "release: $ROOT is not a git work tree" >&2
    exit 1
fi

SHA="$(git -C "$ROOT" rev-parse --short=12 HEAD)"

echo "> Building release DMG for ${SHA}..."
"$ROOT/scripts/package.sh"

mkdir -p "$DIST_DIR"

SOURCE_TAR="$DIST_DIR/${APP_NAME}-source.tar.gz"
SOURCE_TAR_SHA="$DIST_DIR/${APP_NAME}-source-${SHA}.tar.gz"
DMG="$DIST_DIR/${APP_NAME}.dmg"
DMG_SHA="$DIST_DIR/${APP_NAME}-${SHA}.dmg"

echo "> Building source archive..."
rm -f "$SOURCE_TAR" "$SOURCE_TAR_SHA"
git -C "$ROOT" archive \
    --format=tar.gz \
    --prefix="${APP_NAME}-${SHA}/" \
    -o "$SOURCE_TAR" \
    HEAD
cp "$SOURCE_TAR" "$SOURCE_TAR_SHA"

if [[ -f "$DMG" ]]; then
    cp "$DMG" "$DMG_SHA"
else
    echo "release: expected DMG missing at $DMG" >&2
    exit 1
fi

echo "Release artifacts ready."
echo "  dmg:    $DMG"
echo "  dmg:    $DMG_SHA"
echo "  source: $SOURCE_TAR"
echo "  source: $SOURCE_TAR_SHA"
