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

/// rapid-mlx resolution, re-evaluated on each call so a venv provisioned *after*
/// backend launch is picked up:
///   1. `${YT2O_VENV_DIR}/bin/rapid-mlx` (app-private venv, stable path)
///   2. `YT2O_MLX_BIN` (explicit override, used in dev)
///   3. system PATH / bundled
pub fn rapid_mlx() -> anyhow::Result<PathBuf> {
    if let Some(dir) = std::env::var_os("YT2O_VENV_DIR").map(PathBuf::from) {
        let p = dir.join("bin/rapid-mlx");
        if p.exists() {
            return Ok(p);
        }
    }
    if let Some(p) = std::env::var_os("YT2O_MLX_BIN").map(PathBuf::from) {
        if p.exists() {
            return Ok(p);
        }
    }
    resolve("rapid-mlx")
        .map_err(|_| anyhow!("rapid-mlx not found; it is provisioned on first run via uv"))
}
