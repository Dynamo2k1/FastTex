//! Resource Quota Management
//!
//! This module implements per-user resource quotas to prevent abuse and ensure
//! fair resource allocation across users. It enforces limits on:
//! - Concurrent compilation jobs
//! - Total compute time
//! - Memory usage
//! - Request rate

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Error types for quota operations
#[derive(Error, Debug, Clone)]
pub enum QuotaError {
    #[error("Concurrent job limit exceeded: {current}/{max}")]
    ConcurrentJobLimitExceeded { current: usize, max: usize },
    
    #[error("Compute time quota exceeded: {used:?}/{limit:?}")]
    ComputeTimeExceeded { used: Duration, limit: Duration },
    
    #[error("Memory quota exceeded: {used}/{limit} bytes")]
    MemoryQuotaExceeded { used: u64, limit: u64 },
    
    #[error("Rate limit exceeded: retry after {retry_after_secs} seconds")]
    RateLimitExceeded { retry_after_secs: u64 },
    
    #[error("Document size exceeded: {size}/{limit} bytes")]
    DocumentSizeExceeded { size: u64, limit: u64 },
    
    #[error("Project size exceeded: {size}/{limit} bytes")]
    ProjectSizeExceeded { size: u64, limit: u64 },
}

/// User tier for quota allocation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UserTier {
    /// Free tier with limited resources
    Free,
    /// Standard paid tier
    Standard,
    /// Premium tier with higher limits
    Premium,
    /// Enterprise tier with custom limits
    Enterprise,
}

impl Default for UserTier {
    fn default() -> Self {
        UserTier::Free
    }
}

/// Quota limits for a user tier
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaLimits {
    /// Maximum concurrent compilation jobs
    pub max_concurrent_jobs: usize,
    /// Maximum compute time per hour
    pub max_compute_time_per_hour: Duration,
    /// Maximum memory per job (bytes)
    pub max_memory_per_job: u64,
    /// Maximum compilation time per job
    pub max_job_duration: Duration,
    /// Maximum document size (bytes)
    pub max_document_size: u64,
    /// Maximum project size (bytes)
    pub max_project_size: u64,
    /// Request rate limit (requests per minute)
    pub requests_per_minute: u32,
    /// Maximum percentage of total workers this user can consume
    pub max_worker_percentage: f32,
    /// Scheduling weight (higher = more priority)
    pub scheduling_weight: u32,
}

impl QuotaLimits {
    /// Get quota limits for a user tier
    pub fn for_tier(tier: UserTier) -> Self {
        match tier {
            UserTier::Free => QuotaLimits {
                max_concurrent_jobs: 1,
                max_compute_time_per_hour: Duration::from_secs(30 * 60), // 30 min
                max_memory_per_job: 256 * 1024 * 1024, // 256 MB
                max_job_duration: Duration::from_secs(5 * 60), // 5 min
                max_document_size: 5 * 1024 * 1024, // 5 MB
                max_project_size: 50 * 1024 * 1024, // 50 MB
                requests_per_minute: 10,
                max_worker_percentage: 0.05, // 5%
                scheduling_weight: 1,
            },
            UserTier::Standard => QuotaLimits {
                max_concurrent_jobs: 3,
                max_compute_time_per_hour: Duration::from_secs(120 * 60), // 2 hours
                max_memory_per_job: 512 * 1024 * 1024, // 512 MB
                max_job_duration: Duration::from_secs(10 * 60), // 10 min
                max_document_size: 10 * 1024 * 1024, // 10 MB
                max_project_size: 100 * 1024 * 1024, // 100 MB
                requests_per_minute: 30,
                max_worker_percentage: 0.15, // 15%
                scheduling_weight: 5,
            },
            UserTier::Premium => QuotaLimits {
                max_concurrent_jobs: 5,
                max_compute_time_per_hour: Duration::from_secs(300 * 60), // 5 hours
                max_memory_per_job: 1024 * 1024 * 1024, // 1 GB
                max_job_duration: Duration::from_secs(20 * 60), // 20 min
                max_document_size: 25 * 1024 * 1024, // 25 MB
                max_project_size: 500 * 1024 * 1024, // 500 MB
                requests_per_minute: 60,
                max_worker_percentage: 0.25, // 25%
                scheduling_weight: 10,
            },
            UserTier::Enterprise => QuotaLimits {
                max_concurrent_jobs: 10,
                max_compute_time_per_hour: Duration::from_secs(600 * 60), // 10 hours
                max_memory_per_job: 2 * 1024 * 1024 * 1024, // 2 GB
                max_job_duration: Duration::from_secs(30 * 60), // 30 min
                max_document_size: 50 * 1024 * 1024, // 50 MB
                max_project_size: 1024 * 1024 * 1024, // 1 GB
                requests_per_minute: 120,
                max_worker_percentage: 0.30, // 30%
                scheduling_weight: 20,
            },
        }
    }
}

