//! Job pipeline orchestration. Individual stages live in submodules and are wired
//! up across Phases 2–6.

pub mod assemble;
pub mod frames;
pub mod infer;
pub mod probe;
pub mod transcribe;
pub mod ytdlp;

use std::sync::Arc;

use crate::mlx::MlxManager;
use crate::model::{JobData, JobResult, Outputs, ProcessRequest, Source};
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
    let work = tempfile::tempdir().map_err(|e| anyhow::anyhow!("tempdir: {e}"))?;
    let mut data = JobData::default();

    let source = Source::classify(&req.url);
    // Stream URL and caption track resolved from the YouTube dump (reused downstream to
    // avoid extra yt-dlp extractions).
    let mut stream_url: Option<String> = None;
    let mut caption: Option<ytdlp::CaptionRef> = None;
    let mut local_has_audio = true;
    let mut local_has_video = true;

    // --- Stage: fetch metadata, chapters, comments ---
    reporter
        .stage("fetch", "Fetching media metadata…", 0.05)
        .await;
    match &source {
        Source::YouTube(url) => {
            let dump = ytdlp::dump(url, settings).await?;
            data.meta = dump.meta;
            data.chapters = dump.chapters;
            data.comments = dump.comments;
            stream_url = dump.stream_url;
            caption = dump.caption;
        }
        Source::Local(path) => {
            let probed = probe::meta(path).await?;
            local_has_audio = probed.has_audio;
            local_has_video = probed.has_video;
            data.meta = probed.meta;
            data.chapters = probed.chapters;
            // No comments for local files.
        }
    }
    reporter
        .stage(
            "fetch",
            format!("Got “{}”", truncate(&data.meta.title, 60)),
            0.18,
        )
        .await;

    // --- Stage: transcript ---
    if settings.include_transcript || settings.sections.ai_overview {
        let (cues, lang) = match &source {
            Source::YouTube(url) => {
                reporter
                    .stage("transcript", "Fetching transcript…", 0.22)
                    .await;
                // Prefer the caption URL from the dump (no second yt-dlp); fall back to
                // the yt-dlp subtitle download if the direct fetch yields nothing.
                let mut got: Option<(Vec<crate::model::Cue>, String)> = None;
                if let Some((lang, ext, curl)) = &caption {
                    if let Ok(cues) = ytdlp::fetch_caption(curl, ext).await {
                        if !cues.is_empty() {
                            got = Some((cues, lang.clone()));
                        }
                    }
                }
                match got {
                    Some(g) => g,
                    None => ytdlp::transcript(url, settings, work.path()).await?,
                }
            }
            Source::Local(path) => {
                reporter
                    .stage("transcript", "Transcribing audio (local)…", 0.22)
                    .await;
                if !local_has_audio {
                    tracing::warn!("local file has no audio stream; checking subtitles only");
                }
                match transcribe::local(path, settings, work.path()).await {
                    Ok(g) => g,
                    Err(e) => {
                        tracing::warn!("local transcription failed: {e:#}");
                        (Vec::new(), String::new())
                    }
                }
            }
        };
        let n = cues.len();
        data.cues = cues;
        data.transcript_lang = lang;
        reporter
            .stage("transcript", format!("Transcript: {n} cues"), 0.35)
            .await;
    }

    // --- Stage: keyframes for the visual overview ---
    if settings.include_visual
        && local_has_video
        && settings.frame_sample_count(data.meta.duration) > 0
    {
        reporter.stage("frames", "Extracting keyframes…", 0.4).await;
        match frames::extract(
            &source,
            stream_url.as_deref(),
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
            reporter.stage("model", "Writing AI overview…", 0.6).await;
            let job_for_text = reporter.clone_job();
            let progress = move |done: usize, total: usize| {
                let job = job_for_text.clone();
                tokio::spawn(async move {
                    let (msg, frac) = if total > 1 {
                        (
                            format!("Summarizing transcript… part {}/{}", done.min(total), total),
                            0.6 + 0.18 * (done as f32 / total as f32),
                        )
                    } else {
                        ("Writing AI overview…".to_string(), 0.66)
                    };
                    job.emit(ProgressEvent::progress("model", msg, frac)).await;
                });
            };
            match infer::text_overview(
                &endpoint,
                settings,
                &data.meta,
                &data.chapters,
                &data.cues,
                &progress,
            )
            .await
            {
                Ok(t) => data.ai_overview = t,
                Err(e) => tracing::warn!("text overview failed: {e:#}"),
            }
        }
        if need_visual {
            reporter.stage("model", "Describing visuals…", 0.82).await;
            match infer::visual_overview(&endpoint, settings, &data.meta, &data.frames).await {
                Ok(t) => data.visual_overview = t,
                Err(e) => tracing::warn!("visual overview failed: {e:#}"),
            }
        }
    }

    reporter.stage("assemble", "Assembling output…", 0.95).await;
    let outputs = assemble::assemble(&data, settings);

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
