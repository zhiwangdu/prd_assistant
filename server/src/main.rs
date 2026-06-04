mod api;
mod auth;
mod config;
mod error;
mod fs_utils;
mod id;
mod log_analyzer;
mod metadata;
mod models;
mod pipeline;
mod state;

use std::{net::SocketAddr, path::PathBuf};

use anyhow::Context;
use axum::Router;
use clap::Parser;
use tokio::net::TcpListener;
use tower_http::{
    cors::{Any, CorsLayer},
    services::ServeDir,
    trace::TraceLayer,
};
use tracing::info;

use crate::{config::load_config, state::AppState};

#[derive(Parser, Debug)]
#[command(author, version, about = "LogAgent MVP server")]
struct Args {
    #[arg(long, default_value = "logagent.yaml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();
    let config = load_config(&args.config).context("failed to load server config")?;
    config.prepare_dirs()?;

    let bind: SocketAddr = config
        .server
        .bind
        .parse()
        .with_context(|| format!("invalid bind address '{}'", config.server.bind))?;

    let state = AppState::new(config);
    let app = Router::new()
        .merge(api::router(state.clone()))
        .fallback_service(ServeDir::new("webui").append_index_html_on_directories(true))
        .layer(cors_layer())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = TcpListener::bind(bind).await?;
    info!("server listening on http://{}", bind);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

fn cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
}
