//! ffprobe metadata for locally-uploaded media files (no yt-dlp / network).

use std::path::Path;

use anyhow::{anyhow, Context};
use serde_json::Value;
use tokio::process::Command;

use crate::model::{Chapter, VideoMeta};
use crate::tools;

/// Result of probing a local file.
pub struct ProbeResult {
    pub meta: VideoMeta,
    pub chapters: Vec<Chapter>,
    pub has_audio: bool,
    pub has_video: bool,
}

/// Probe a local audio/video file for duration, title, and embedded chapters.
pub async fn meta(file: &Path) -> anyhow::Result<ProbeResult> {
    let ffprobe = tools::ffprobe()?;
    let out = Command::new(&ffprobe)
        .arg("-v")
        .arg("quiet")
        .arg("-print_format")
        .arg("json")
        .arg("-show_format")
        .arg("-show_streams")
        .arg("-show_chapters")
        .arg(file)
        .output()
        .await
        .context("failed to launch ffprobe")?;
    if !out.status.success() {
        return Err(anyhow!(
            "ffprobe failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let v: Value = serde_json::from_slice(&out.stdout).context("ffprobe JSON parse")?;

    let format = v.get("format").cloned().unwrap_or(Value::Null);
    let tag = |k: &str| {
        format
            .get("tags")
            .and_then(|t| t.get(k))
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string()
    };

    // Title: container tag if present, else the file's stem.
    let stem = file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Local media")
        .to_string();
    let mut title = tag("title");
    if title.is_empty() {
        title = stem;
    }

    let format_duration = format
        .get("duration")
        .and_then(|d| d.as_str())
        .and_then(|d| d.parse::<f64>().ok())
        .unwrap_or(0.0);
    let streams = v
        .get("streams")
        .and_then(|s| s.as_array())
        .cloned()
        .unwrap_or_default();
    let has_audio = streams.iter().any(|s| {
        s.get("codec_type")
            .and_then(|t| t.as_str())
            .map(|t| t == "audio")
            .unwrap_or(false)
    });
    let has_video = streams.iter().any(|s| {
        s.get("codec_type")
            .and_then(|t| t.as_str())
            .map(|t| t == "video")
            .unwrap_or(false)
    });
    let stream_duration = streams
        .iter()
        .filter_map(|s| {
            s.get("duration")
                .and_then(|d| d.as_str())
                .and_then(|d| d.parse::<f64>().ok())
        })
        .fold(0.0, f64::max);
    let duration = format_duration.max(stream_duration);

    let meta = VideoMeta {
        title,
        uploader: tag("artist"),
        channel: tag("artist"),
        duration,
        webpage_url: file.to_string_lossy().to_string(),
        upload_date: String::new(),
        ..Default::default()
    };

    let chapters = v
        .get("chapters")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .map(|c| Chapter {
                    title: c
                        .get("tags")
                        .and_then(|t| t.get("title"))
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
                    start: c
                        .get("start_time")
                        .and_then(|x| x.as_str())
                        .and_then(|x| x.parse::<f64>().ok())
                        .unwrap_or(0.0),
                    end: c
                        .get("end_time")
                        .and_then(|x| x.as_str())
                        .and_then(|x| x.parse::<f64>().ok())
                        .unwrap_or(0.0),
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(ProbeResult {
        meta,
        chapters,
        has_audio,
        has_video,
    })
}
