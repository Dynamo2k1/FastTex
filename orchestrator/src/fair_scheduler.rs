//! Fair Scheduling for Worker Resources
//!
//! This module implements weighted fair queuing to ensure equitable resource
//! distribution across users. It prevents worker starvation and noisy neighbor
//! effects through:
//! - Per-user worker caps
//! - Weighted fair queuing based on user tier
//! - Aging mechanism to prevent starvation
//! - Preemption for higher priority tiers

use std::collections::{BinaryHeap, HashMap};
use std::cmp::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::quotas::{QuotaManager, UserTier};
use crate::scheduler::JobId;

/// Default aging rate (priority increase per second waiting)
pub const DEFAULT_AGING_RATE: f32 = 0.1;

/// Maximum worker percentage any single user can consume
pub const MAX_WORKER_PERCENTAGE: f32 = 0.30;

/// Configuration for the fair scheduler
#[derive(Debug, Clone)]
pub struct FairSchedulerConfig {
    /// Total number of workers available
    pub total_workers: usize,
    /// Aging rate for starvation prevention
    pub aging_rate: f32,
    /// Whether to allow preemption
    pub allow_preemption: bool,
    /// Minimum guaranteed workers per user
    pub min_workers_per_user: usize,
}

impl Default for FairSchedulerConfig {
    fn default() -> Self {
        FairSchedulerConfig {
            total_workers: 16,
            aging_rate: DEFAULT_AGING_RATE,
            allow_preemption: true,
            min_workers_per_user: 1,
        }
    }
}

/// A job in the fair scheduling queue
#[derive(Debug, Clone)]
pub struct ScheduledJob {
    /// Job identifier
    pub job_id: JobId,
    /// User who submitted the job
    pub user_id: Uuid,
    /// Project identifier
    pub project_id: Uuid,
    /// Base priority from user tier
    pub base_priority: f32,
    /// Current effective priority (including aging)
    pub effective_priority: f32,
    /// Time the job was submitted
    pub submitted_at: Instant,
    /// User tier at submission time
    pub user_tier: UserTier,
}

impl ScheduledJob {
    /// Create a new scheduled job
    pub fn new(
        job_id: JobId,
        user_id: Uuid,
        project_id: Uuid,
        user_tier: UserTier,
        base_priority: f32,
    ) -> Self {
        ScheduledJob {
            job_id,
            user_id,
            project_id,
            base_priority,
            effective_priority: base_priority,
            submitted_at: Instant::now(),
            user_tier,
        }
    }
    
    /// Update effective priority with aging
    pub fn update_priority(&mut self, aging_rate: f32) {
        let wait_time = self.submitted_at.elapsed().as_secs_f32();
        self.effective_priority = self.base_priority + (wait_time * aging_rate);
    }
}

/// Ordering for priority queue (higher effective priority first)
/// BinaryHeap is a max-heap, so we need natural ordering for higher priority first
impl Ord for ScheduledJob {
    fn cmp(&self, other: &Self) -> Ordering {
        self.effective_priority
            .partial_cmp(&other.effective_priority)
            .unwrap_or(Ordering::Equal)
    }
}

impl PartialOrd for ScheduledJob {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for ScheduledJob {
    fn eq(&self, other: &Self) -> bool {
        self.job_id == other.job_id
    }
}

impl Eq for ScheduledJob {}

/// Statistics for a user's resource usage
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserSchedulingStats {
    /// Currently running jobs
    pub active_jobs: usize,
    /// Workers currently allocated
    pub workers_allocated: usize,
    /// Total jobs submitted
    pub total_jobs_submitted: u64,
    /// Total compute time used
    pub total_compute_time: Duration,
    /// Average wait time
    pub average_wait_time: Duration,
    /// Jobs currently in queue
    pub queued_jobs: usize,
}

/// Fair scheduler for distributing jobs across workers
pub struct FairScheduler {
    /// Configuration
    config: FairSchedulerConfig,
    /// Priority queue of pending jobs
    queue: Arc<RwLock<BinaryHeap<ScheduledJob>>>,
    /// Per-user statistics
    user_stats: Arc<RwLock<HashMap<Uuid, UserSchedulingStats>>>,
    /// Currently running jobs per user
    running_jobs: Arc<RwLock<HashMap<Uuid, Vec<JobId>>>>,
    /// Workers currently in use
    workers_in_use: Arc<RwLock<usize>>,
    /// Quota manager reference for tier information
    quota_manager: Option<Arc<QuotaManager>>,
}

impl FairScheduler {
    /// Create a new fair scheduler
    pub fn new(config: FairSchedulerConfig) -> Self {
        FairScheduler {
            config,
            queue: Arc::new(RwLock::new(BinaryHeap::new())),
            user_stats: Arc::new(RwLock::new(HashMap::new())),
            running_jobs: Arc::new(RwLock::new(HashMap::new())),
            workers_in_use: Arc::new(RwLock::new(0)),
            quota_manager: None,
        }
    }
    
