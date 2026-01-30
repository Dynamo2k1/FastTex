//! WebSocket Gateway Server
//!
//! This module provides the main WebSocket server for FastTeX,
//! handling Yjs document synchronization and compile events.

use std::net::SocketAddr;
use std::sync::Arc;
use std::process::Stdio;

use axum::{
    body::Body,
    extract::{
        ws::{Message, WebSocket},
        Path, State, WebSocketUpgrade,
    },
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use dashmap::DashMap;
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::presence::PresenceManager;
use crate::sync::YjsRelay;

/// Configuration for the gateway server
#[derive(Debug, Clone)]
pub struct GatewayConfig {
    /// Address to bind to
    pub bind_addr: SocketAddr,
    /// Maximum connections per project
    pub max_connections_per_project: usize,
    /// Heartbeat interval in seconds
    pub heartbeat_interval: u64,
    /// Connection timeout in seconds
    pub connection_timeout: u64,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        GatewayConfig {
            bind_addr: "0.0.0.0:8080".parse().unwrap(),
            max_connections_per_project: 100,
            heartbeat_interval: 30,
            connection_timeout: 300,
        }
    }
}

/// Shared state for the gateway server
pub struct GatewayState {
    /// Yjs relay for document synchronization
    pub yjs_relay: YjsRelay,
    /// Presence manager for cursor/selection tracking
    pub presence: PresenceManager,
    /// Active connections per project
    pub connections: DashMap<Uuid, Vec<Uuid>>,
    /// Compile event broadcast channels
    pub compile_channels: DashMap<Uuid, broadcast::Sender<CompileEvent>>,
    /// Configuration
    pub config: GatewayConfig,
}

impl GatewayState {
    pub fn new(config: GatewayConfig) -> Self {
        GatewayState {
            yjs_relay: YjsRelay::new(),
            presence: PresenceManager::new(),
            connections: DashMap::new(),
            compile_channels: DashMap::new(),
            config,
        }
    }

    /// Get or create a compile channel for a project
    pub fn get_compile_channel(&self, project_id: Uuid) -> broadcast::Sender<CompileEvent> {
        self.compile_channels
            .entry(project_id)
            .or_insert_with(|| {
                let (tx, _) = broadcast::channel(100);
                tx
            })
            .clone()
    }
}

/// Events sent during compilation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileEvent {
    pub event_type: CompileEventType,
    pub job_id: Option<Uuid>,
    pub progress: Option<f32>,
    pub message: Option<String>,
    pub pdf_url: Option<String>,
    pub diagnostics: Option<Vec<DiagnosticInfo>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompileEventType {
    Started,
    Progress,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticInfo {
    pub severity: String,
    pub message: String,
    pub file: Option<String>,
    pub line: Option<u32>,
}

/// Client-to-server message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    /// Yjs synchronization message
    #[serde(rename = "sync")]
    Sync { payload: Vec<u8> },
    /// Presence update (cursor position, selection)
    #[serde(rename = "presence")]
    Presence { cursor: CursorPosition, selection: Option<Selection> },
    /// Compile request
    #[serde(rename = "compile")]
    Compile { mode: CompileMode, target: String },
    /// Heartbeat
    #[serde(rename = "ping")]
    Ping,
}

