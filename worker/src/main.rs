//! FastTeX Compilation Worker
//!
//! This is the main entry point for the compilation worker.
//! In production, this runs inside a Firecracker microVM and executes
//! Tectonic-based LaTeX compilation.

use std::path::PathBuf;
use std::time::Duration;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;

use fasttex_worker::{
    TectonicCompiler,
    compiler::CompileOptions,
    protocol::{WorkerRequest, RequestType},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "fasttex_worker=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let worker_id = Uuid::new_v4();
    tracing::info!("Starting FastTeX Worker {}", worker_id);

    // Get working directory from environment or use temp dir
    let work_dir: PathBuf = std::env::var("WORKER_WORK_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("fasttex-worker"));

    // Create working directory if it doesn't exist
    std::fs::create_dir_all(&work_dir)?;
    tracing::info!("Working directory: {:?}", work_dir);

    // Create compiler instance
    let compiler = TectonicCompiler::new(work_dir.clone());

    // In production, the worker would:
    // 1. Connect to the orchestrator via gRPC
    // 2. Wait for compilation jobs
    // 3. Execute compilations
    // 4. Return results

    // For development, demonstrate a simple compilation
    demo_compile(&compiler, &work_dir).await?;

    tracing::info!("Worker {} shutting down", worker_id);
    Ok(())
}

/// Demonstrate compilation capability
async fn demo_compile(compiler: &TectonicCompiler, work_dir: &PathBuf) -> anyhow::Result<()> {
    tracing::info!("Running demonstration compilation...");

    // Create a sample LaTeX document
    let sample_tex = r#"
\documentclass{article}
\begin{document}
\title{FastTeX Demo}
\author{FastTeX Worker}
\maketitle

\section{Introduction}
This is a demonstration document compiled by the FastTeX worker.

\section{Features}
\begin{itemize}
    \item Parallel compilation
    \item Preamble caching
    \item Real-time collaboration
\end{itemize}

\end{document}
"#;

    let input_path = work_dir.join("demo.tex");
    let output_dir = work_dir.join("output");

    std::fs::write(&input_path, sample_tex)?;
    std::fs::create_dir_all(&output_dir)?;

    tracing::info!("Compiling demo document...");

    let options = CompileOptions {
        input_path: PathBuf::from("demo.tex"),
        output_dir: output_dir.clone(),
        timeout: Duration::from_secs(60),
        ..Default::default()
    };

    let result = compiler.compile(&options)?;

    if result.success {
        tracing::info!(
            "Compilation successful! PDF: {:?} (took {:?})",
            result.pdf_path,
            result.duration
        );

        if !result.diagnostics.is_empty() {
            tracing::warn!("Diagnostics:");
            for diag in &result.diagnostics {
                tracing::warn!("  {:?}: {}", diag.severity, diag.message);
            }
        }
    } else {
        tracing::error!("Compilation failed!");
        for diag in &result.diagnostics {
            tracing::error!("  {:?}: {}", diag.severity, diag.message);
        }
    }

    Ok(())
}
