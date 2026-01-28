//! FastTeX Build Orchestrator
//!
//! This is the main entry point for the build orchestration service.
//! It handles compilation job scheduling, dependency analysis, and
//! worker coordination using MPI-style scatter/gather.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    extract::State,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;

use fasttex_orchestrator::{
    JobScheduler,
    scheduler::SchedulerConfig,
};

/// Application state shared across handlers
struct AppState {
    scheduler: JobScheduler,
}

/// Health check response
#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
}

/// Compile request body
#[derive(Deserialize)]
struct CompileRequest {
    project_id: String,
    target_file: String,
    mode: Option<String>,
}

/// Compile response
#[derive(Serialize)]
struct CompileResponse {
    job_id: String,
    status: String,
    message: String,
}

/// Stats response
#[derive(Serialize)]
struct StatsResponse {
    total_jobs: usize,
    pending_jobs: usize,
    running_jobs: usize,
    completed_jobs: usize,
    failed_jobs: usize,
    available_workers: usize,
    total_workers: usize,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "fasttex_orchestrator=info,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Parse configuration from environment
    let bind_addr: SocketAddr = std::env::var("ORCHESTRATOR_BIND_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8081".to_string())
        .parse()
        .expect("Invalid ORCHESTRATOR_BIND_ADDR");

    let max_concurrent_jobs: usize = std::env::var("ORCHESTRATOR_MAX_JOBS")
        .unwrap_or_else(|_| "16".to_string())
        .parse()
        .unwrap_or(16);

    let scheduler_config = SchedulerConfig {
        max_concurrent_jobs,
        ..Default::default()
    };

    let state = Arc::new(AppState {
        scheduler: JobScheduler::new(scheduler_config),
    });

    // Build the router
    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/api/compile", post(compile_handler))
        .route("/api/stats", get(stats_handler))
        .with_state(state);

    tracing::info!("Starting FastTeX Orchestrator on {}", bind_addr);

    // Start the server
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Health check endpoint
async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy",
        version: env!("CARGO_PKG_VERSION"),
    })
}

/// Submit a compilation request
async fn compile_handler(
    State(state): State<Arc<AppState>>,
    Json(request): Json<CompileRequest>,
) -> Json<CompileResponse> {
    tracing::info!(
        "Compile request received for project {} (target: {})",
        request.project_id,
        request.target_file
    );

    // In production, this would:
    // 1. Build the dependency graph
    // 2. Schedule jobs
    // 3. Return the job ID for status polling

    let job_id = Uuid::new_v4();

    Json(CompileResponse {
        job_id: job_id.to_string(),
        status: "queued".to_string(),
        message: format!(
            "Compilation queued for {} with mode {}",
            request.target_file,
            request.mode.unwrap_or_else(|| "full".to_string())
        ),
    })
}

/// Get scheduler statistics
async fn stats_handler(State(state): State<Arc<AppState>>) -> Json<StatsResponse> {
    let stats = state.scheduler.stats().await;

    Json(StatsResponse {
        total_jobs: stats.total_jobs,
        pending_jobs: stats.pending_jobs,
        running_jobs: stats.running_jobs,
        completed_jobs: stats.completed_jobs,
        failed_jobs: stats.failed_jobs,
        available_workers: stats.available_workers,
        total_workers: stats.total_workers,
    })
}
