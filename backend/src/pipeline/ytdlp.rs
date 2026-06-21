//! yt-dlp integration: metadata, chapters, comments, and transcript.

use std::path::Path;

use anyhow::{anyhow, Context};
use serde_json::Value;
use tokio::process::Command;

use crate::config::{CommentSort, Settings};
use crate::model::{Chapter, Comment, Cue, VideoMeta};
use crate::tools;

/// Result of the single metadata/comments dump pass.
pub struct DumpResult {
    pub meta: VideoMeta,
    pub chapters: Vec<Chapter>,
    pub comments: Vec<Comment>,
}

fn s(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_string()
}
fn f(v: &Value, key: &str) -> f64 {
    v.get(key).and_then(|x| x.as_f64()).unwrap_or(0.0)
}
fn i(v: &Value, key: &str) -> i64 {
    v.get(key).and_then(|x| x.as_i64()).unwrap_or(0)
}

/// Run `yt-dlp -J` (optionally with comments) and parse metadata, chapters, comments.
pub async fn dump(url: &str, settings: &Settings) -> anyhow::Result<DumpResult> {
    let yt = tools::yt_dlp()?;
    let mut cmd = Command::new(&yt);
    cmd.arg("-J")
        .arg("--no-warnings")
        .arg("--no-playlist")
        .arg(url);

    let want_comments = settings.include_comments && settings.max_comments() > 0;
    if want_comments {
        let n = settings.max_comments();
        let sort = match settings.comment_sort {
            CommentSort::Top => "top",
            CommentSort::New => "new",
        };
        // Fetch top-level comments only, capped at N, sorted as requested.
        cmd.arg("--write-comments").arg("--extractor-args").arg(format!(
            "youtube:max_comments={n},all,0,0;comment_sort={sort}"
        ));
    }

    let out = cmd
        .output()
        .await
        .context("failed to launch yt-dlp")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(anyhow!("yt-dlp dump failed: {}", err.trim()));
    }
    let v: Value = serde_json::from_slice(&out.stdout).context("yt-dlp JSON parse")?;

    let meta = VideoMeta {
        id: s(&v, "id"),
        title: s(&v, "title"),
        uploader: s(&v, "uploader"),
        channel: s(&v, "channel"),
        duration: f(&v, "duration"),
        view_count: i(&v, "view_count"),
        like_count: i(&v, "like_count"),
        upload_date: s(&v, "upload_date"),
        webpage_url: s(&v, "webpage_url"),
        thumbnail: s(&v, "thumbnail"),
        description: s(&v, "description"),
    };

    let chapters = v
        .get("chapters")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .map(|c| Chapter {
                    title: s(c, "title"),
                    start: f(c, "start_time"),
                    end: f(c, "end_time"),
                })
                .collect()
        })
        .unwrap_or_default();

    let mut comments: Vec<Comment> = v
        .get("comments")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                // top-level only (parent == "root")
                .filter(|c| s(c, "parent") == "root" || c.get("parent").is_none())
                .map(|c| Comment {
                    author: s(c, "author"),
                    text: s(c, "text"),
                    likes: i(c, "like_count"),
                    is_favorited: c.get("is_favorited").and_then(|b| b.as_bool()).unwrap_or(false),
                })
                .collect()
        })
        .unwrap_or_default();

    if matches!(settings.comment_sort, CommentSort::Top) {
        comments.sort_by(|a, b| b.likes.cmp(&a.likes));
    }
    comments.truncate(settings.max_comments() as usize);

    Ok(DumpResult {
        meta,
        chapters,
        comments,
    })
}

