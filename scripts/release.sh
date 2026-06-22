#!/usr/bin/env bash
# Build release artifacts for the current commit:
#   - dist/yt2overview.dmg
#   - dist/yt2overview-source.tar.gz
#   - semver-stamped copies of both artifacts
# Optionally uploads the semver-stamped artifacts to GitHub Releases with --github.
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
COMMIT_MSG="$(git -C "$ROOT" log -1 --pretty=%B HEAD)"

semver_pattern='^v?[0-9]+\.[0-9]+\.[0-9]+$'

manifest_version() {
    awk -F '"' '/^version = / { print $2; exit }' "$ROOT/backend/Cargo.toml"
}

normalize_semver_tag() {
    local version="$1"
    if [[ ! "$version" =~ $semver_pattern ]]; then
        echo "release: invalid semantic version '$version' (expected vMAJOR.MINOR.PATCH)" >&2
        exit 2
    fi
    if [[ "$version" == v* ]]; then
        echo "$version"
    else
        echo "v${version}"
    fi
}

bump_patch_tag() {
    local tag="${1#v}"
    local major minor patch
    IFS=. read -r major minor patch <<< "$tag"
    echo "v${major}.${minor}.$((patch + 1))"
}

resolve_release_tag() {
    local head_tag latest base
    head_tag="$(git -C "$ROOT" tag --points-at HEAD --list 'v[0-9]*.[0-9]*.[0-9]*' --sort=-v:refname | head -n 1)"
    if [[ -n "$head_tag" ]]; then
        echo "$head_tag"
        return
    fi

    if [[ -n "${YT2O_RELEASE_VERSION:-}" ]]; then
        normalize_semver_tag "$YT2O_RELEASE_VERSION"
        return
    fi

    latest="$(git -C "$ROOT" tag --list 'v[0-9]*.[0-9]*.[0-9]*' --sort=-v:refname | head -n 1)"
    if [[ -n "$latest" ]]; then
        bump_patch_tag "$latest"
        return
    fi

    base="$(manifest_version)"
    normalize_semver_tag "${base:-0.1.0}"
}

if [[ "$UPLOAD_GITHUB" == "1" ]]; then
    git -C "$ROOT" fetch --tags origin >/dev/null 2>&1 || true
fi

TAG="$(resolve_release_tag)"
VERSION="${TAG#v}"

echo "> Building release DMG for ${TAG} (${SHA})..."
"$ROOT/scripts/package.sh"

mkdir -p "$DIST_DIR"

SOURCE_TAR="$DIST_DIR/${APP_NAME}-source.tar.gz"
SOURCE_TAR_TAG="$DIST_DIR/${APP_NAME}-source-${TAG}.tar.gz"
DMG="$DIST_DIR/${APP_NAME}.dmg"
DMG_TAG="$DIST_DIR/${APP_NAME}-${TAG}.dmg"

echo "> Building source archive..."
rm -f "$SOURCE_TAR" "$SOURCE_TAR_TAG"
git -C "$ROOT" archive \
    --format=tar.gz \
    --prefix="${APP_NAME}-${VERSION}/" \
    -o "$SOURCE_TAR" \
    HEAD
cp "$SOURCE_TAR" "$SOURCE_TAR_TAG"

if [[ -f "$DMG" ]]; then
    cp "$DMG" "$DMG_TAG"
else
    echo "release: expected DMG missing at $DMG" >&2
    exit 1
fi

echo "Release artifacts ready."
echo "  dmg:    $DMG"
echo "  dmg:    $DMG_TAG"
echo "  source: $SOURCE_TAR"
echo "  source: $SOURCE_TAR_TAG"

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
        gh release upload "$TAG" "$DMG_TAG" "$SOURCE_TAR_TAG" --clobber
    else
        gh release create "$TAG" \
            "$DMG_TAG" \
            "$SOURCE_TAR_TAG" \
            --title "yt2overview ${TAG}" \
            --notes "${COMMIT_MSG}"$'\n\n'"Automated commit release for ${FULL_SHA}."
    fi
    echo "GitHub Release ready: ${TAG}"
fi
