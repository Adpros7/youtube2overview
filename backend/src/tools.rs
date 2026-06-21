//! Resolution of external tool binaries (yt-dlp, ffmpeg, uv, rapid-mlx).
//!
//! Resolution order:
//!   1. `YT2O_BIN_DIR` env var (set by the .app to its bundled `Resources/bin`)
//!   2. the system `PATH`
//! so the same backend works both in dev (Homebrew tools on PATH) and when bundled.

use std::path::PathBuf;

use anyhow::{anyhow, Context};

fn bundled(name: &str) -> Option<PathBuf> {
    let dir = std::env::var_os("YT2O_BIN_DIR")?;
    let p = PathBuf::from(dir).join(name);
    p.exists().then_some(p)
}

/// Resolve a tool by name, preferring the bundled copy.
pub fn resolve(name: &str) -> anyhow::Result<PathBuf> {
    if let Some(p) = bundled(name) {
        return Ok(p);
    }
    which::which(name).with_context(|| format!("`{name}` not found (bundled or on PATH)"))
}

pub fn yt_dlp() -> anyhow::Result<PathBuf> {
    resolve("yt-dlp")
}

pub fn ffmpeg() -> anyhow::Result<PathBuf> {
    resolve("ffmpeg")
}

pub fn uv() -> anyhow::Result<PathBuf> {
    resolve("uv")
}

/// rapid-mlx may live in a provisioned venv (`YT2O_MLX_BIN`) before it's on PATH.
pub fn rapid_mlx() -> anyhow::Result<PathBuf> {
    if let Some(p) = std::env::var_os("YT2O_MLX_BIN").map(PathBuf::from) {
        if p.exists() {
            return Ok(p);
        }
    }
    resolve("rapid-mlx").map_err(|_| {
        anyhow!("rapid-mlx not found; it is provisioned on first run via uv")
    })
}