/// Download auto/manual subtitles and parse them into cues. Returns (cues, lang).
pub async fn transcript(
    url: &str,
    settings: &Settings,
    work_dir: &Path,
) -> anyhow::Result<(Vec<Cue>, String)> {
    let yt = tools::yt_dlp()?;
    let pref = if settings.language.trim().is_empty() {
        "en.*,en".to_string()
    } else {
        let l = settings.language.trim();
        format!("{l}.*,{l},en.*,en")
    };
    let out_tmpl = work_dir.join("sub.%(ext)s");

    let out = Command::new(&yt)
        .arg("--skip-download")
        .arg("--write-auto-subs")
        .arg("--write-subs")
        .arg("--no-warnings")
        .arg("--no-playlist")
        .arg("--sub-langs")
        .arg(&pref)
        .arg("--sub-format")
        .arg("json3/srv3/vtt")
        .arg("-o")
        .arg(out_tmpl.to_string_lossy().to_string())
        .arg(url)
        .output()
        .await
        .context("failed to launch yt-dlp for subtitles")?;

    // Non-fatal: a video may simply have no captions.
    if !out.status.success() {
        tracing::warn!(
            "yt-dlp subtitle fetch returned non-zero: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }

    // Find the best subtitle file produced.
    let mut best: Option<(std::path::PathBuf, String)> = None;
    if let Ok(rd) = std::fs::read_dir(work_dir) {
        for ent in rd.flatten() {
            let p = ent.path();
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !name.starts_with("sub.") {
                continue;
            }
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
            // language tag sits between "sub." and the format extension
            let lang = name
                .trim_start_matches("sub.")
                .rsplit_once('.')
                .map(|(l, _)| l.to_string())
                .unwrap_or_default();
            let rank = match ext {
                "json3" => 3,
                "srv3" => 2,
                "vtt" => 1,
                _ => 0,
            };
            let cur_rank = best
                .as_ref()
                .map(|(bp, _)| match bp.extension().and_then(|e| e.to_str()) {
                    Some("json3") => 3,
                    Some("srv3") => 2,
                    Some("vtt") => 1,
                    _ => 0,
                })
                .unwrap_or(0);
            if rank > cur_rank {
                best = Some((p, lang));
            }
        }
    }

    let Some((path, lang)) = best else {
        return Ok((Vec::new(), String::new()));
    };
    let raw = std::fs::read_to_string(&path).unwrap_or_default();
    let cues = match path.extension().and_then(|e| e.to_str()) {
        Some("json3") | Some("srv3") => parse_json3(&raw),
        _ => parse_vtt(&raw),
    };
    Ok((cues, lang))
}

/// Parse YouTube json3/srv3 caption format.
fn parse_json3(raw: &str) -> Vec<Cue> {
    let Ok(v) = serde_json::from_str::<Value>(raw) else {
        return Vec::new();
    };
    let Some(events) = v.get("events").and_then(|e| e.as_array()) else {
        return Vec::new();
    };
    let mut cues = Vec::new();
    for e in events {
        let start = e.get("tStartMs").and_then(|x| x.as_f64()).unwrap_or(0.0) / 1000.0;
        let text = e
            .get("segs")
            .and_then(|s| s.as_array())
            .map(|segs| {
                segs.iter()
                    .filter_map(|s| s.get("utf8").and_then(|u| u.as_str()))
                    .collect::<String>()
            })
            .unwrap_or_default();
        let text = text.replace('\n', " ");
        let text = text.trim();
        if !text.is_empty() {
            cues.push(Cue {
                start,
                text: text.to_string(),
            });
        }
    }
    cues
}

/// Minimal WebVTT parser (fallback). De-duplicates consecutive identical lines
/// (rolling auto-captions repeat).
fn parse_vtt(raw: &str) -> Vec<Cue> {
    let mut cues: Vec<Cue> = Vec::new();
    let mut cur_start: Option<f64> = None;
    let mut buf: Vec<String> = Vec::new();

    let flush = |cues: &mut Vec<Cue>, start: Option<f64>, buf: &mut Vec<String>| {
        if let Some(start) = start {
            let text = buf.join(" ");
            let text = strip_tags(&text);
            let text = text.trim();
            if !text.is_empty() && cues.last().map(|c| c.text != text).unwrap_or(true) {
                cues.push(Cue {
                    start,
                    text: text.to_string(),
                });
            }
        }
        buf.clear();
    };

    for line in raw.lines() {
        let line = line.trim();
        if line.contains("-->") {
            flush(&mut cues, cur_start.take(), &mut buf);
            cur_start = line
                .split("-->")
                .next()
                .and_then(|t| parse_ts(t.trim()));
        } else if line.is_empty()
            || line == "WEBVTT"
            || line.starts_with("Kind:")
            || line.starts_with("Language:")
            || line.starts_with("NOTE")
        {
            if line.is_empty() {
                flush(&mut cues, cur_start.take(), &mut buf);
            }
        } else if cur_start.is_some() {
            buf.push(line.to_string());
        }
    }
    flush(&mut cues, cur_start.take(), &mut buf);
    cues
}

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

fn parse_ts(t: &str) -> Option<f64> {
    // HH:MM:SS.mmm or MM:SS.mmm
    let t = t.split_whitespace().next().unwrap_or(t);
    let parts: Vec<&str> = t.split(':').collect();
    let (h, m, s) = match parts.as_slice() {
        [h, m, s] => (h.parse::<f64>().ok()?, m.parse::<f64>().ok()?, s.replace(',', ".").parse::<f64>().ok()?),
        [m, s] => (0.0, m.parse::<f64>().ok()?, s.replace(',', ".").parse::<f64>().ok()?),
        _ => return None,
    };
    Some(h * 3600.0 + m * 60.0 + s)
}
