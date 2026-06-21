//! HTTP API surface.

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::stream::Stream;
use serde_json::json;
use tower_http::cors::CorsLayer;

use crate::error::{AppError, AppResult};
use crate::model::ProcessRequest;
use crate::pipeline;
use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/process", post(process))
        .route("/events/:id", get(events))
        .route("/result/:id", get(result))
        .layer(CorsLayer::very_permissive())
        .with_state(state)
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok", "service": "yt2overview", "version": env!("CARGO_PKG_VERSION") }))
}

/// Kick off a processing job. Returns a job id immediately; progress streams over `/events/:id`.
async fn process(
    State(state): State<AppState>,
    Json(req): Json<ProcessRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let url = req.url.trim().to_string();
    if url.is_empty() {
        return Err(AppError::BadRequest("url is required".into()));
    }
    let job = state.create_job();
    let id = job.id.clone();
    tokio::spawn(pipeline::run(job, ProcessRequest { url, ..req }));
    Ok(Json(json!({ "job_id": id })))
}

/// Server-sent events stream of progress for a job. Replays prior events to late subscribers.
async fn events(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    let job = state
        .get(&id)
        .ok_or_else(|| AppError::NotFound(format!("job {id}")))?;

    let (replay, mut rx) = job.subscribe().await;
    let already_finished = job.is_finished().await;

    let stream = async_stream::stream! {
        for ev in replay {
            let kind = ev.kind.clone();
            yield Ok(Event::default().json_data(&ev).unwrap_or_default());
            if kind == "done" || kind == "error" {
                return;
            }
        }
        if already_finished {
            return;
        }
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    let kind = ev.kind.clone();
                    yield Ok(Event::default().json_data(&ev).unwrap_or_default());
                    if kind == "done" || kind == "error" {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}

/// Fetch the final result once a job has completed.
async fn result(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let job = state
        .get(&id)
        .ok_or_else(|| AppError::NotFound(format!("job {id}")))?;

    if let Some(result) = job.result().await {
        return Ok(Json(json!({ "status": "done", "result": result })));
    }
    if let Some(err) = job.snapshot_error().await {
        return Ok(Json(json!({ "status": "error", "error": err })));
    }
    Ok(Json(json!({ "status": "running" })))
}
