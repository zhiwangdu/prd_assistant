mod app;
mod domain;
mod http;
mod mcp_server;
mod pipeline;
mod services;
mod stores;
mod support;

use std::{net::SocketAddr, path::PathBuf};

use anyhow::Context;
use axum::{http::HeaderValue, Router};
use clap::{Parser, Subcommand};
use tokio::net::TcpListener;
use tower_http::{
    cors::{Any, CorsLayer},
    services::ServeDir,
    trace::{DefaultMakeSpan, DefaultOnFailure, DefaultOnResponse, TraceLayer},
};
use tracing::{info, Level};
use tracing_subscriber::EnvFilter;

use crate::{app::AppState, support::config::load_config};

#[derive(Parser, Debug)]
#[command(author, version, about = "LocalToolHub server")]
struct Args {
    #[arg(long, global = true, default_value = "logagent.yaml")]
    config: PathBuf,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Standalone task-free MCP stdio server for external clients.
    McpServe,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        // MCP mode speaks JSON-RPC on stdout, so runtime logs must always go to stderr.
        .with_writer(std::io::stderr)
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("logagent_server=info,tower_http=info")),
        )
        .init();

    let args = Args::parse();
    info!(config = %args.config.display(), "loading server config");
    let config = load_config(&args.config).context("failed to load server config")?;
    config.prepare_dirs()?;

    match args.command {
        Some(Command::McpServe) => return mcp_server::run_stdio(config, Some(args.config)).await,
        None => {}
    }

    let bind: SocketAddr = config
        .server
        .bind
        .parse()
        .with_context(|| format!("invalid bind address '{}'", config.server.bind))?;

    let state = AppState::new_with_config_path(config, Some(args.config))?;
    state.recover_tasks().await?;
    let app = Router::new()
        .merge(http::router(state.clone()))
        .fallback_service(ServeDir::new("webui/out").append_index_html_on_directories(true))
        .layer(cors_layer(&state.config.mcp.allowed_origins))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO))
                .on_failure(DefaultOnFailure::new().level(Level::ERROR)),
        )
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

/// CORS layer. When `mcp.allowed_origins` is empty, origins are unrestricted
/// (localhost dev / SSH-tunnel use). When populated (direct remote MCP exposure),
/// CORS is tightened to only those origins so browser-based MCP clients are scoped.
fn cors_layer(allowed_origins: &[String]) -> CorsLayer {
    let cors = CorsLayer::new().allow_methods(Any).allow_headers(Any);
    if allowed_origins.is_empty() {
        cors.allow_origin(Any)
    } else {
        let origins: Vec<HeaderValue> = allowed_origins
            .iter()
            .filter_map(|origin| HeaderValue::from_str(origin).ok())
            .collect();
        cors.allow_origin(origins)
    }
}
