//! Presence Management for Collaborative Editing
//!
//! This module tracks user presence (cursor positions, selections) for
//! real-time collaboration awareness.

use std::time::{Duration, Instant};

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::server::{CursorPosition, Selection};

/// Presence information for a single user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPresence {
    /// Connection/user identifier
    pub user_id: Uuid,
    /// Display name
    pub name: String,
    /// Current cursor position
    pub cursor: CursorPosition,
    /// Current selection (if any)
    pub selection: Option<Selection>,
    /// Currently open file
    pub active_file: Option<String>,
    /// User color for cursor highlighting
    pub color: String,
}

/// Internal presence with timestamp (not serialized)
#[derive(Debug, Clone)]
pub struct InternalPresence {
    pub presence: UserPresence,
    pub last_update: Instant,
}

impl UserPresence {
    /// Create new presence for a user
    pub fn new(user_id: Uuid) -> Self {
        UserPresence {
            user_id,
            name: format!("User {}", &user_id.to_string()[..8]),
            cursor: CursorPosition { line: 0, character: 0 },
            selection: None,
            active_file: None,
            color: generate_user_color(user_id),
        }
    }
}

impl InternalPresence {
    /// Create new internal presence for a user
    pub fn new(user_id: Uuid) -> Self {
        InternalPresence {
            presence: UserPresence::new(user_id),
            last_update: Instant::now(),
        }
    }
    
    /// Check if presence is stale
    pub fn is_stale(&self, timeout: Duration) -> bool {
        self.last_update.elapsed() > timeout
    }
}

/// Generate a consistent color for a user based on their ID
fn generate_user_color(user_id: Uuid) -> String {
    // Use bytes from UUID to generate HSL color
    let bytes = user_id.as_bytes();
    let hue = ((bytes[0] as u32 * 256 + bytes[1] as u32) % 360) as u16;
    let saturation = 70;
    let lightness = 50;
    
    format!("hsl({}, {}%, {}%)", hue, saturation, lightness)
}

/// Manager for user presence across projects
pub struct PresenceManager {
    /// Presence data per project (project_id -> (user_id -> presence))
    presence: DashMap<Uuid, DashMap<Uuid, InternalPresence>>,
    /// Presence timeout
    timeout: Duration,
}

impl PresenceManager {
    /// Create a new presence manager
    pub fn new() -> Self {
        PresenceManager {
            presence: DashMap::new(),
            timeout: Duration::from_secs(60),
        }
    }

    /// Create a presence manager with custom timeout
    pub fn with_timeout(timeout: Duration) -> Self {
        PresenceManager {
            presence: DashMap::new(),
            timeout,
        }
    }

    /// Update a user's presence
    pub fn update(
        &self,
        project_id: Uuid,
        user_id: Uuid,
        cursor: CursorPosition,
        selection: Option<Selection>,
    ) {
        let project_presence = self.presence
            .entry(project_id)
            .or_insert_with(DashMap::new);
        
        let mut internal = project_presence
            .entry(user_id)
            .or_insert_with(|| InternalPresence::new(user_id));
        
        internal.presence.cursor = cursor;
        internal.presence.selection = selection;
        internal.last_update = Instant::now();
    }

    /// Update which file a user has open
    pub fn update_active_file(
        &self,
        project_id: Uuid,
        user_id: Uuid,
        file_path: Option<String>,
    ) {
        if let Some(project_presence) = self.presence.get(&project_id) {
            if let Some(mut internal) = project_presence.get_mut(&user_id) {
                internal.presence.active_file = file_path;
                internal.last_update = Instant::now();
            }
        }
    }

    /// Remove a user's presence (when they disconnect)
    pub fn remove(&self, project_id: Uuid, user_id: Uuid) {
        if let Some(project_presence) = self.presence.get(&project_id) {
            project_presence.remove(&user_id);
        }
    }

