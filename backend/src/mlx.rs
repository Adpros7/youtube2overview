//! rapid-mlx server *pool* orchestration.
//!
//! Maintains a pool of rapid-mlx servers — each its own process on its own port, all
//! serving the same model. Concurrent jobs are load-balanced across the pool (least
//! in-flight wins) so they don't all serialize behind one server.
//!
//! IMPORTANT: every server shares the one Apple Silicon GPU, so a pool is **not** a
//! linear speedup. It overlaps host-side gaps (HTTP, base64, JSON, subprocess spin-up)
//! and lets one job's GPU-free stages run while another job holds the GPU. Throughput is
//! still ultimately GPU-bound. See the project memory note on this.
//!
//! The pool is started lazily on first use, up to `target` servers, and persists for the
//! lifetime of the process so the (one-time) model load is amortized across all jobs.
//! Crucially, the model load is awaited *without* holding any lock, so a second job is
//! never blocked behind the first job's load.

use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context};
use tokio::sync::Mutex;

use crate::tools;

/// A resolved, ready-to-use OpenAI-compatible endpoint.
#[derive(Clone, Debug)]
pub struct Endpoint {
    pub base_url: String,
    pub model_id: String,
}

/// A pooled rapid-mlx server.
struct Server {
    port: u16,
    /// What we asked rapid-mlx to serve (repo id or alias).
    requested: String,
    /// `Some(id)` once the server has reported a served model (i.e. is ready).
    model_id: StdMutex<Option<String>>,
    /// Number of jobs currently using this server, for least-loaded selection.
    inflight: AtomicUsize,
    /// `Some` if we started it (kept alive so the process isn't reaped); `None` if adopted.
    _child: Option<tokio::process::Child>,
}

impl Server {
    fn ready_model_id(&self) -> Option<String> {
        self.model_id.lock().unwrap().clone()
    }
}

/// A handle to a pooled server held for the duration of a job. Releases its in-flight
/// slot on drop so the next job can prefer a less-busy server.
pub struct Lease {
    server: Arc<Server>,
}

impl Lease {
    pub fn endpoint(&self) -> Endpoint {
        Endpoint {
            base_url: base(self.server.port),
            model_id: self.server.ready_model_id().unwrap_or_default(),
        }
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        self.server.inflight.fetch_sub(1, Ordering::SeqCst);
    }
}

#[derive(Default)]
pub struct MlxManager {
    servers: Mutex<Vec<Arc<Server>>>,
    /// Serializes pool growth so concurrent jobs cooperate instead of racing to start
    /// duplicate servers. Held only while *launching* processes — never across a load.
    grow: Mutex<()>,
}

/// Progress callback used while waiting for the model to load/download.
pub type StatusFn<'a> = dyn Fn(String) + Send + Sync + 'a;

/// Overall budget for a server to become ready (first run may download several GB).
const READY_BUDGET: Duration = Duration::from_secs(60 * 30);

impl MlxManager {
    pub fn new() -> Arc<Self> {
        Arc::new(MlxManager::default())
    }

    /// Acquire a ready server serving `model`, returning a [`Lease`].
    ///
    /// `forced_port != 0` pins the pool to a single server on that port. Otherwise the
    /// pool is grown to `target_servers` (min 1) on free ports.
    pub async fn acquire(
        &self,
        model: &str,
        forced_port: u16,
        target_servers: u16,
        status: &StatusFn<'_>,
    ) -> anyhow::Result<Lease> {
        let target = if forced_port != 0 {
            1
        } else {
            target_servers.max(1) as usize
        };

        // Ensure the pool has `target` servers for this model (launches processes; does
        // not wait for them to load).
        self.ensure_pool(model, forced_port, target, status).await?;

        // Wait until at least one server is ready, then take the least-loaded one.
        let deadline = Instant::now() + READY_BUDGET;
        let mut announced = false;
        loop {
            self.refresh_ready(model).await;
            if let Some(lease) = self.pick_ready(model).await {
                return Ok(lease);
            }
            if Instant::now() > deadline {
                return Err(anyhow!(
                    "model server did not become ready within {:?}",
                    READY_BUDGET
                ));
            }
            if !announced {
                status(format!("Loading model ({model})…"));
                announced = true;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }

    /// Launch servers until the pool holds `target` for this model. Fast: only spawns
    /// processes (and optionally adopts an externally-running server); never awaits a load.
    async fn ensure_pool(
        &self,
        model: &str,
        forced_port: u16,
        target: usize,
        status: &StatusFn<'_>,
    ) -> anyhow::Result<()> {
        let _g = self.grow.lock().await;

        let have = {
            let servers = self.servers.lock().await;
            servers
                .iter()
                .filter(|s| model_matches(&s.requested, model))
                .count()
        };
        let need = target.saturating_sub(have);
        if need == 0 {
            return Ok(());
        }

        let mut fresh: Vec<Server> = Vec::new();

        // Prefer adopting an already-running rapid-mlx server (e.g. one a developer
        // started by hand) as a pool member, unless a specific port was forced.
        if forced_port == 0 {
            if let Some((port, _)) = find_running(model).await {
                status(format!("Reusing running model server on :{port}"));
                fresh.push(Server::adopted(port, model));
            }
        }

        while fresh.len() < need {
            let port = if forced_port != 0 {
                forced_port
            } else {
                free_port()?
            };
            status(format!("Starting model server on :{port} ({model})…"));
            let child = spawn_server(model, port).await?;
            fresh.push(Server::started(port, model, child));
            if forced_port != 0 {
                break; // a forced port can only host one server
            }
        }

        // Commit, skipping any port we somehow already track.
        let mut servers = self.servers.lock().await;
        for s in fresh {
            if !servers.iter().any(|e| e.port == s.port) {
                servers.push(Arc::new(s));
            }
        }
        Ok(())
    }

    /// Probe every not-yet-ready server for this model and record its served model id.
    async fn refresh_ready(&self, model: &str) {
        let pending: Vec<Arc<Server>> = {
            let servers = self.servers.lock().await;
            servers
                .iter()
                .filter(|s| model_matches(&s.requested, model) && s.ready_model_id().is_none())
                .cloned()
                .collect()
        };
        for s in pending {
            if let Some(id) = probe_ready(s.port).await {
                *s.model_id.lock().unwrap() = Some(id);
            }
        }
    }

    /// Pick the ready server for `model` with the fewest in-flight jobs, reserving a slot.
    async fn pick_ready(&self, model: &str) -> Option<Lease> {
        let servers = self.servers.lock().await;
        let chosen = servers
            .iter()
            .filter(|s| model_matches(&s.requested, model) && s.ready_model_id().is_some())
            .min_by_key(|s| s.inflight.load(Ordering::SeqCst))?;
        chosen.inflight.fetch_add(1, Ordering::SeqCst);
        Some(Lease {
            server: chosen.clone(),
        })
    }
}

impl Server {
    fn started(port: u16, model: &str, child: tokio::process::Child) -> Self {
        Server {
            port,
            requested: model.to_string(),
            model_id: StdMutex::new(None),
            inflight: AtomicUsize::new(0),
            _child: Some(child),
        }
    }
    fn adopted(port: u16, model: &str) -> Self {
        Server {
            port,
            requested: model.to_string(),
            model_id: StdMutex::new(None),
            inflight: AtomicUsize::new(0),
            _child: None,
        }
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