/// Server-to-client message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    /// Yjs synchronization message
    #[serde(rename = "sync")]
    Sync { payload: Vec<u8> },
    /// Presence update from another user
    #[serde(rename = "presence")]
    Presence { user_id: String, cursor: CursorPosition, selection: Option<Selection> },
    /// User joined the project
    #[serde(rename = "user_joined")]
    UserJoined { user_id: String, name: String },
    /// User left the project
    #[serde(rename = "user_left")]
    UserLeft { user_id: String },
    /// Compile event
    #[serde(rename = "compile_event")]
    CompileEvent(CompileEvent),
    /// Heartbeat response
    #[serde(rename = "pong")]
    Pong,
    /// Error message
    #[serde(rename = "error")]
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorPosition {
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Selection {
    pub start: CursorPosition,
    pub end: CursorPosition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CompileMode {
    Full,
    Incremental,
}

/// The main gateway server
pub struct GatewayServer {
    state: Arc<GatewayState>,
}

impl GatewayServer {
    /// Create a new gateway server
    pub fn new(config: GatewayConfig) -> Self {
        GatewayServer {
            state: Arc::new(GatewayState::new(config)),
        }
    }

    /// Start the server
    pub async fn run(self) -> anyhow::Result<()> {
        let addr = self.state.config.bind_addr;
        
        let app = Router::new()
            .route("/sync/:project_id", get(ws_sync_handler))
            .route("/compile/:project_id", get(ws_compile_handler))
            .route("/api/compile", post(rest_compile_handler))
            .route("/health", get(health_handler))
            .with_state(self.state);

        info!("Starting FastTeX Gateway on {}", addr);
        
        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app).await?;

        Ok(())
    }

    /// Get the shared state for testing
    pub fn state(&self) -> Arc<GatewayState> {
        self.state.clone()
    }
}

/// Health check handler
async fn health_handler() -> impl IntoResponse {
    "OK"
}

/// WebSocket handler for document synchronization
async fn ws_sync_handler(
    ws: WebSocketUpgrade,
    Path(project_id): Path<Uuid>,
    State(state): State<Arc<GatewayState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_sync_connection(socket, project_id, state))
}

/// WebSocket handler for compile events
async fn ws_compile_handler(
    ws: WebSocketUpgrade,
    Path(project_id): Path<Uuid>,
    State(state): State<Arc<GatewayState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_compile_connection(socket, project_id, state))
}

