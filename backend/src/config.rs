//! Granular, user-controllable settings. Every knob the UI exposes lives here so the
//! Rust pipeline and the SwiftUI settings panel share one contract.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommentSort {
    Top,
    New,
}

impl Default for CommentSort {
    fn default() -> Self {
        CommentSort::Top
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameStrategy {
    /// Evenly spaced across the full duration.
    Even,
    /// One frame near the start of each chapter (falls back to Even when no chapters).
    Chapters,
    /// ffmpeg scene-change detection (most "informative" frames).
    SceneChange,
}

impl Default for FrameStrategy {
    fn default() -> Self {
        FrameStrategy::Even
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverviewLength {
    Brief,
    Standard,
    Detailed,
}

impl Default for OverviewLength {
    fn default() -> Self {
        OverviewLength::Standard
    }
}

/// Which sections to include in the assembled output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sections {
    pub ai_preamble: bool,
    pub metadata: bool,
    pub chapters: bool,
    pub ai_overview: bool,
    pub visual_overview: bool,
    pub comments: bool,
    pub transcript: bool,
}

impl Default for Sections {
    fn default() -> Self {
        Sections {
            ai_preamble: true,
            metadata: true,
            chapters: true,
            ai_overview: true,
            visual_overview: true,
            comments: true,
            transcript: true,
        }
    }
}

/// The full settings payload sent with a process request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    // ---- Model / serving ----
    /// rapid-mlx model alias or HF id. Defaults to the cached multimodal Gemma 4.
    pub model: String,
    /// Whisper model (mlx-whisper) for transcribing uploaded local media files.
    pub whisper_model: String,
    /// Force a specific server port; 0 = auto (reuse running or pick free).
    pub mlx_port: u16,
    pub temperature: f32,
    pub max_tokens: u32,

    // ---- Comments ----
    pub include_comments: bool,
    pub max_comments: u32,
    pub comment_sort: CommentSort,

    // ---- Frames / vision ----
    pub include_visual: bool,
    /// Number of visual samples to send to the vision model.
    /// `-1` = auto by duration, `0` = disabled, positive values = explicit cap.
    pub max_frames: i32,
    pub frame_strategy: FrameStrategy,

    // ---- Overview ----
    pub overview_length: OverviewLength,
    /// Free-form style hint, e.g. "neutral", "bullet points", "ELI5".
    pub overview_style: String,
    /// Preferred transcript/output language code, e.g. "en". Empty = auto.
    pub language: String,

    // ---- Transcript ----
    pub include_transcript: bool,
    /// Keep inline `[mm:ss]` timestamps in the transcript section.
    pub transcript_timestamps: bool,

    // ---- Output composition ----
    pub sections: Sections,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            model: "mlx-community/gemma-4-12b-it-4bit".to_string(),
            whisper_model: "mlx-community/whisper-large-v3-turbo".to_string(),
            mlx_port: 0,
            temperature: 0.4,
            max_tokens: 1536,
            include_comments: true,
            max_comments: 20,
            comment_sort: CommentSort::Top,
            include_visual: true,
            max_frames: -1,
            frame_strategy: FrameStrategy::Even,
            overview_length: OverviewLength::Standard,
            overview_style: "neutral, informative".to_string(),
            language: String::new(),
            include_transcript: true,
            transcript_timestamps: true,
            sections: Sections::default(),
        }
    }
}

impl Settings {
    pub fn max_comments(&self) -> u32 {
        self.max_comments.clamp(0, 200)
    }
    pub fn frame_sample_count(&self, duration: f64) -> usize {
        if self.max_frames < 0 {
            if duration <= 0.0 {
                return 8;
            }
            // Automatic mode covers the whole media with roughly one sample every
            // 30 seconds, while keeping the vision prompt bounded.
            ((duration / 30.0).ceil() as usize).clamp(8, 32)
        } else {
            self.max_frames.clamp(0, 32) as usize
        }
    }
}
