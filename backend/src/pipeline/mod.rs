//! Job pipeline orchestration. Individual stages live in submodules and are wired
//! up across Phases 2–6.

use std::sync::Arc;

use crate::config::Settings;
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
}

/// Run the full pipeline for a job. Emits progress as it goes and stores the result.
pub async fn run(job: Arc<Job>, req: ProcessRequest) {
    let reporter = Reporter::new(job.clone());
    match run_inner(&reporter, &req).await {
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
    req: &ProcessRequest,
) -> anyhow::Result<(JobData, Outputs)> {
    let _settings: &Settings = &req.settings;
    reporter.stage("start", "Starting…", 0.02).await;

    // Phase 1 placeholder: real stages (fetch, frames, model, assemble) are wired in
    // Phases 2–6. For now produce an empty-but-valid result so the SSE + result flow
    // can be exercised end to end.
    let data = JobData::default();
    let outputs = Outputs {
        human_markdown: format!("# (pipeline not yet wired)\n\nURL: {}\n", req.url),
        ai_payload: String::new(),
        sections: Vec::new(),
    };

    reporter.stage("done", "Done", 1.0).await;
    Ok((data, outputs))
}
