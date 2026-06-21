//! rapid-mlx server orchestration.
//!
//! Strategy: reuse a server that is already serving the requested model; otherwise
//! start one on a free port. The handle is cached in [`MlxManager`] so subsequent jobs
//! reuse the same server instead of reloading the model.

use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context};
use tokio::sync::Mutex;

use crate::tools;

/// A resolved, ready-to-use OpenAI-compatible endpoint.
#[derive(Clone, Debug)]
pub struct Endpoint {
    pub base_url: String,
    pub model_id: String,
}

struct Running {
    port: u16,
    /// What we asked rapid-mlx to serve (repo id or alias).
    requested: String,
    model_id: String,
    /// `Some` if we started it (so we could stop it); `None` if pre-existing.
    _child: Option<tokio::process::Child>,
}

#[derive(Default)]
pub struct MlxManager {
    current: Mutex<Option<Running>>,
}

/// Progress callback used while waiting for the model to load/download.
pub type StatusFn<'a> = dyn Fn(String) + Send + Sync + 'a;

impl MlxManager {
    pub fn new() -> Arc<Self> {
        Arc::new(MlxManager::default())
    }

    /// Ensure a server is serving `model`, returning a ready endpoint.
    pub async fn ensure(
        &self,
        model: &str,
        forced_port: u16,
        status: &StatusFn<'_>,
    ) -> anyhow::Result<Endpoint> {
        let mut guard = self.current.lock().await;

        // 1. Reuse our own previously-started server if it matches.
        if let Some(r) = guard.as_ref() {
            if model_matches(&r.requested, model) && probe_ready(r.port).await.is_some() {
                return Ok(Endpoint {
                    base_url: base(r.port),
                    model_id: r.model_id.clone(),
                });
            }
        }

        // 2. Reuse an externally-running rapid-mlx server serving this model.
        if let Some((port, _m)) = find_running(model).await {
            if let Some(model_id) = probe_ready(port).await {
                status(format!("Reusing running model server on :{port}"));
                *guard = Some(Running {
                    port,
                    requested: model.to_string(),
                    model_id: model_id.clone(),
                    _child: None,
                });
                return Ok(Endpoint {
                    base_url: base(port),
                    model_id,
                });
            }
        }

        // 3. Start a new server on a free (or forced) port.
        let port = if forced_port != 0 {
            forced_port
        } else {
            free_port()?
        };
        status(format!("Starting model server (loading {model})…"));
        let child = spawn_server(model, port).await?;

        // 4. Wait for readiness (first run may download several GB).
        let model_id = wait_ready(port, status)
            .await
            .context("model server did not become ready")?;

        *guard = Some(Running {
            port,
            requested: model.to_string(),
            model_id: model_id.clone(),
            _child: Some(child),
        });
        Ok(Endpoint {
            base_url: base(port),
            model_id,
        })
    }
}

fn base(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

/// A model present in the local HuggingFace cache.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CachedModel {
    pub repo: String,
    pub alias: Option<String>,
    pub size: String,
    /// Heuristic: multimodal (vision) models can do the visual overview.
    pub multimodal: bool,
}

/// Parse `rapid-mlx ls` into structured cached models.
pub async fn list_cached() -> Vec<CachedModel> {
    let Ok(bin) = tools::rapid_mlx() else {
        return Vec::new();
    };
    let Ok(out) = tokio::process::Command::new(bin).arg("ls").output().await else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut models = Vec::new();
    for line in text.lines() {
        // Data rows contain a HF repo with a '/'.
        let cols: Vec<&str> = line.split_whitespace().collect();
        let Some(repo) = cols.iter().find(|c| c.contains('/') && !c.contains("://")) else {
            continue;
        };
        let repo = repo.to_string();
        // Alias is the first column unless it's the "(unmapped)" placeholder.
        let alias = cols.first().and_then(|a| {
            if *a == "(unmapped)" || a.contains('/') {
                None
            } else {
                Some(a.to_string())
            }
        });
        // Size like "12.6 GiB" — grab the token before a unit.
        let size = cols
            .windows(2)
            .find(|w| matches!(w[1], "GiB" | "MiB" | "KiB" | "B"))
            .map(|w| format!("{} {}", w[0], w[1]))
            .unwrap_or_default();
        let low = repo.to_ascii_lowercase();
        // Gemma 3+/4, llava, qwen-vl etc. accept images.
        let multimodal = low.contains("gemma-4")
            || low.contains("gemma-3")
            || low.contains("vl")
            || low.contains("llava")
            || low.contains("vision");
        models.push(CachedModel {
            repo,
            alias,
            size,
            multimodal,
        });
    }
    models
}