    /// Get all active users for a project
    pub fn get_project_presence(&self, project_id: Uuid) -> Vec<UserPresence> {
        self.presence
            .get(&project_id)
            .map(|project_presence| {
                project_presence
                    .iter()
                    .filter(|entry| !entry.value().is_stale(self.timeout))
                    .map(|entry| entry.value().presence.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get presence for a specific user
    pub fn get_user_presence(&self, project_id: Uuid, user_id: Uuid) -> Option<UserPresence> {
        self.presence
            .get(&project_id)
            .and_then(|project_presence| {
                project_presence.get(&user_id).map(|p| p.presence.clone())
            })
    }

    /// Get the number of active users in a project
    pub fn user_count(&self, project_id: Uuid) -> usize {
        self.presence
            .get(&project_id)
            .map(|project_presence| {
                project_presence
                    .iter()
                    .filter(|entry| !entry.value().is_stale(self.timeout))
                    .count()
            })
            .unwrap_or(0)
    }

    /// Clean up stale presence entries
    pub fn cleanup_stale(&self) {
        for project in self.presence.iter() {
            let project_presence = project.value();
            let stale_users: Vec<Uuid> = project_presence
                .iter()
                .filter(|entry| entry.value().is_stale(self.timeout))
                .map(|entry| *entry.key())
                .collect();
            
            for user_id in stale_users {
                project_presence.remove(&user_id);
            }
        }
    }

    /// Clean up empty projects
    pub fn cleanup_empty_projects(&self) {
        let empty_projects: Vec<Uuid> = self.presence
            .iter()
            .filter(|entry| entry.value().is_empty())
            .map(|entry| *entry.key())
            .collect();
        
        for project_id in empty_projects {
            self.presence.remove(&project_id);
        }
    }
}

impl Default for PresenceManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_presence_creation() {
        let user_id = Uuid::new_v4();
        let presence = UserPresence::new(user_id);
        
        assert_eq!(presence.user_id, user_id);
        assert!(!presence.color.is_empty());
    }

    #[test]
    fn test_user_color_generation() {
        let user1 = Uuid::new_v4();
        let user2 = Uuid::new_v4();
        
        let color1 = generate_user_color(user1);
        let color2 = generate_user_color(user2);
        
        // Colors should be valid HSL
        assert!(color1.starts_with("hsl("));
        assert!(color2.starts_with("hsl("));
        
        // Same user should get same color
        assert_eq!(color1, generate_user_color(user1));
    }

    #[test]
    fn test_presence_manager() {
        let manager = PresenceManager::new();
        let project_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        // Update presence
        manager.update(
            project_id,
            user_id,
            CursorPosition { line: 10, character: 5 },
            None,
        );

        // Check user count
        assert_eq!(manager.user_count(project_id), 1);

        // Get presence
        let presence = manager.get_user_presence(project_id, user_id).unwrap();
        assert_eq!(presence.cursor.line, 10);
        assert_eq!(presence.cursor.character, 5);
    }

    #[test]
    fn test_presence_removal() {
        let manager = PresenceManager::new();
        let project_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        manager.update(
            project_id,
            user_id,
            CursorPosition { line: 0, character: 0 },
            None,
        );
        assert_eq!(manager.user_count(project_id), 1);

        manager.remove(project_id, user_id);
        assert_eq!(manager.user_count(project_id), 0);
    }

    #[test]
    fn test_multiple_users() {
        let manager = PresenceManager::new();
        let project_id = Uuid::new_v4();

        for _ in 0..5 {
            let user_id = Uuid::new_v4();
            manager.update(
                project_id,
                user_id,
                CursorPosition { line: 0, character: 0 },
                None,
            );
        }

        assert_eq!(manager.user_count(project_id), 5);
        
        let all_presence = manager.get_project_presence(project_id);
        assert_eq!(all_presence.len(), 5);
    }

    #[test]
    fn test_stale_presence() {
        let manager = PresenceManager::with_timeout(Duration::from_millis(10));
        let project_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        manager.update(
            project_id,
            user_id,
            CursorPosition { line: 0, character: 0 },
            None,
        );

        // Wait for timeout
        std::thread::sleep(Duration::from_millis(20));

        // User should be considered stale
        assert_eq!(manager.user_count(project_id), 0);
    }

    #[test]
    fn test_active_file_update() {
        let manager = PresenceManager::new();
        let project_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        manager.update(
            project_id,
            user_id,
            CursorPosition { line: 0, character: 0 },
            None,
        );

        manager.update_active_file(project_id, user_id, Some("chapter1.tex".to_string()));

        let presence = manager.get_user_presence(project_id, user_id).unwrap();
        assert_eq!(presence.active_file, Some("chapter1.tex".to_string()));
    }
}
