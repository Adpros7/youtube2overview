//! Shared application state + job tracking with live progress broadcasting.

use std::sync::Arc;

use dashmap::DashMap;
use serde::Serialize;
use tokio::sync::{broadcast, Mutex};
use tokio::task::AbortHandle;

use crate::mlx::MlxManager;
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
            if inner.finished {
                return;
            }
            inner.events.push(ev.clone());
        }
        let _ = self.tx.send(ev);
    }

    pub async fn complete(&self, result: JobResult) {
        let should_send = {
            let mut inner = self.inner.lock().await;
            if inner.finished {
                return;
            }
            inner.result = Some(result);
            inner.finished = true;
            let event = ProgressEvent::done();
            inner.events.push(event.clone());
            event
        };
        let _ = self.tx.send(should_send);
    }

    pub async fn fail(&self, message: String) {
        let should_send = {
            let mut inner = self.inner.lock().await;
            if inner.finished {
                return;
            }
            inner.error = Some(message.clone());
            inner.finished = true;
            let event = ProgressEvent::error(message);
            inner.events.push(event.clone());
            event
        };
        let _ = self.tx.send(should_send);
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

#[derive(Clone)]
pub struct AppState {
    jobs: Arc<DashMap<String, Arc<Job>>>,
    tasks: Arc<DashMap<String, AbortHandle>>,
    pub mlx: Arc<MlxManager>,
}

impl AppState {
    pub fn new() -> Self {
        AppState {
            jobs: Arc::new(DashMap::new()),
            tasks: Arc::new(DashMap::new()),
            mlx: MlxManager::new(),
        }
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

    pub fn track_task(&self, id: String, handle: AbortHandle) {
        self.tasks.insert(id, handle);
    }

    /// Mark the job cancelled before aborting its task so event subscribers receive
    /// a terminal state even when the pipeline is currently blocked in a subprocess.
    pub async fn cancel(&self, id: &str) -> bool {
        let Some(job) = self.get(id) else { return false };
        if job.is_finished().await { return false }
        job.fail("Cancelled".to_string()).await;
        if let Some((_, handle)) = self.tasks.remove(id) {
            handle.abort();
        }
        true
    }
}
