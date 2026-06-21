//! Keyframe extraction via ffmpeg. For YouTube we seek directly into a low-res stream
//! URL (no full-video download); for local files we seek the file in place.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Context};
use tokio::process::Command;
use tokio::sync::Semaphore;

use crate::config::{FrameStrategy, Settings};
use crate::model::{Chapter, Frame, Source};
use crate::tools;

/// Bound on concurrent ffmpeg seeks (each opens its own connection for remote streams).
const MAX_CONCURRENT_SEEKS: usize = 4;

/// Resolve the ffmpeg input for a source: a local file path, or a remote stream URL.
/// For YouTube, prefer a URL already resolved from the metadata dump to avoid a second
/// (expensive) yt-dlp extraction; fall back to `yt-dlp -g` only if none was provided.
async fn ffmpeg_input(source: &Source, prefetched: Option<&str>) -> anyhow::Result<String> {
    match source {
        Source::Local(path) => Ok(path.to_string_lossy().to_string()),
        Source::YouTube(url) => {
            if let Some(u) = prefetched.filter(|u| !u.trim().is_empty()) {
                return Ok(u.to_string());
            }
            stream_url(url).await
        }
    }
}

/// Resolve a low-res, seekable stream URL via `yt-dlp -g` (fallback path).
async fn stream_url(url: &str) -> anyhow::Result<String> {
    let yt = tools::yt_dlp()?;
    let out = Command::new(&yt)
        .arg("-f")
        .arg("18/worst[ext=mp4]/worst[vcodec!=none]/worst")
        .arg("-g")
        .arg("--no-warnings")
        .arg("--no-playlist")
        .arg(url)
        .output()
        .await
        .context("yt-dlp -g failed to launch")?;
    if !out.status.success() {
        return Err(anyhow!(
            "yt-dlp could not resolve a stream url: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let s = String::from_utf8_lossy(&out.stdout);
    s.lines()
        .next()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.trim().to_string())
        .ok_or_else(|| anyhow!("no stream url returned"))
}

fn chapter_of(chapters: &[Chapter], t: f64) -> Option<String> {
    chapters
        .iter()
        .find(|c| t >= c.start && (c.end <= 0.0 || t < c.end))
        .map(|c| c.title.clone())
}

/// Compute the timestamps to sample given the strategy.
fn timestamps(settings: &Settings, chapters: &[Chapter], duration: f64) -> Vec<f64> {
    let n = settings.max_frames().max(1) as usize;
    let dur = if duration > 0.0 { duration } else { 0.0 };

    match settings.frame_strategy {
        FrameStrategy::Chapters if !chapters.is_empty() => {
            // One frame a little into each chapter; subsample evenly if too many.
            let picks: Vec<f64> = chapters
                .iter()
                .map(|c| {
                    let span = (c.end - c.start).max(0.0);
                    c.start + (span * 0.25).min(span)
                })
                .collect();
            subsample(picks, n)
        }
        _ => {
            // Even (also the fallback for Chapters w/o chapters and for SceneChange's planner).
            if dur <= 0.0 {
                return Vec::new();
            }
            (0..n)
                .map(|i| ((i as f64) + 0.5) / (n as f64) * dur)
                .collect()
        }
    }
}

fn subsample(items: Vec<f64>, n: usize) -> Vec<f64> {
    if items.len() <= n {
        return items;
    }
    let step = items.len() as f64 / n as f64;
    (0..n)
        .map(|i| items[((i as f64) * step) as usize])
        .collect()
}

/// Extract keyframes. Returns the frames in chronological order. `prefetched_url` is a
/// stream URL already resolved from the metadata dump (YouTube only), to avoid a second
/// yt-dlp call.
pub async fn extract(
    source: &Source,
    prefetched_url: Option<&str>,
    settings: &Settings,
    chapters: &[Chapter],
    duration: f64,
    work_dir: &Path,
) -> anyhow::Result<Vec<Frame>> {
    if settings.max_frames() == 0 {
        return Ok(Vec::new());
    }
    let ffmpeg = tools::ffmpeg()?;
    let surl = ffmpeg_input(source, prefetched_url).await?;

    if matches!(settings.frame_strategy, FrameStrategy::SceneChange) {
        return scene_change(&ffmpeg, &surl, settings, chapters, duration, work_dir).await;
    }

    let ts = timestamps(settings, chapters, duration);
    extract_at(&ffmpeg, &surl, &ts, chapters, work_dir).await
}

/// Extract one frame per timestamp, concurrently (bounded by spawn), chronologically sorted.
async fn extract_at(
    ffmpeg: &Path,
    surl: &str,
    ts: &[f64],
    chapters: &[Chapter],
    work_dir: &Path,
) -> anyhow::Result<Vec<Frame>> {
    if ts.is_empty() {
        return Ok(Vec::new());
    }
    let sem = Arc::new(Semaphore::new(MAX_CONCURRENT_SEEKS));
    let mut handles = Vec::new();
    for (idx, &t) in ts.iter().enumerate() {
        let ffmpeg = ffmpeg.to_path_buf();
        let surl = surl.to_string();
        let path = work_dir.join(format!("frame_{idx:02}.jpg"));
        let sem = sem.clone();
        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await;
            let ok = extract_one(&ffmpeg, &surl, t, &path).await;
            (t, path, ok)
        }));
    }

    let mut frames = Vec::new();
    for h in handles {
        if let Ok((t, path, ok)) = h.await {
            if ok && path.exists() {
                frames.push(Frame {
                    timestamp: t,
                    chapter: chapter_of(chapters, t),
                    path,
                });
            }
        }
    }
    frames.sort_by(|a, b| a.timestamp.partial_cmp(&b.timestamp).unwrap());
    Ok(frames)
}

