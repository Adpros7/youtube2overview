# yt2overview

A macOS-native app that turns any YouTube URL into copiable, AI-ready text:
**full transcript · top comments · visual (frame) overview · AI text overview · chapters**,
all generated locally on Apple Silicon.

> Paste a link → get one clean document — a **Readable** view *and* an **AI-optimized** payload
> (with an instruction preamble) you can drop into any chatbot. Copy the whole thing or any single
> section.

## How it works

```
┌─────────────────────────────┐     spawns      ┌──────────────────────────────┐
│  SwiftUI app (Liquid Glass)  │ ───────────────▶│   Rust backend (axum, SSE)   │
│  • URL input, live progress  │  localhost HTTP │   the orchestrator           │
│  • Readable / AI-optimized   │◀─────────────── │                              │
│  • per-section copy          │                 └───────────┬──────────────────┘
│  • granular settings         │                             │
└─────────────────────────────┘            ┌────────────────┼────────────────┐
                                            ▼                ▼                ▼
                                      yt-dlp            ffmpeg          rapid-mlx
                                  (transcript,       (keyframes      (serves Gemma 4,
                                   comments,          via stream-     OpenAI-compatible;
                                   chapters,          seek, no         text + vision)
                                   metadata)          full download)
```

| Layer | Tech |
|-------|------|
| **UI** | SwiftUI, macOS 26 native **Liquid Glass** (`.glassEffect`) + `NSVisualEffectView` mica |
| **Backend** | **Rust** (`axum`) localhost server, SSE progress — bundled in the `.app` |
| **Data** | **yt-dlp** (transcript, comments, metadata, chapters) + **ffmpeg** (keyframes) |
| **Local AI** | **rapid-mlx** serving **Gemma 4** (multimodal) — text overview + vision frame overview |
| **Provisioning** | first-run auto-install of `rapid-mlx[vision]` into a private venv via **uv** |
| **Ship** | ad-hoc signed `.app` → **DMG** |

Everything runs locally. No API keys. No cloud.

## Use it

1. Open the app (or mount `dist/yt2overview.dmg` and drag to Applications).
2. On first launch it provisions the model runtime (`rapid-mlx[vision]`) into
   `~/Library/Application Support/yt2overview/venv` — one-time, needs network.
3. Paste a YouTube URL, hit **Generate**. The backend:
   - pulls metadata, chapters, top-N comments and the transcript (yt-dlp),
   - extracts keyframes by seeking into a low-res stream (ffmpeg — no full download),
   - reuses a running/cached **Gemma 4** server or serves one on a free port (with `--mllm`
     for vision), and asks it for a text overview + a visual overview,
   - assembles the **Readable** and **AI-optimized** outputs.
4. Toggle **Readable / AI-optimized**, **Copy all**, or copy any single section.

Open **Settings** (⚙️ / ⌘,) for granular control: model + quant, temperature, max tokens, server
port, comment count/sort, frame count/strategy, overview length/style/language, transcript
timestamps, and which sections to include.

**History** (🕘 / ⌘Y): every generation is saved to
`~/Library/Application Support/yt2overview/history.json`; reopen any past result from the History
panel or the **History** menu.

**Menu bar & shortcuts:** full native menus — *File* (Generate ⌘↵, Paste URL & Generate ⌘⇧V,
Clear ⌘⌫), *Overview* (Copy All ⌘⇧C, Copy AI payload ⌘⌥C, Readable ⌘1 / AI-optimized ⌘2),
*History* (⌘Y + recents), plus About and Settings.

## Build from source

```sh
scripts/build.sh      # release builds of backend + app
scripts/package.sh    # → build/yt2overview.app and dist/yt2overview.dmg (ad-hoc signed)
```

### Dev run (skip provisioning)

Point the app at a prebuilt vision venv and the freshly built backend:

```sh
# one-time: a vision-capable rapid-mlx venv
uv venv /tmp/yt2o-venv --python 3.12 --seed
uv pip install --python /tmp/yt2o-venv/bin/python 'rapid-mlx[vision]'

cd app
YT2O_MLX_BIN=/tmp/yt2o-venv/bin/rapid-mlx \
YT2O_BACKEND_BIN=../backend/target/debug/yt2overview-backend \
swift run
```

## Layout

```
backend/     Rust axum server (the brain)
app/         SwiftUI macOS app (SPM executable, wrapped into .app)
scripts/     build + packaging (icon, ad-hoc sign, DMG)
build/, dist/  packaging outputs (git-ignored)
```

## Requirements

Apple Silicon · macOS 26+ · (to build) Xcode 26 / Swift 6.3 · Rust · `uv`.

### Notes

- **Models:** defaults to an already-cached multimodal **Gemma 4** if present (e.g.
  `mlx-community/gemma-4-12b-it-4bit`), otherwise pick one in Settings. Vision models are served
  with rapid-mlx `--mllm`; Gemma 4 "thinking" is disabled per-request so the answer isn't truncated.
- **ffmpeg portability:** the bundled `ffmpeg`/`ffprobe` are copied from the build machine and may
  depend on Homebrew libraries; on a clean Mac the backend falls back to any `ffmpeg` on `PATH`. For
  a fully portable DMG, drop static `ffmpeg`/`ffprobe` builds into `Resources/bin` at package time.
- **Gatekeeper:** the DMG is ad-hoc signed (no Apple Developer ID). First open may need
  right-click → **Open**.
