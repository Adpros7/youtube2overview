//! Job pipeline orchestration. Individual stages live in submodules and are wired
//! up across Phases 2–6.

pub mod frames;
pub mod infer;
pub mod ytdlp;

use std::sync::Arc;

use crate::mlx::MlxManager;
use crate::model::{JobData, JobResult, Outputs, ProcessRequest};
use crate::state::{Job, ProgressEvent};

/// Convenience wrapper around a job for emitting staged progress.
pub struct Reporter {
    job: Arc<Job>,
}

impl Reporter {
    pub fn new(job: Arc<Job>) -> Self {
        Reporter { job }
    }
    pub async fn stage(&self, stage: &str, message: impl Into<String>, progress: f32) {
        self.job
            .emit(ProgressEvent::progress(stage, message, progress))
            .await;
    }
    pub fn clone_job(&self) -> Arc<Job> {
        self.job.clone()
    }
}

/// Run the full pipeline for a job. Emits progress as it goes and stores the result.
pub async fn run(job: Arc<Job>, mlx: Arc<MlxManager>, req: ProcessRequest) {
    let reporter = Reporter::new(job.clone());
    match run_inner(&reporter, &mlx, &req).await {
        Ok((data, outputs)) => {
            job.complete(JobResult {
                data,
                outputs,
                settings: req.settings,
            })
            .await;
        }
        Err(err) => {
            tracing::error!("pipeline failed: {err:#}");
            job.fail(format!("{err:#}")).await;
        }
    }
}

async fn run_inner(
    reporter: &Reporter,
    mlx: &Arc<MlxManager>,
    req: &ProcessRequest,
) -> anyhow::Result<(JobData, Outputs)> {
    let settings = &req.settings;
    let _ = mlx; // used by the model stage (Phase 5)
    let work = tempfile::tempdir().map_err(|e| anyhow::anyhow!("tempdir: {e}"))?;
    let mut data = JobData::default();

    // --- Stage: fetch metadata, chapters, comments ---
    reporter.stage("fetch", "Fetching video metadata…", 0.05).await;
    let dump = ytdlp::dump(&req.url, settings).await?;
    data.meta = dump.meta;
    data.chapters = dump.chapters;
    data.comments = dump.comments;
    reporter
        .stage(
            "fetch",
            format!("Got “{}”", truncate(&data.meta.title, 60)),
            0.18,
        )
        .await;

    // --- Stage: transcript ---
    if settings.include_transcript || settings.sections.ai_overview {
        reporter.stage("transcript", "Fetching transcript…", 0.22).await;
        let (cues, lang) = ytdlp::transcript(&req.url, settings, work.path()).await?;
        let n = cues.len();
        data.cues = cues;
        data.transcript_lang = lang;
        reporter
            .stage("transcript", format!("Transcript: {n} cues"), 0.35)
            .await;
    }

    // --- Stage: keyframes for the visual overview ---
    if settings.include_visual && settings.max_frames() > 0 {
        reporter.stage("frames", "Extracting keyframes…", 0.4).await;
        match frames::extract(
            &req.url,
            settings,
            &data.chapters,
            data.meta.duration,
            work.path(),
        )
        .await
        {
            Ok(frames) => {
                data.frame_count = frames.len();
                data.frames = frames;
                reporter
                    .stage(
                        "frames",
                        format!("Extracted {} frames", data.frame_count),
                        0.5,
                    )
                    .await;
            }
            Err(e) => {
                tracing::warn!("frame extraction failed: {e:#}");
                reporter
                    .stage("frames", "Skipped frames (unavailable)", 0.5)
                    .await;
            }
        }
    }

    // --- Stage: model overviews (text + visual) ---
    let need_text = settings.sections.ai_overview && !data.cues.is_empty();
    let need_visual = settings.sections.visual_overview && !data.frames.is_empty();
    if need_text || need_visual {
        reporter
            .stage("model", "Preparing local model…", 0.55)
            .await;
        let reporter_status = reporter.clone_job();
        let status = move |msg: String| {
            let job = reporter_status.clone();
            tokio::spawn(async move {
                job.emit(ProgressEvent::progress("model", msg, 0.58)).await;
            });
        };
        let endpoint = mlx
            .ensure(&settings.model, settings.mlx_port, &status)
            .await?;
        data.model_used = endpoint.model_id.clone();

        if need_text {
            reporter.stage("model", "Writing AI overview…", 0.66).await;
            match infer::text_overview(&endpoint, settings, &data.meta, &data.chapters, &data.cues)
                .await
            {
                Ok(t) => data.ai_overview = t,
                Err(e) => tracing::warn!("text overview failed: {e:#}"),
            }
        }
        if need_visual {
            reporter
                .stage("model", "Describing visuals…", 0.82)
                .await;
            match infer::visual_overview(&endpoint, settings, &data.meta, &data.frames).await {
                Ok(t) => data.visual_overview = t,
                Err(e) => tracing::warn!("visual overview failed: {e:#}"),
            }
        }
    }

    reporter.stage("assemble", "Assembling output…", 0.95).await;
    // Final assembly (Phase 6) replaces this basic dump next.
    let outputs = Outputs {
        human_markdown: basic_markdown(&data),
        ai_payload: String::new(),
        sections: Vec::new(),
    };

    reporter.stage("done", "Done", 1.0).await;
    Ok((data, outputs))
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let t: String = s.chars().take(n).collect();
        format!("{t}…")
    }
}

/// Temporary assembly until Phase 6 replaces it.
fn basic_markdown(data: &JobData) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", data.meta.title));
    out.push_str(&format!("**Channel:** {}\n\n", data.meta.channel));
    if !data.ai_overview.is_empty() {
        out.push_str("## AI Overview\n\n");
        out.push_str(&data.ai_overview);
        out.push_str("\n\n");
    }
    if !data.visual_overview.is_empty() {
        out.push_str("## Visual Overview\n\n");
        out.push_str(&data.visual_overview);
        out.push_str("\n\n");
    }
    if !data.chapters.is_empty() {
        out.push_str("## Chapters\n\n");
        for c in &data.chapters {
            out.push_str(&format!("- {} ({:.0}s)\n", c.title, c.start));
        }
        out.push('\n');
    }
    if !data.comments.is_empty() {
        out.push_str(&format!("## Top {} comments\n\n", data.comments.len()));
        for c in &data.comments {
            out.push_str(&format!("- **{}** ({}): {}\n", c.author, c.likes, c.text));
        }
        out.push('\n');
    }
    out.push_str(&format!("## Transcript ({} cues)\n\n", data.cues.len()));
    for cue in &data.cues {
        out.push_str(&format!("[{:.0}s] {}\n", cue.start, cue.text));
    }
    out
}