async fn extract_one(ffmpeg: &Path, surl: &str, t: f64, out: &Path) -> bool {
    let status = Command::new(ffmpeg)
        .arg("-nostdin")
        .arg("-loglevel")
        .arg("error")
        .arg("-ss")
        .arg(format!("{t:.2}"))
        .arg("-i")
        .arg(surl)
        .arg("-frames:v")
        .arg("1")
        .arg("-vf")
        .arg("scale=512:-2")
        .arg("-q:v")
        .arg("3")
        .arg("-y")
        .arg(out)
        .status()
        .await;
    matches!(status, Ok(s) if s.success())
}

/// Scene-change strategy: one ffmpeg pass over the stream selecting scene cuts.
async fn scene_change(
    ffmpeg: &Path,
    surl: &str,
    settings: &Settings,
    chapters: &[Chapter],
    duration: f64,
    work_dir: &Path,
) -> anyhow::Result<Vec<Frame>> {
    let pattern = work_dir.join("scene_%03d.jpg");
    let n = settings.max_frames();
    // Select frames whose scene score exceeds a threshold, cap at N.
    let status = Command::new(ffmpeg)
        .arg("-nostdin")
        .arg("-loglevel")
        .arg("error")
        .arg("-i")
        .arg(surl)
        .arg("-vf")
        .arg("select='gt(scene,0.3)',scale=512:-2")
        .arg("-vsync")
        .arg("vfr")
        .arg("-frames:v")
        .arg(n.to_string())
        .arg("-q:v")
        .arg("3")
        .arg("-y")
        .arg(&pattern)
        .status()
        .await;

    if !matches!(status, Ok(s) if s.success()) {
        // Fall back to even sampling over the same stream if scene detection fails.
        let mut even = settings.clone();
        even.frame_strategy = FrameStrategy::Even;
        let ts = timestamps(&even, chapters, duration);
        return extract_at(ffmpeg, surl, &ts, chapters, work_dir).await;
    }

    let mut frames = Vec::new();
    if let Ok(rd) = std::fs::read_dir(work_dir) {
        let mut paths: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("scene_"))
                    .unwrap_or(false)
            })
            .collect();
        paths.sort();
        for p in paths {
            frames.push(Frame {
                timestamp: -1.0, // scene cuts are unordered in time here
                chapter: None,
                path: p,
            });
        }
    }
    Ok(frames)
}
