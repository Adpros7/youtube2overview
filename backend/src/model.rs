//! Shared data model for a processing job: inputs, intermediate data, and final outputs.

use serde::{Deserialize, Serialize};

use crate::config::Settings;

/// Incoming request body for `POST /process`.
#[derive(Debug, Clone, Deserialize)]
pub struct ProcessRequest {
    pub url: String,
    #[serde(default)]
    pub settings: Settings,
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
