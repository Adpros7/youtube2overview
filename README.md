# yt2overview

A macOS-native app that turns any YouTube URL into copiable, AI-ready text:
**full transcript · top comments · visual (frame) overview · AI text overview · chapters**,
all generated locally on Apple Silicon.

> Paste a link → get one clean Markdown dump (human-readable *and* an AI-optimized payload with an
> instruction preamble) you can drop into any chatbot — or read yourself.

## Design

| Layer | Tech |
|-------|------|
| **UI** | SwiftUI, macOS 26 native **Liquid Glass** + `NSVisualEffectView` mica background |
| **Backend** | **Rust** (`axum`) local HTTP server, SSE progress streaming — bundled in the `.app` |
| **Data** | bundled **yt-dlp** (transcript, comments, metadata, chapters) + **ffmpeg** (keyframes) |
| **Local AI** | **rapid-mlx** serving **Gemma 4** (multimodal) — text overview + vision frame overview |
| **Provisioning** | first-run auto-install of `rapid-mlx` into an isolated venv via bundled **uv** |
| **Ship** | ad-hoc signed `.app` → **DMG** |

Everything runs locally. No API keys. No cloud.

## Layout

```
backend/     Rust axum server (the brain)
app/         SwiftUI macOS app (SPM executable, wrapped into .app)
scripts/     build + packaging (ad-hoc sign, DMG)
resources/   bundled binaries staged at package time
```

## Build

```sh
scripts/build.sh      # build backend + app
scripts/package.sh    # produce yt2overview.app and dist/yt2overview.dmg
```

## Requirements (dev)

Apple Silicon · macOS 26+ · Xcode 26 / Swift 6.3 · Rust · `uv`. The packaged app provisions the
rest (rapid-mlx) on first run.
