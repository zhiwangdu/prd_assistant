mod app;
mod domain;
mod http;
mod mcp;
mod pipeline;
mod services;
mod stores;
mod support;

use std::{net::SocketAddr, path::PathBuf};

use anyhow::Context;
use axum::Router;
use clap::{Parser, Subcommand};
use tokio::net::TcpListener;
use tower_http::{
    cors::{Any, CorsLayer},
    services::ServeDir,
    trace::TraceLayer,
};
use tracing::info;

use crate::{
    app::AppState,
    support::config::{load_config, AnalysisMode},
};

#[derive(Parser, Debug)]
#[command(author, version, about = "LogAgent MVP server")]
struct Args {
    #[arg(long, global = true, default_value = "logagent.yaml")]
    config: PathBuf,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    Mcp(McpArgs),
}

#[derive(Parser, Debug)]
struct McpArgs {
    #[arg(long)]
    task_id: String,
    #[arg(long, default_value = "diagnose")]
    mode: AnalysisMode,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();
    let config = load_config(&args.config).context("failed to load server config")?;
    config.prepare_dirs()?;

    if let Some(Command::Mcp(mcp_args)) = args.command {
        return mcp::run_stdio(config, mcp_args.task_id, mcp_args.mode).await;
    }

    let bind: SocketAddr = config
        .server
        .bind
        .parse()
        .with_context(|| format!("invalid bind address '{}'", config.server.bind))?;

    let state = AppState::new(config)?;
    state.recover_tasks().await?;
    let app = Router::new()
        .merge(http::router(state.clone()))
        .fallback_service(ServeDir::new("webui/out").append_index_html_on_directories(true))
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