/// Loose match between a requested model and a `ps`/serve MODEL string.
fn model_matches(a: &str, b: &str) -> bool {
    let norm = |s: &str| {
        s.rsplit('/')
            .next()
            .unwrap_or(s)
            .to_ascii_lowercase()
            .replace(['_', ' '], "-")
    };
    let (a, b) = (norm(a), norm(b));
    a == b || a.contains(&b) || b.contains(&a)
}

/// Probe `/v1/models`; returns the served model id if ready.
async fn probe_ready(port: u16) -> Option<String> {
    let url = format!("{}/v1/models", base(port));
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .timeout(Duration::from_secs(2))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let v: serde_json::Value = resp.json().await.ok()?;
    v.get("data")
        .and_then(|d| d.as_array())
        .and_then(|a| a.first())
        .and_then(|m| m.get("id"))
        .and_then(|id| id.as_str())
        .map(|s| s.to_string())
}

/// Parse `rapid-mlx ps` for a server serving the requested model. Returns (port, model).
async fn find_running(model: &str) -> Option<(u16, String)> {
    let bin = tools::rapid_mlx().ok()?;
    let out = tokio::process::Command::new(bin)
        .arg("ps")
        .output()
        .await
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        // Columns: PID PORT MODEL UPTIME
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 3 {
            continue;
        }
        let Ok(port) = cols[1].parse::<u16>() else {
            continue;
        };
        let m = cols[2];
        if model_matches(m, model) {
            return Some((port, m.to_string()));
        }
    }
    None
}

fn free_port() -> anyhow::Result<u16> {
    let l = std::net::TcpListener::bind(("127.0.0.1", 0)).context("bind free port")?;
    Ok(l.local_addr()?.port())
}

/// Heuristic: does this model id name a vision-capable architecture?
pub fn is_multimodal(model: &str) -> bool {
    let low = model.to_ascii_lowercase();
    low.contains("gemma-4")
        || low.contains("gemma-3")
        || low.contains("vl")
        || low.contains("llava")
        || low.contains("vision")
        || low.contains("mllm")
}

async fn spawn_server(model: &str, port: u16) -> anyhow::Result<tokio::process::Child> {
    let bin = tools::rapid_mlx()?;
    let mut cmd = tokio::process::Command::new(bin);
    cmd.arg("serve")
        .arg(model)
        .arg("--port")
        .arg(port.to_string())
        .arg("--host")
        .arg("127.0.0.1");
    // Force the multimodal/VLM path for vision-capable models (rapid-mlx otherwise
    // routes Gemma 4 to its text-only loader, which rejects image inputs).
    if is_multimodal(model) {
        cmd.arg("--mllm");
    }
    let child = cmd
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(false)
        .spawn()
        .map_err(|e| anyhow!("failed to start rapid-mlx serve: {e}"))?;
    Ok(child)
}

/// Poll until the server reports a model, up to a generous timeout for first-run downloads.
async fn wait_ready(port: u16, status: &StatusFn<'_>) -> anyhow::Result<String> {
    let max = Duration::from_secs(60 * 30); // 30 min budget for big downloads
    let start = std::time::Instant::now();
    let mut tick = 0u64;
    loop {
        if let Some(id) = probe_ready(port).await {
            return Ok(id);
        }
        if start.elapsed() > max {
            return Err(anyhow!("timed out after {:?}", start.elapsed()));
        }
        tick += 1;
        if tick % 3 == 0 {
            status(format!(
                "Loading model… ({}s)",
                start.elapsed().as_secs()
            ));
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}
