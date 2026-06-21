//! Shared data model for a processing job: inputs, intermediate data, and final outputs.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::Settings;

/// Incoming request body for `POST /process`.
#[derive(Debug, Clone, Deserialize)]
pub struct ProcessRequest {
    /// A web media URL or a local audio/video file path / `file://` URL,
    /// classified by [`Source`].
    pub url: String,
    #[serde(default)]
    pub settings: Settings,
}

/// Where the media comes from. The same `url` field carries either a web URL or a
/// local path; `classify` decides which.
#[derive(Debug, Clone)]
pub enum Source {
    YouTube(String),
    Local(PathBuf),
}

impl Source {
    /// Classify the request input: a local media file (existing path or `file://`
    /// URL) vs. a web URL handled by yt-dlp.
    pub fn classify(input: &str) -> Source {
        let trimmed = input.trim();
        if let Some(rest) = trimmed.strip_prefix("file://") {
            // Percent-decoding is unlikely to matter for picker-produced paths; take the
            // path verbatim, trimming an authority-less leading host if present.
            let path = rest.strip_prefix("localhost").unwrap_or(rest);
            return Source::Local(PathBuf::from(path));
        }
        let looks_web = trimmed.starts_with("http://") || trimmed.starts_with("https://");
        if !looks_web {
            let p = PathBuf::from(trimmed);
            if p.is_file() {
                return Source::Local(p);
            }
        }
        Source::YouTube(trimmed.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Chapter {
    pub title: String,
    pub start: f64,
    pub end: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    pub author: String,
    pub text: String,
    pub likes: i64,
    #[serde(default)]
    pub is_favorited: bool,
}

/// A single transcript cue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cue {
    pub start: f64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VideoMeta {
    pub id: String,
    pub title: String,
    pub uploader: String,
    pub channel: String,
    pub duration: f64,
    pub view_count: i64,
    pub like_count: i64,
    pub upload_date: String,
    pub webpage_url: String,
    pub thumbnail: String,
    #[serde(default)]
    pub description: String,
}

/// A keyframe extracted for the vision model.
#[derive(Debug, Clone, Serialize)]
pub struct Frame {
    pub timestamp: f64,
    /// Absolute path to the extracted JPEG on disk.
    #[serde(skip)]
    pub path: std::path::PathBuf,
    /// Chapter title this frame falls in, if any.
    pub chapter: Option<String>,
}

/// Everything gathered + generated, before assembly into text.
#[derive(Debug, Clone, Serialize, Default)]
pub struct JobData {
    pub meta: VideoMeta,
    pub chapters: Vec<Chapter>,
    pub comments: Vec<Comment>,
    pub cues: Vec<Cue>,
    /// Extracted keyframes (image paths are kept in-process, not serialized).
    pub frames: Vec<Frame>,
    pub transcript_lang: String,
    /// Model-generated text overview.
    pub ai_overview: String,
    /// Model-generated visual overview (from frames).
    pub visual_overview: String,
    pub frame_count: usize,
    pub model_used: String,
}

/// Final assembled outputs handed to the UI.
#[derive(Debug, Clone, Serialize, Default)]
pub struct Outputs {
    /// Nicely formatted Markdown for humans to read.
    pub human_markdown: String,
    /// Token-efficient payload with an AI instruction preamble.
    pub ai_payload: String,
    /// Individually copiable sections keyed by id (transcript, comments, ...).
    pub sections: Vec<OutputSection>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OutputSection {
    pub id: String,
    pub title: String,
    pub markdown: String,
}

/// The settings echoed back so the UI knows what actually ran.
#[derive(Debug, Clone, Serialize)]
pub struct JobResult {
    pub data: JobData,
    pub outputs: Outputs,
    pub settings: Settings,
}
