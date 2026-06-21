#!/usr/bin/env bash
# Build release artifacts for the current commit:
#   - dist/yt2overview.dmg
#   - dist/yt2overview-source.tar.gz
#   - SHA-stamped copies of both artifacts
# Optionally uploads the SHA-stamped artifacts to GitHub Releases with --github.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="$ROOT/dist"
APP_NAME="yt2overview"
UPLOAD_GITHUB=0

for arg in "$@"; do
    case "$arg" in
        --github) UPLOAD_GITHUB=1 ;;
        *)
            echo "usage: scripts/release.sh [--github]" >&2
            exit 2
            ;;
    esac
done

if ! git -C "$ROOT" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    echo "release: $ROOT is not a git work tree" >&2
    exit 1
fi

SHA="$(git -C "$ROOT" rev-parse --short=12 HEAD)"
FULL_SHA="$(git -C "$ROOT" rev-parse HEAD)"
TAG="release-${SHA}"

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

if [[ "$UPLOAD_GITHUB" == "1" ]]; then
    if [[ "${YT2O_SKIP_GITHUB_RELEASE:-}" == "1" ]]; then
        echo "GitHub Release upload skipped because YT2O_SKIP_GITHUB_RELEASE=1."
        exit 0
    fi
    if ! command -v gh >/dev/null 2>&1; then
        echo "release: gh CLI is required for --github" >&2
        exit 1
    fi

    echo "> Publishing GitHub Release ${TAG}..."
    if ! git -C "$ROOT" rev-parse -q --verify "refs/tags/${TAG}" >/dev/null; then
        git -C "$ROOT" tag "$TAG" "$FULL_SHA"
    fi
    git -C "$ROOT" push origin "refs/tags/${TAG}"

    if gh release view "$TAG" >/dev/null 2>&1; then
        gh release upload "$TAG" "$DMG_SHA" "$SOURCE_TAR_SHA" --clobber
    else
        gh release create "$TAG" \
            "$DMG_SHA" \
            "$SOURCE_TAR_SHA" \
            --title "yt2overview ${SHA}" \
            --notes "Automated commit release for ${FULL_SHA}."
    fi
    echo "GitHub Release ready: ${TAG}"
fi
