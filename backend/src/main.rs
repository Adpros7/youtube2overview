//! yt2overview backend entrypoint.
//!
//! Binds an ephemeral localhost port (chosen by the OS), prints the chosen URL to
//! stdout as `YT2O_LISTENING http://127.0.0.1:<port>` so the SwiftUI host can read it,
//! then serves the HTTP API.

mod api;
mod config;
mod error;
mod mlx;
mod model;
mod pipeline;
mod state;
mod tools;

use std::net::{Ipv4Addr, SocketAddr};

use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_env("YT2O_LOG").unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    // Honour an explicit port if provided, else let the OS pick a free one.
    let requested_port: u16 = std::env::var("YT2O_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(0);

    let state = state::AppState::new();
    let app = api::router(state);

    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, requested_port));
    let listener = TcpListener::bind(addr).await?;
    let local = listener.local_addr()?;

    // Contract with the host app: this exact line announces readiness.
    println!("YT2O_LISTENING http://127.0.0.1:{}", local.port());
    use std::io::Write;
    let _ = std::io::stdout().flush();

    tracing::info!("yt2overview backend listening on {local}");
    axum::serve(listener, app).await?;
    Ok(())
}