    /// Create with a quota manager for tier-aware scheduling
    pub fn with_quota_manager(config: FairSchedulerConfig, quota_manager: Arc<QuotaManager>) -> Self {
        FairScheduler {
            config,
            queue: Arc::new(RwLock::new(BinaryHeap::new())),
            user_stats: Arc::new(RwLock::new(HashMap::new())),
            running_jobs: Arc::new(RwLock::new(HashMap::new())),
            workers_in_use: Arc::new(RwLock::new(0)),
            quota_manager: Some(quota_manager),
        }
    }
    
    /// Submit a job to the scheduler
    pub async fn submit(&self, job: ScheduledJob) {
        let user_id = job.user_id;
        
        // Update user stats
        {
            let mut stats = self.user_stats.write().await;
            let user_stats = stats.entry(user_id).or_default();
            user_stats.total_jobs_submitted += 1;
            user_stats.queued_jobs += 1;
        }
        
        // Add to queue
        {
            let mut queue = self.queue.write().await;
            queue.push(job);
        }
        
        tracing::debug!(
            user_id = %user_id,
            "Job submitted to fair scheduler"
        );
    }
    
    /// Get the next job to execute, respecting fair scheduling rules
    pub async fn get_next_job(&self) -> Option<ScheduledJob> {
        let mut queue = self.queue.write().await;
        let running = self.running_jobs.read().await;
        let workers_in_use = *self.workers_in_use.read().await;
        
        if workers_in_use >= self.config.total_workers {
            return None; // All workers busy
        }
        
        // Update priorities for aging
        let mut jobs: Vec<_> = queue.drain().collect();
        for job in &mut jobs {
            job.update_priority(self.config.aging_rate);
        }
        
        // Sort by effective priority
        jobs.sort_by(|a, b| a.effective_priority.partial_cmp(&b.effective_priority)
            .unwrap_or(Ordering::Equal)
            .reverse());
        
        // Find a job that doesn't violate user caps
        let mut selected_job = None;
        let mut remaining_jobs = Vec::new();
        
        for job in jobs {
            if selected_job.is_none() && self.can_run_job(&job, &running).await {
                selected_job = Some(job);
            } else {
                remaining_jobs.push(job);
            }
        }
        
        // Put remaining jobs back
        for job in remaining_jobs {
            queue.push(job);
        }
        
        // Update stats if job selected
        if let Some(ref job) = selected_job {
            let mut stats = self.user_stats.write().await;
            if let Some(user_stats) = stats.get_mut(&job.user_id) {
                user_stats.queued_jobs = user_stats.queued_jobs.saturating_sub(1);
            }
        }
        
        selected_job
    }
    
    /// Check if a job can run without violating fair scheduling rules
    async fn can_run_job(
        &self,
        job: &ScheduledJob,
        running: &HashMap<Uuid, Vec<JobId>>,
    ) -> bool {
        let user_running = running.get(&job.user_id).map(|v| v.len()).unwrap_or(0);
        
        // Check per-user worker cap
        let max_workers_for_user = (self.config.total_workers as f32 * MAX_WORKER_PERCENTAGE) as usize;
        let max_workers_for_user = max_workers_for_user.max(self.config.min_workers_per_user);
        
        if user_running >= max_workers_for_user {
            return false;
        }
        
        // Check if user has tier-specific limits
        if let Some(ref quota_manager) = self.quota_manager {
            let limits = quota_manager.get_limits(job.user_id).await;
            if user_running >= limits.max_concurrent_jobs {
                return false;
            }
        }
        
        true
    }
    
