//! FastTeX Realtime Gateway
//!
//! This module implements the WebSocket gateway for real-time collaboration
//! using Yjs CRDT synchronization and compile event streaming.

pub mod server;
pub mod sync;
pub mod presence;

pub use server::GatewayServer;
pub use sync::YjsRelay;
pub use presence::PresenceManager;
