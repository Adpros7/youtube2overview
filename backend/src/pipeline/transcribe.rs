//! Local-file transcription: embedded subtitle track if present, else mlx-whisper ASR.

use std::path::Path;

use anyhow::{anyhow, Context};
use serde_json::Value;
use tokio::process::Command;

use crate::config::Settings;
use crate::model::Cue;
use crate::pipeline::ytdlp;
use crate::tools;

/// Transcribe a local file. Returns (cues, language). Empty cues are not an error —
/// the rest of the pipeline (visual overview, metadata) still runs.
pub async fn local(
    file: &Path,
    settings: &Settings,
    work_dir: &Path,
) -> anyhow::Result<(Vec<Cue>, String)> {
    // 1. Reuse an embedded subtitle track if the container has one (instant, no model).
    if let Some(cues) = embedded_subs(file, work_dir).await {
        if !cues.is_empty() {
            return Ok((cues, String::new()));
        }
    }
    // 2. Fall back to on-device ASR.
    whisper(file, settings, work_dir).await
}

/// Try to extract the first embedded subtitle stream as WebVTT (only if one exists).
async fn embedded_subs(file: &Path, work_dir: &Path) -> Option<Vec<Cue>> {
    if !has_subtitle_stream(file).await {
        return None;
    }
    let ffmpeg = tools::ffmpeg().ok()?;
    let out = work_dir.join("embedded.vtt");
    let result = Command::new(&ffmpeg)
        .arg("-nostdin")
        .arg("-loglevel")
        .arg("error")
        .arg("-i")
        .arg(file)
        .arg("-map")
        .arg("0:s:0?")
        .arg("-f")
        .arg("webvtt")
        .arg("-y")
        .arg(&out)
        .output()
        .await
        .ok()?;
    if !result.status.success() {
        return None;
    }
    let raw = std::fs::read_to_string(&out).ok()?;
    Some(ytdlp::parse_vtt(&raw))
}

/// Does the file have at least one subtitle stream? (Quiet ffprobe check.)
async fn has_subtitle_stream(file: &Path) -> bool {
    let Ok(ffprobe) = tools::ffprobe() else {
        return false;
    };
    let out = Command::new(&ffprobe)
        .arg("-v")
        .arg("quiet")
        .arg("-select_streams")
        .arg("s")
        .arg("-show_entries")
        .arg("stream=index")
        .arg("-of")
        .arg("csv=p=0")
        .arg(file)
        .output()
        .await;
    matches!(out, Ok(o) if o.status.success() && !o.stdout.is_empty())
}

/// Transcribe via the mlx-whisper CLI. The model downloads from HuggingFace on first use.
async fn whisper(
    file: &Path,
    settings: &Settings,
    work_dir: &Path,
) -> anyhow::Result<(Vec<Cue>, String)> {
    let bin = tools::mlx_whisper()?;

    // mlx-whisper's audio loader shells out to `ffmpeg`; make sure our bundled copy is
    // on PATH for the child process.
    let mut path = std::env::var("PATH").unwrap_or_default();
    if let Some(dir) = std::env::var_os("YT2O_BIN_DIR") {
        path = format!("{}:{}", dir.to_string_lossy(), path);
    } else if let Ok(ff) = tools::ffmpeg() {
        if let Some(parent) = ff.parent() {
            path = format!("{}:{}", parent.display(), path);
        }
    }

    let out = Command::new(&bin)
        .arg(file)
        .arg("--model")
        .arg(&settings.whisper_model)
        .arg("--task")
        .arg("transcribe")
        .arg("--output-format")
        .arg("json")
        .arg("--output-dir")
        .arg(work_dir)
        .env("PATH", path)
        .output()
        .await
        .context("failed to launch mlx_whisper")?;
    if !out.status.success() {
        return Err(anyhow!(
            "mlx_whisper failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }

    // mlx-whisper writes `<stem>.json` into the output dir; fall back to scanning.
    let stem = file.file_stem().and_then(|s| s.to_str()).unwrap_or("audio");
    let mut json_path = work_dir.join(format!("{stem}.json"));
    if !json_path.exists() {
        json_path = newest_json(work_dir).ok_or_else(|| anyhow!("whisper produced no output"))?;
    }
    let raw = std::fs::read_to_string(&json_path).context("whisper output missing")?;
    let v: Value = serde_json::from_str(&raw).context("whisper JSON parse")?;

    let lang = v
        .get("language")
        .and_then(|l| l.as_str())
        .unwrap_or_default()
        .to_string();
    let mut cues = Vec::new();
    if let Some(segs) = v.get("segments").and_then(|s| s.as_array()) {
        for seg in segs {
            let start = seg.get("start").and_then(|x| x.as_f64()).unwrap_or(0.0);
            let text = seg
                .get("text")
                .and_then(|x| x.as_str())
                .unwrap_or_default()
                .trim()
                .to_string();
            if !text.is_empty() {
                cues.push(Cue { start, text });
            }
        }
    }
    Ok((cues, lang))
}

fn newest_json(dir: &Path) -> Option<std::path::PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .max_by_key(|p| {
            std::fs::metadata(p)
                .and_then(|m| m.modified())
                .ok()
        })
}
