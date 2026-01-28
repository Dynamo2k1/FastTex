//! FastTeX Gateway Server
//!
//! This is the main entry point for the WebSocket gateway server.
//! It handles real-time collaboration via Yjs synchronization and
//! compile event streaming.

use std::net::SocketAddr;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use fasttex_gateway::{GatewayServer, server::GatewayConfig};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "fasttex_gateway=info,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Parse configuration from environment
    let bind_addr: SocketAddr = std::env::var("GATEWAY_BIND_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8080".to_string())
        .parse()
        .expect("Invalid GATEWAY_BIND_ADDR");

    let max_connections: usize = std::env::var("GATEWAY_MAX_CONNECTIONS")
        .unwrap_or_else(|_| "100".to_string())
        .parse()
        .unwrap_or(100);

    let config = GatewayConfig {
        bind_addr,
        max_connections_per_project: max_connections,
        heartbeat_interval: 30,
        connection_timeout: 300,
    };

    tracing::info!(
        "Starting FastTeX Gateway on {} (max {} connections/project)",
        config.bind_addr,
        config.max_connections_per_project
    );

    // Create and run the server
    let server = GatewayServer::new(config);
    server.run().await?;

    Ok(())
}
