//! Yjs Document Synchronization Relay
//!
//! This module provides the CRDT synchronization relay for collaborative editing.
//! It relays Yjs updates between connected clients without transforming them.

use dashmap::DashMap;
use tokio::sync::broadcast;
use uuid::Uuid;

/// Relay for Yjs document synchronization
///
/// The relay acts as a broadcast hub for Yjs updates. It does not
/// transform or process the updates - this is handled by Yjs on the client side.
pub struct YjsRelay {
    /// Broadcast channels per project (project_id -> sender)
    channels: DashMap<Uuid, broadcast::Sender<Vec<u8>>>,
    /// Document state snapshots for new client sync
    snapshots: DashMap<Uuid, Vec<u8>>,
}

impl YjsRelay {
    /// Create a new Yjs relay
    pub fn new() -> Self {
        YjsRelay {
            channels: DashMap::new(),
            snapshots: DashMap::new(),
        }
    }

    /// Subscribe to updates for a project
    pub fn subscribe(&self, project_id: Uuid) -> broadcast::Receiver<Vec<u8>> {
        let sender = self.channels
            .entry(project_id)
            .or_insert_with(|| {
                let (tx, _) = broadcast::channel(1000);
                tx
            });
        sender.subscribe()
    }

    /// Broadcast an update to all subscribers of a project
    pub fn broadcast(&self, project_id: Uuid, update: Vec<u8>) {
        if let Some(sender) = self.channels.get(&project_id) {
            // Ignore send errors (no receivers)
            let _ = sender.send(update.clone());
        }
        
        // Store as part of snapshot (in production, merge with existing)
        self.snapshots.insert(project_id, update);
    }

    /// Get the current document snapshot for a project
    /// Used for syncing new clients
    pub fn get_snapshot(&self, project_id: Uuid) -> Option<Vec<u8>> {
        self.snapshots.get(&project_id).map(|v| v.clone())
    }

    /// Store a document snapshot
    pub fn store_snapshot(&self, project_id: Uuid, snapshot: Vec<u8>) {
        self.snapshots.insert(project_id, snapshot);
    }

    /// Get the number of active subscribers for a project
    pub fn subscriber_count(&self, project_id: Uuid) -> usize {
        self.channels
            .get(&project_id)
            .map(|sender| sender.receiver_count())
            .unwrap_or(0)
    }

    /// Remove a project's channel (when no more subscribers)
    pub fn cleanup_project(&self, project_id: Uuid) {
        if let Some(sender) = self.channels.get(&project_id) {
            if sender.receiver_count() == 0 {
                self.channels.remove(&project_id);
            }
        }
    }
}

impl Default for YjsRelay {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relay_creation() {
        let relay = YjsRelay::new();
        assert!(relay.channels.is_empty());
        assert!(relay.snapshots.is_empty());
    }

    #[tokio::test]
    async fn test_subscribe_creates_channel() {
        let relay = YjsRelay::new();
        let project_id = Uuid::new_v4();

        let _rx = relay.subscribe(project_id);
        
        assert!(relay.channels.contains_key(&project_id));
    }

    #[tokio::test]
    async fn test_broadcast_and_receive() {
        let relay = YjsRelay::new();
        let project_id = Uuid::new_v4();

        let mut rx = relay.subscribe(project_id);
        
        let update = vec![1, 2, 3, 4];
        relay.broadcast(project_id, update.clone());

        let received = rx.recv().await.unwrap();
        assert_eq!(received, update);
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let relay = YjsRelay::new();
        let project_id = Uuid::new_v4();

        let mut rx1 = relay.subscribe(project_id);
        let mut rx2 = relay.subscribe(project_id);
        
        let update = vec![5, 6, 7, 8];
        relay.broadcast(project_id, update.clone());

        let received1 = rx1.recv().await.unwrap();
        let received2 = rx2.recv().await.unwrap();
        
        assert_eq!(received1, update);
        assert_eq!(received2, update);
    }

    #[test]
    fn test_snapshot_storage() {
        let relay = YjsRelay::new();
        let project_id = Uuid::new_v4();

        let snapshot = vec![10, 20, 30];
        relay.store_snapshot(project_id, snapshot.clone());

        let retrieved = relay.get_snapshot(project_id).unwrap();
        assert_eq!(retrieved, snapshot);
    }

    #[test]
    fn test_subscriber_count() {
        let relay = YjsRelay::new();
        let project_id = Uuid::new_v4();

        assert_eq!(relay.subscriber_count(project_id), 0);

        let _rx1 = relay.subscribe(project_id);
        assert_eq!(relay.subscriber_count(project_id), 1);

        let _rx2 = relay.subscribe(project_id);
        assert_eq!(relay.subscriber_count(project_id), 2);
    }
}