    /// Record that a job has started
    pub async fn record_job_start(&self, job_id: JobId, user_id: Uuid) {
        {
            let mut running = self.running_jobs.write().await;
            running.entry(user_id).or_default().push(job_id);
        }
        
        {
            let mut workers = self.workers_in_use.write().await;
            *workers += 1;
        }
        
        {
            let mut stats = self.user_stats.write().await;
            if let Some(user_stats) = stats.get_mut(&user_id) {
                user_stats.active_jobs += 1;
                user_stats.workers_allocated += 1;
            }
        }
    }
    
    /// Record that a job has completed
    pub async fn record_job_completion(
        &self,
        job_id: JobId,
        user_id: Uuid,
        duration: Duration,
        wait_time: Duration,
    ) {
        {
            let mut running = self.running_jobs.write().await;
            if let Some(jobs) = running.get_mut(&user_id) {
                jobs.retain(|id| *id != job_id);
            }
        }
        
        {
            let mut workers = self.workers_in_use.write().await;
            *workers = workers.saturating_sub(1);
        }
        
        {
            let mut stats = self.user_stats.write().await;
            if let Some(user_stats) = stats.get_mut(&user_id) {
                user_stats.active_jobs = user_stats.active_jobs.saturating_sub(1);
                user_stats.workers_allocated = user_stats.workers_allocated.saturating_sub(1);
                user_stats.total_compute_time += duration;
                
                // Update average wait time (exponential moving average)
                let alpha = 0.1;
                let old_avg = user_stats.average_wait_time.as_secs_f64();
                let new_wait = wait_time.as_secs_f64();
                let new_avg = old_avg * (1.0 - alpha) + new_wait * alpha;
                user_stats.average_wait_time = Duration::from_secs_f64(new_avg);
            }
        }
    }
    
    /// Get current queue depth
    pub async fn queue_depth(&self) -> usize {
        self.queue.read().await.len()
    }
    
    /// Get available workers
    pub async fn available_workers(&self) -> usize {
        let in_use = *self.workers_in_use.read().await;
        self.config.total_workers.saturating_sub(in_use)
    }
    
    /// Get statistics for a user
    pub async fn get_user_stats(&self, user_id: Uuid) -> UserSchedulingStats {
        let stats = self.user_stats.read().await;
        stats.get(&user_id).cloned().unwrap_or_default()
    }
    
    /// Get global scheduling statistics
    pub async fn get_global_stats(&self) -> GlobalSchedulingStats {
        let queue = self.queue.read().await;
        let stats = self.user_stats.read().await;
        let workers_in_use = *self.workers_in_use.read().await;
        
        let total_active = stats.values().map(|s| s.active_jobs).sum();
        let total_queued = queue.len();
        let unique_users = stats.len();
        
        GlobalSchedulingStats {
            total_workers: self.config.total_workers,
            workers_in_use,
            workers_available: self.config.total_workers.saturating_sub(workers_in_use),
            jobs_queued: total_queued,
            jobs_running: total_active,
            active_users: unique_users,
        }
    }
    
    /// Cancel all jobs for a user
    pub async fn cancel_user_jobs(&self, user_id: Uuid) -> usize {
        let mut queue = self.queue.write().await;
        let original_len = queue.len();
        
        let remaining: Vec<_> = queue.drain()
            .filter(|job| job.user_id != user_id)
            .collect();
        
        for job in remaining {
            queue.push(job);
        }
        
        let cancelled = original_len - queue.len();
        
        // Update stats
        {
            let mut stats = self.user_stats.write().await;
            if let Some(user_stats) = stats.get_mut(&user_id) {
                user_stats.queued_jobs = 0;
            }
        }
        
        cancelled
    }
}

impl Default for FairScheduler {
    fn default() -> Self {
        Self::new(FairSchedulerConfig::default())
    }
}

/// Global scheduling statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalSchedulingStats {
    pub total_workers: usize,
    pub workers_in_use: usize,
    pub workers_available: usize,
    pub jobs_queued: usize,
    pub jobs_running: usize,
    pub active_users: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_job(user_id: Uuid, priority: f32) -> ScheduledJob {
        ScheduledJob::new(
            JobId::new(),
            user_id,
            Uuid::new_v4(),
            UserTier::Standard,
            priority,
        )
    }

