//! Shared application state + job tracking with live progress broadcasting.

use std::sync::Arc;

use dashmap::DashMap;
use serde::Serialize;
use tokio::sync::{broadcast, Mutex};

use crate::model::JobResult;

/// A progress event streamed to the UI over SSE.
#[derive(Debug, Clone, Serialize)]
pub struct ProgressEvent {
    /// Machine id of the stage, e.g. "fetch", "frames", "overview".
    pub stage: String,
    /// Human-readable status line.
    pub message: String,
    /// Overall completion 0.0..=1.0.
    pub progress: f32,
    /// One of: "progress", "done", "error".
    pub kind: String,
}

impl ProgressEvent {
    pub fn progress(stage: &str, message: impl Into<String>, progress: f32) -> Self {
        ProgressEvent {
            stage: stage.to_string(),
            message: message.into(),
            progress,
            kind: "progress".into(),
        }
    }
    pub fn done() -> Self {
        ProgressEvent {
            stage: "done".into(),
            message: "Complete".into(),
            progress: 1.0,
            kind: "done".into(),
        }
    }
    pub fn error(message: impl Into<String>) -> Self {
        ProgressEvent {
            stage: "error".into(),
            message: message.into(),
            progress: 1.0,
            kind: "error".into(),
        }
    }
}

#[derive(Default)]
pub struct JobInner {
    pub events: Vec<ProgressEvent>,
    pub result: Option<JobResult>,
    pub error: Option<String>,
    pub finished: bool,
}

pub struct Job {
    pub id: String,
    tx: broadcast::Sender<ProgressEvent>,
    inner: Mutex<JobInner>,
}

impl Job {
    fn new(id: String) -> Self {
        let (tx, _) = broadcast::channel(256);
        Job {
            id,
            tx,
            inner: Mutex::new(JobInner::default()),
        }
    }

    /// Subscribe to live events. Returns already-emitted events for replay plus a live receiver.
    pub async fn subscribe(&self) -> (Vec<ProgressEvent>, broadcast::Receiver<ProgressEvent>) {
        let rx = self.tx.subscribe();
        let inner = self.inner.lock().await;
        (inner.events.clone(), rx)
    }

    pub async fn emit(&self, ev: ProgressEvent) {
        {
            let mut inner = self.inner.lock().await;
            inner.events.push(ev.clone());
        }
        let _ = self.tx.send(ev);
    }

    pub async fn complete(&self, result: JobResult) {
        {
            let mut inner = self.inner.lock().await;
            inner.result = Some(result);
            inner.finished = true;
        }
        self.emit(ProgressEvent::done()).await;
    }

    pub async fn fail(&self, message: String) {
        {
            let mut inner = self.inner.lock().await;
            inner.error = Some(message.clone());
            inner.finished = true;
        }
        self.emit(ProgressEvent::error(message)).await;
    }

    pub async fn result(&self) -> Option<JobResult> {
        self.inner.lock().await.result.clone()
    }

    pub async fn snapshot_error(&self) -> Option<String> {
        self.inner.lock().await.error.clone()
    }

    pub async fn is_finished(&self) -> bool {
        self.inner.lock().await.finished
    }
}

#[derive(Clone, Default)]
pub struct AppState {
    jobs: Arc<DashMap<String, Arc<Job>>>,
}

impl AppState {
    pub fn new() -> Self {
        AppState::default()
    }

    pub fn create_job(&self) -> Arc<Job> {
        let id = uuid::Uuid::new_v4().to_string();
        let job = Arc::new(Job::new(id.clone()));
        self.jobs.insert(id, job.clone());
        job
    }

    pub fn get(&self, id: &str) -> Option<Arc<Job>> {
        self.jobs.get(id).map(|j| j.clone())
    }
}
