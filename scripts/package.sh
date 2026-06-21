#!/usr/bin/env bash
# Assemble yt2overview.app, ad-hoc sign it, and produce dist/yt2overview.dmg.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_NAME="yt2overview"
BUILD_DIR="$ROOT/build"
DIST_DIR="$ROOT/dist"
APP="$BUILD_DIR/$APP_NAME.app"

"$ROOT/scripts/build.sh"

APP_BIN="$(cd "$ROOT/app" && swift build -c release --show-bin-path)/yt2overview"
BACKEND_BIN="$ROOT/backend/target/release/yt2overview-backend"

echo "▸ Assembling app bundle…"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources/bin"

cp "$APP_BIN" "$APP/Contents/MacOS/$APP_NAME"
cp "$BACKEND_BIN" "$APP/Contents/Resources/bin/yt2overview-backend"
cp "$ROOT/scripts/Info.plist" "$APP/Contents/Info.plist"
printf 'APPL????' > "$APP/Contents/PkgInfo"

# ---- App icon ----
echo "▸ Generating app icon…"
ICON_TMP="$(mktemp -d)"
if swift "$ROOT/scripts/make_icon.swift" "$ICON_TMP" >/dev/null 2>&1; then
    iconutil -c icns "$ICON_TMP/AppIcon.iconset" -o "$APP/Contents/Resources/AppIcon.icns" || \
        echo "  (icon conversion skipped)"
fi
rm -rf "$ICON_TMP"

# ---- Bundle helper tools (best effort; backend falls back to PATH) ----
bundle_tool() {
    local name="$1"; local src
    src="$(command -v "$name" || true)"
    if [[ -n "$src" ]]; then
        cp -L "$src" "$APP/Contents/Resources/bin/$name" 2>/dev/null && \
            echo "  bundled $name" || echo "  could not bundle $name (will use PATH)"
    else
        echo "  $name not found (will use PATH)"
    fi
}

echo "▸ Bundling helper tools…"
# uv is a self-contained static binary — fully portable.
bundle_tool uv
# Prefer the self-contained yt-dlp_macos build for portability.
if curl -fsSL "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos" \
        -o "$APP/Contents/Resources/bin/yt-dlp" 2>/dev/null; then
    chmod +x "$APP/Contents/Resources/bin/yt-dlp"; echo "  bundled yt-dlp (standalone)"
else
    bundle_tool yt-dlp
fi
# ffmpeg/ffprobe: copied best-effort (may rely on system libs on other machines).
bundle_tool ffmpeg
bundle_tool ffprobe

# ---- Ad-hoc code signing ----
echo "▸ Ad-hoc signing…"
for f in "$APP/Contents/Resources/bin/"*; do
    codesign --force -s - --timestamp=none "$f" 2>/dev/null || true
done
codesign --force --deep -s - --timestamp=none "$APP" 2>/dev/null || true
codesign --verify --deep --strict "$APP" 2>/dev/null && echo "  signature verified" || \
    echo "  (ad-hoc signature, verification informational)"

# ---- DMG ----
echo "▸ Building DMG…"
mkdir -p "$DIST_DIR"
DMG="$DIST_DIR/$APP_NAME.dmg"
rm -f "$DMG"
STAGE="$(mktemp -d)"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname "$APP_NAME" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null
rm -rf "$STAGE"

echo "✓ Done."
echo "  app: $APP"
echo "  dmg: $DMG"