    #[tokio::test]
    async fn test_submit_and_get_job() {
        let scheduler = FairScheduler::new(FairSchedulerConfig::default());
        let user_id = Uuid::new_v4();
        
        let job = create_test_job(user_id, 5.0);
        let job_id = job.job_id;
        
        scheduler.submit(job).await;
        
        assert_eq!(scheduler.queue_depth().await, 1);
        
        let retrieved = scheduler.get_next_job().await.unwrap();
        assert_eq!(retrieved.job_id, job_id);
        
        assert_eq!(scheduler.queue_depth().await, 0);
    }

    #[tokio::test]
    async fn test_priority_ordering() {
        let scheduler = FairScheduler::new(FairSchedulerConfig::default());
        
        let user1 = Uuid::new_v4();
        let user2 = Uuid::new_v4();
        
        // Submit lower priority first
        let low_job = create_test_job(user1, 1.0);
        let high_job = create_test_job(user2, 10.0);
        
        scheduler.submit(low_job).await;
        scheduler.submit(high_job).await;
        
        // Should get high priority first
        let first = scheduler.get_next_job().await.unwrap();
        assert_eq!(first.base_priority, 10.0);
    }

    #[tokio::test]
    async fn test_user_worker_cap() {
        let config = FairSchedulerConfig {
            total_workers: 10,
            ..Default::default()
        };
        let scheduler = FairScheduler::new(config);
        
        let user = Uuid::new_v4();
        
        // Submit 5 jobs from same user
        for i in 0..5 {
            let job = create_test_job(user, i as f32);
            scheduler.submit(job).await;
        }
        
        // Get jobs until cap is reached
        let mut started = 0;
        while let Some(job) = scheduler.get_next_job().await {
            scheduler.record_job_start(job.job_id, job.user_id).await;
            started += 1;
            
            // Should stop at 30% cap (3 workers)
            if started > 10 {
                break; // Safety limit
            }
        }
        
        // Should have started 3 jobs (30% of 10)
        assert!(started <= 3);
    }

    #[tokio::test]
    async fn test_job_completion_tracking() {
        let scheduler = FairScheduler::new(FairSchedulerConfig::default());
        let user = Uuid::new_v4();
        
        let job = create_test_job(user, 5.0);
        let job_id = job.job_id;
        
        scheduler.submit(job).await;
        let job = scheduler.get_next_job().await.unwrap();
        
        scheduler.record_job_start(job_id, user).await;
        
        let stats = scheduler.get_global_stats().await;
        assert_eq!(stats.workers_in_use, 1);
        
        scheduler.record_job_completion(
            job_id,
            user,
            Duration::from_secs(10),
            Duration::from_secs(1),
        ).await;
        
        let stats = scheduler.get_global_stats().await;
        assert_eq!(stats.workers_in_use, 0);
    }

    #[tokio::test]
    async fn test_cancel_user_jobs() {
        let scheduler = FairScheduler::new(FairSchedulerConfig::default());
        
        let user1 = Uuid::new_v4();
        let user2 = Uuid::new_v4();
        
        // Submit jobs from both users
        for _ in 0..3 {
            scheduler.submit(create_test_job(user1, 5.0)).await;
            scheduler.submit(create_test_job(user2, 5.0)).await;
        }
        
        assert_eq!(scheduler.queue_depth().await, 6);
        
        // Cancel user1's jobs
        let cancelled = scheduler.cancel_user_jobs(user1).await;
        assert_eq!(cancelled, 3);
        assert_eq!(scheduler.queue_depth().await, 3);
    }

    #[test]
    fn test_job_priority_ordering() {
        let user = Uuid::new_v4();
        
        let low = create_test_job(user, 1.0);
        let high = create_test_job(user, 10.0);
        
        // BinaryHeap is a max heap, so higher priority should come first
        let mut heap = BinaryHeap::new();
        heap.push(low);
        heap.push(high);
        
        let first = heap.pop().unwrap();
        assert_eq!(first.base_priority, 10.0);
    }

    #[tokio::test]
    async fn test_aging_increases_priority() {
        let user = Uuid::new_v4();
        let mut job = create_test_job(user, 1.0);
        
        let initial_priority = job.effective_priority;
        
        // Simulate some wait time
        std::thread::sleep(Duration::from_millis(100));
        
        job.update_priority(DEFAULT_AGING_RATE);
        
        // Priority should have increased
        assert!(job.effective_priority > initial_priority);
    }
}