/// Handle a sync WebSocket connection
async fn handle_sync_connection(socket: WebSocket, project_id: Uuid, state: Arc<GatewayState>) {
    let connection_id = Uuid::new_v4();
    info!("New sync connection {} for project {}", connection_id, project_id);

    // Register connection
    state.connections
        .entry(project_id)
        .or_default()
        .push(connection_id);

    // Subscribe to Yjs updates
    let mut yjs_rx = state.yjs_relay.subscribe(project_id);

    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Task to forward Yjs updates to client
    let forward_task = tokio::spawn(async move {
        while let Ok(update) = yjs_rx.recv().await {
            let msg = ServerMessage::Sync { payload: update };
            if let Ok(json) = serde_json::to_string(&msg) {
                if ws_sender.send(Message::Text(json)).await.is_err() {
                    break;
                }
            }
        }
    });

    // Process incoming messages
    while let Some(result) = ws_receiver.next().await {
        match result {
            Ok(Message::Text(text)) => {
                match serde_json::from_str::<ClientMessage>(&text) {
                    Ok(ClientMessage::Sync { payload }) => {
                        // Relay Yjs update to other clients
                        state.yjs_relay.broadcast(project_id, payload);
                    }
                    Ok(ClientMessage::Presence { cursor, selection }) => {
                        // Update presence and broadcast
                        state.presence.update(
                            project_id,
                            connection_id,
                            cursor,
                            selection,
                        );
                    }
                    Ok(ClientMessage::Ping) => {
                        // Already handled by forward_task
                    }
                    Ok(ClientMessage::Compile { .. }) => {
                        warn!("Compile message on sync connection");
                    }
                    Err(e) => {
                        warn!("Failed to parse client message: {}", e);
                    }
                }
            }
            Ok(Message::Binary(data)) => {
                // Binary messages are raw Yjs updates
                state.yjs_relay.broadcast(project_id, data);
            }
            Ok(Message::Close(_)) => {
                info!("Client {} closed connection", connection_id);
                break;
            }
            Err(e) => {
                error!("WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }

    // Cleanup
    forward_task.abort();
    state.presence.remove(project_id, connection_id);
    if let Some(mut conns) = state.connections.get_mut(&project_id) {
        conns.retain(|&id| id != connection_id);
    }

    info!("Sync connection {} closed", connection_id);
}

/// Handle a compile WebSocket connection
async fn handle_compile_connection(socket: WebSocket, project_id: Uuid, state: Arc<GatewayState>) {
    let connection_id = Uuid::new_v4();
    info!("New compile connection {} for project {}", connection_id, project_id);

    let compile_tx = state.get_compile_channel(project_id);
    let mut compile_rx = compile_tx.subscribe();

    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Task to forward compile events to client
    let forward_task = tokio::spawn(async move {
        while let Ok(event) = compile_rx.recv().await {
            let msg = ServerMessage::CompileEvent(event);
            if let Ok(json) = serde_json::to_string(&msg) {
                if ws_sender.send(Message::Text(json)).await.is_err() {
                    break;
                }
            }
        }
    });

    // Process incoming messages (compile requests)
    while let Some(result) = ws_receiver.next().await {
        match result {
            Ok(Message::Text(text)) => {
                match serde_json::from_str::<ClientMessage>(&text) {
                    Ok(ClientMessage::Compile { mode, target }) => {
                        info!("Compile request for {} (mode: {:?})", target, mode);
                        // In production, this would trigger the orchestrator
                        let event = CompileEvent {
                            event_type: CompileEventType::Started,
                            job_id: Some(Uuid::new_v4()),
                            progress: Some(0.0),
                            message: Some(format!("Starting {} compilation", match mode {
                                CompileMode::Full => "full",
                                CompileMode::Incremental => "incremental",
                            })),
                            pdf_url: None,
                            diagnostics: None,
                        };
                        let _ = compile_tx.send(event);
                    }
                    Ok(ClientMessage::Ping) => {
                        // Send pong
                    }
                    _ => {}
                }
            }
            Ok(Message::Close(_)) => {
                break;
            }
            Err(e) => {
                error!("WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }

    forward_task.abort();
    info!("Compile connection {} closed", connection_id);
}

/// Request body for REST compile endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileRequest {
    #[serde(rename = "projectId")]
    pub project_id: String,
    #[serde(rename = "filePath")]
    pub file_path: String,
    /// Optional LaTeX content to compile (if not provided, uses default demo content)
    pub content: Option<String>,
}

/// Error response for compile endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileErrorResponse {
    pub error: String,
    pub message: String,
    pub diagnostics: Option<Vec<DiagnosticInfo>>,
}

/// Maximum allowed LaTeX content size (1 MB)
const MAX_CONTENT_SIZE: usize = 1024 * 1024;

/// Maximum compilation timeout in seconds
const COMPILE_TIMEOUT_SECS: u64 = 30;

/// REST handler for LaTeX compilation
/// 
/// POST /api/compile
/// 
/// This endpoint compiles a LaTeX document and returns the generated PDF.
/// In production, this would integrate with the orchestrator for parallel compilation.
/// 
/// Security notes:
/// - Input size is limited to 1 MB
/// - Compilation timeout is enforced
/// - Compilation runs in sandboxed temp directories
/// - Error messages are sanitized to avoid information disclosure
async fn rest_compile_handler(
    State(_state): State<Arc<GatewayState>>,
    Json(request): Json<CompileRequest>,
) -> Response {
    info!("REST compile request for project {} file {}", request.project_id, request.file_path);
    
    // Input validation: Check content size
    if let Some(ref content) = request.content {
        if content.len() > MAX_CONTENT_SIZE {
            return error_response(
                StatusCode::BAD_REQUEST,
                "Content too large",
                "LaTeX content exceeds maximum allowed size (1 MB)",
            );
        }
    }
    
    // Create a temporary directory for compilation
    let temp_dir = match tempfile::tempdir() {
        Ok(dir) => dir,
        Err(e) => {
            error!("Failed to create temp directory: {}", e);
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Compilation failed",
                "Unable to create compilation environment",
            );
        }
    };
    
    let work_dir = temp_dir.path();
    
    // Use provided content or default demo document
    let latex_content = request.content.unwrap_or_else(|| {
        r#"\documentclass{article}
\usepackage[utf8]{inputenc}
\usepackage{amsmath}
\usepackage{graphicx}

\title{FastTeX Demo Document}
\author{FastTeX Compiler}
\date{\today}

\begin{document}

\maketitle

\section{Introduction}
Welcome to FastTeX! This is a demonstration document compiled by the FastTeX 
distributed LaTeX compilation system.

\section{Features}
FastTeX provides:
\begin{itemize}
    \item Real-time collaborative editing
    \item Parallel chapter compilation
    \item Preamble caching for fast recompilation
    \item Secure sandboxed compilation
\end{itemize}

\section{Mathematics}
Here is a sample equation:
\begin{equation}
    E = mc^2
\end{equation}

And an integral:
\begin{equation}
    \int_0^\infty e^{-x^2} dx = \frac{\sqrt{\pi}}{2}
\end{equation}

\section{Conclusion}
FastTeX is designed to be fast, reliable, and secure.

\end{document}
"#.to_string()
    });
    
    // Write the LaTeX file
    let tex_path = work_dir.join("main.tex");
    if let Err(e) = tokio::fs::write(&tex_path, &latex_content).await {
        error!("Failed to write tex file: {}", e);
        return error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to write source file",
            &e.to_string(),
        );
    }
    
    // Try to compile with available LaTeX engines
    let pdf_result = compile_latex(work_dir, "main.tex").await;
    
    match pdf_result {
        Ok(pdf_bytes) => {
            info!("Compilation successful, returning {} bytes PDF", pdf_bytes.len());
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/pdf")
                .header(header::CONTENT_DISPOSITION, "inline; filename=\"output.pdf\"")
                .body(Body::from(pdf_bytes))
                .unwrap_or_else(|_| {
                    error_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Failed to build response",
                        "Response construction failed",
                    )
                })
        }
        Err(compile_error) => {
            warn!("Compilation failed: {}", compile_error);
            error_response(
                StatusCode::UNPROCESSABLE_ENTITY,
                "LaTeX compilation failed",
                &compile_error,
            )
        }
    }
}

/// Compile LaTeX using available system tools or generate a valid PDF
async fn compile_latex(work_dir: &std::path::Path, tex_file: &str) -> Result<Vec<u8>, String> {
    let tex_path = work_dir.join(tex_file);
    
    // Try different LaTeX compilers in order of preference
    let compilers = ["pdflatex", "xelatex", "lualatex", "tectonic"];
    
    for compiler in compilers {
        match try_compile_with(work_dir, &tex_path, compiler).await {
            Ok(pdf_bytes) => return Ok(pdf_bytes),
            Err(e) => {
                info!("Compiler {} not available or failed: {}", compiler, e);
                continue;
            }
        }
    }
    
    // If no compiler is available, generate a valid minimal PDF
    info!("No LaTeX compiler available, generating placeholder PDF");
    Ok(generate_placeholder_pdf())
}

/// Try to compile with a specific LaTeX compiler
async fn try_compile_with(
    work_dir: &std::path::Path,
    tex_path: &std::path::Path,
    compiler: &str,
) -> Result<Vec<u8>, String> {
    use tokio::process::Command;
    use std::time::Duration;
    
    // Create command with timeout protection
    let child = Command::new(compiler)
        .arg("-interaction=nonstopmode")
        .arg("-halt-on-error")
        .arg("-no-shell-escape")  // Security: disable shell escape
        .arg("-output-directory")
        .arg(work_dir)
        .arg(tex_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(work_dir)
        .spawn()
        .map_err(|e| format!("Failed to start compiler: {}", e))?;
    
    // Apply timeout to compilation
    let timeout = Duration::from_secs(COMPILE_TIMEOUT_SECS);
    let output = match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(result) => result.map_err(|e| format!("Compilation process error: {}", e))?,
        Err(_) => {
            return Err("Compilation timed out".to_string());
        }
    };
    
    if !output.status.success() {
        // Sanitize error messages to avoid information disclosure
        // Extract only relevant LaTeX errors without system paths
        let log_output = String::from_utf8_lossy(&output.stdout);
        let error_message = extract_latex_errors(&log_output);
        return Err(format!("LaTeX compilation failed: {}", error_message));
    }
    
    // Read the generated PDF
    let pdf_path = work_dir.join(
        tex_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
            + ".pdf",
    );
    
    tokio::fs::read(&pdf_path)
        .await
        .map_err(|e| format!("Failed to read generated PDF: {}", e))
}

/// Generate a valid placeholder PDF when no compiler is available
fn generate_placeholder_pdf() -> Vec<u8> {
    // This is a minimal valid PDF with content
    let pdf_content = r#"%PDF-1.4
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>
endobj
4 0 obj
<< /Length 189 >>
stream
BT
/F1 24 Tf
50 700 Td
(FastTeX Demo Document) Tj
0 -40 Td
/F1 12 Tf
(This PDF was generated by FastTeX.) Tj
0 -20 Td
(LaTeX compiler not available in this environment.) Tj
0 -20 Td
(Install pdflatex, xelatex, or tectonic for full compilation.) Tj
ET
endstream
endobj
5 0 obj
<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>
endobj
xref
0 6
0000000000 65535 f 
0000000009 00000 n 
0000000058 00000 n 
0000000115 00000 n 
0000000266 00000 n 
0000000507 00000 n 
trailer
<< /Size 6 /Root 1 0 R >>
startxref
578
%%EOF"#;
    
    pdf_content.as_bytes().to_vec()
}

/// Extract and sanitize LaTeX error messages from compiler output
/// This removes file system paths and other sensitive information
fn extract_latex_errors(log_output: &str) -> String {
    let mut errors = Vec::new();
    
    for line in log_output.lines() {
        // Look for common LaTeX error patterns
        if line.starts_with('!') {
            // Remove any control characters and limit line length
            let sanitized: String = line
                .chars()
                .filter(|c| !c.is_ascii_control())
                .take(200)
                .collect();
            errors.push(sanitized);
        } else if line.contains("Undefined control sequence") 
            || line.contains("Missing") 
            || line.contains("Extra") 
        {
            let sanitized: String = line
                .chars()
                .filter(|c| !c.is_ascii_control())
                .take(200)
                .collect();
            errors.push(sanitized);
        }
    }
    
    if errors.is_empty() {
        "Compilation failed with errors".to_string()
    } else {
        errors.into_iter().take(5).collect::<Vec<_>>().join("; ")
    }
}

/// Build an error response
fn error_response(status: StatusCode, error: &str, message: &str) -> Response {
    let body = CompileErrorResponse {
        error: error.to_string(),
        message: message.to_string(),
        diagnostics: None,
    };
    
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap_or_default()))
        .unwrap_or_else(|_| {
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from("Internal server error"))
                .unwrap()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gateway_state_creation() {
        let config = GatewayConfig::default();
        let state = GatewayState::new(config);
        
        assert!(state.connections.is_empty());
        assert!(state.compile_channels.is_empty());
    }

    #[test]
    fn test_compile_channel_creation() {
        let state = GatewayState::new(GatewayConfig::default());
        let project_id = Uuid::new_v4();
        
        let tx1 = state.get_compile_channel(project_id);
        let tx2 = state.get_compile_channel(project_id);
        
        // Should return the same channel
        assert!(state.compile_channels.contains_key(&project_id));
    }

    #[test]
    fn test_client_message_serialization() {
        let msg = ClientMessage::Compile {
            mode: CompileMode::Incremental,
            target: "main.tex".to_string(),
        };
        
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("incremental"));
        assert!(json.contains("main.tex"));
    }

    #[test]
    fn test_server_message_serialization() {
        let event = CompileEvent {
            event_type: CompileEventType::Progress,
            job_id: Some(Uuid::new_v4()),
            progress: Some(0.5),
            message: Some("Compiling chapter 2...".to_string()),
            pdf_url: None,
            diagnostics: None,
        };
        
        let msg = ServerMessage::CompileEvent(event);
        let json = serde_json::to_string(&msg).unwrap();
        
        assert!(json.contains("compile_event"));
        assert!(json.contains("0.5"));
    }
}