/// Current resource usage for a user
#[derive(Debug, Clone, Default)]
pub struct ResourceUsage {
    /// Currently running jobs
    pub active_jobs: usize,
    /// Compute time used in current hour
    pub compute_time_this_hour: Duration,
    /// Memory currently allocated
    pub memory_allocated: u64,
    /// Request timestamps for rate limiting (sliding window)
    pub request_timestamps: Vec<Instant>,
    /// Hour boundary for compute time reset
    pub hour_start: Option<Instant>,
}

impl ResourceUsage {
    /// Clean up old request timestamps outside the rate limit window
    fn cleanup_old_requests(&mut self) {
        let cutoff = Instant::now() - Duration::from_secs(60);
        self.request_timestamps.retain(|&ts| ts > cutoff);
    }
    
    /// Reset hourly counters if hour has passed
    fn check_hour_reset(&mut self) {
        let now = Instant::now();
        match self.hour_start {
            Some(start) if now.duration_since(start) >= Duration::from_secs(3600) => {
                self.compute_time_this_hour = Duration::ZERO;
                self.hour_start = Some(now);
            }
            None => {
                // First time - initialize both
                self.hour_start = Some(now);
                self.compute_time_this_hour = Duration::ZERO;
            }
            _ => {}
        }
    }
}

/// User quota state
struct UserQuotaState {
    tier: UserTier,
    limits: QuotaLimits,
    usage: ResourceUsage,
}

/// Quota manager for enforcing resource limits
pub struct QuotaManager {
    /// Per-user quota state
    users: Arc<RwLock<HashMap<Uuid, UserQuotaState>>>,
    /// Default tier for new users
    default_tier: UserTier,
}

impl QuotaManager {
    /// Create a new quota manager
    pub fn new(default_tier: UserTier) -> Self {
        QuotaManager {
            users: Arc::new(RwLock::new(HashMap::new())),
            default_tier,
        }
    }
    
    /// Set user tier
    pub async fn set_user_tier(&self, user_id: Uuid, tier: UserTier) {
        let mut users = self.users.write().await;
        let state = users.entry(user_id).or_insert_with(|| UserQuotaState {
            tier: self.default_tier,
            limits: QuotaLimits::for_tier(self.default_tier),
            usage: ResourceUsage::default(),
        });
        state.tier = tier;
        state.limits = QuotaLimits::for_tier(tier);
    }
    
    /// Get quota limits for a user
    pub async fn get_limits(&self, user_id: Uuid) -> QuotaLimits {
        let users = self.users.read().await;
        users
            .get(&user_id)
            .map(|s| s.limits.clone())
            .unwrap_or_else(|| QuotaLimits::for_tier(self.default_tier))
    }
    
    /// Check if a job can be admitted (quota check)
    pub async fn check_admission(
        &self,
        user_id: Uuid,
        document_size: u64,
    ) -> Result<(), QuotaError> {
        let mut users = self.users.write().await;
        let state = users.entry(user_id).or_insert_with(|| UserQuotaState {
            tier: self.default_tier,
            limits: QuotaLimits::for_tier(self.default_tier),
            usage: ResourceUsage::default(),
        });
        
        // Cleanup and reset
        state.usage.cleanup_old_requests();
        state.usage.check_hour_reset();
        
        // Check concurrent job limit
        if state.usage.active_jobs >= state.limits.max_concurrent_jobs {
            return Err(QuotaError::ConcurrentJobLimitExceeded {
                current: state.usage.active_jobs,
                max: state.limits.max_concurrent_jobs,
            });
        }
        
        // Check compute time quota
        if state.usage.compute_time_this_hour >= state.limits.max_compute_time_per_hour {
            return Err(QuotaError::ComputeTimeExceeded {
                used: state.usage.compute_time_this_hour,
                limit: state.limits.max_compute_time_per_hour,
            });
        }
        
        // Check rate limit
        if state.usage.request_timestamps.len() >= state.limits.requests_per_minute as usize {
            let oldest = state.usage.request_timestamps.first().unwrap();
            let retry_after = Duration::from_secs(60).saturating_sub(oldest.elapsed());
            return Err(QuotaError::RateLimitExceeded { 
                retry_after_secs: retry_after.as_secs() 
            });
        }
        
        // Check document size
        if document_size > state.limits.max_document_size {
            return Err(QuotaError::DocumentSizeExceeded {
                size: document_size,
                limit: state.limits.max_document_size,
            });
        }
        
        Ok(())
    }
    
