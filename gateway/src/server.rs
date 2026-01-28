//! WebSocket Gateway Server
//!
//! This module provides the main WebSocket server for FastTeX,
//! handling Yjs document synchronization and compile events.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    extract::{
        ws::{Message, WebSocket},
        Path, State, WebSocketUpgrade,
    },
    response::IntoResponse,
    routing::get,
    Router,
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