    /// Record that a job was started
    pub async fn record_job_start(&self, user_id: Uuid) {
        let mut users = self.users.write().await;
        let state = users.entry(user_id).or_insert_with(|| UserQuotaState {
            tier: self.default_tier,
            limits: QuotaLimits::for_tier(self.default_tier),
            usage: ResourceUsage::default(),
        });
        state.usage.active_jobs += 1;
        state.usage.request_timestamps.push(Instant::now());
    }
    
    /// Record that a job completed
    pub async fn record_job_completion(
        &self,
        user_id: Uuid,
        duration: Duration,
        memory_used: u64,
    ) {
        let mut users = self.users.write().await;
        if let Some(state) = users.get_mut(&user_id) {
            state.usage.active_jobs = state.usage.active_jobs.saturating_sub(1);
            state.usage.compute_time_this_hour += duration;
            // Memory is freed on completion
            state.usage.memory_allocated = state.usage.memory_allocated.saturating_sub(memory_used);
        }
    }
    
    /// Get current usage for a user
    pub async fn get_usage(&self, user_id: Uuid) -> ResourceUsage {
        let users = self.users.read().await;
        users
            .get(&user_id)
            .map(|s| s.usage.clone())
            .unwrap_or_default()
    }
    
    /// Get scheduling weight for a user
    pub async fn get_scheduling_weight(&self, user_id: Uuid) -> u32 {
        let users = self.users.read().await;
        users
            .get(&user_id)
            .map(|s| s.limits.scheduling_weight)
            .unwrap_or(1)
    }
}

impl Default for QuotaManager {
    fn default() -> Self {
        Self::new(UserTier::Free)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_quota_admission_check() {
        let manager = QuotaManager::new(UserTier::Free);
        let user_id = Uuid::new_v4();
        
        // First job should be admitted
        assert!(manager.check_admission(user_id, 1024).await.is_ok());
        
        // Record job start
        manager.record_job_start(user_id).await;
        
        // Second job should fail (free tier = 1 concurrent)
        let result = manager.check_admission(user_id, 1024).await;
        assert!(matches!(result, Err(QuotaError::ConcurrentJobLimitExceeded { .. })));
    }

    #[tokio::test]
    async fn test_document_size_check() {
        let manager = QuotaManager::new(UserTier::Free);
        let user_id = Uuid::new_v4();
        
        // Document within limit
        assert!(manager.check_admission(user_id, 1024).await.is_ok());
        
        // Document exceeds limit (free tier = 5MB)
        let result = manager.check_admission(user_id, 10 * 1024 * 1024).await;
        assert!(matches!(result, Err(QuotaError::DocumentSizeExceeded { .. })));
    }

    #[tokio::test]
    async fn test_tier_upgrade() {
        let manager = QuotaManager::new(UserTier::Free);
        let user_id = Uuid::new_v4();
        
        // Free tier limits
        let limits = manager.get_limits(user_id).await;
        assert_eq!(limits.max_concurrent_jobs, 1);
        
        // Upgrade to premium
        manager.set_user_tier(user_id, UserTier::Premium).await;
        
        let limits = manager.get_limits(user_id).await;
        assert_eq!(limits.max_concurrent_jobs, 5);
    }

    #[tokio::test]
    async fn test_job_completion_tracking() {
        let manager = QuotaManager::new(UserTier::Standard);
        let user_id = Uuid::new_v4();
        
        // Start multiple jobs
        manager.record_job_start(user_id).await;
        manager.record_job_start(user_id).await;
        
        let usage = manager.get_usage(user_id).await;
        assert_eq!(usage.active_jobs, 2);
        
        // Complete one job
        manager.record_job_completion(user_id, Duration::from_secs(10), 0).await;
        
        let usage = manager.get_usage(user_id).await;
        assert_eq!(usage.active_jobs, 1);
        assert!(usage.compute_time_this_hour >= Duration::from_secs(10));
    }

    #[test]
    fn test_quota_limits_by_tier() {
        let free = QuotaLimits::for_tier(UserTier::Free);
        let premium = QuotaLimits::for_tier(UserTier::Premium);
        
        assert!(premium.max_concurrent_jobs > free.max_concurrent_jobs);
        assert!(premium.max_memory_per_job > free.max_memory_per_job);
        assert!(premium.max_document_size > free.max_document_size);
        assert!(premium.scheduling_weight > free.scheduling_weight);
    }
}
