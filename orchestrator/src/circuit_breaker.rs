//! Circuit Breaker for Poisoned Job Handling
//!
//! This module implements the circuit breaker pattern to handle jobs that
//! repeatedly fail. It prevents continuously failing jobs from consuming
//! resources and allows the system to recover gracefully.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

/// Maximum failures before quarantine
pub const DEFAULT_MAX_FAILURES: u32 = 3;

/// Default quarantine duration
pub const DEFAULT_QUARANTINE_DURATION: Duration = Duration::from_secs(15 * 60); // 15 minutes

/// Cooldown period after successful compilation
pub const DEFAULT_COOLDOWN_PERIOD: Duration = Duration::from_secs(5 * 60); // 5 minutes

/// State of the circuit breaker
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CircuitState {
    /// Circuit is closed - jobs allowed through
    Closed,
    /// Circuit is open - jobs rejected
    Open,
    /// Circuit is half-open - allowing test request
    HalfOpen,
}

/// Failure record for tracking job failures
#[derive(Debug, Clone)]
struct FailureRecord {
    /// Number of consecutive failures
    failure_count: u32,
    /// Timestamp of last failure
    last_failure: Instant,
    /// Timestamp when quarantine ends (if open)
    quarantine_until: Option<Instant>,
    /// Current circuit state
    state: CircuitState,
    /// Error messages from failures
    error_messages: Vec<String>,
}

impl Default for FailureRecord {
    fn default() -> Self {
        FailureRecord {
            failure_count: 0,
            last_failure: Instant::now(),
            quarantine_until: None,
            state: CircuitState::Closed,
            error_messages: Vec::new(),
        }
    }
}

/// Configuration for the circuit breaker
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Maximum failures before opening circuit
    pub max_failures: u32,
    /// Duration of quarantine when circuit opens
    pub quarantine_duration: Duration,
    /// Cooldown period for resetting failure count
    pub cooldown_period: Duration,
    /// Maximum error messages to retain
    pub max_error_messages: usize,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        CircuitBreakerConfig {
            max_failures: DEFAULT_MAX_FAILURES,
            quarantine_duration: DEFAULT_QUARANTINE_DURATION,
            cooldown_period: DEFAULT_COOLDOWN_PERIOD,
            max_error_messages: 10,
        }
    }
}

/// Result of a circuit breaker check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CircuitCheckResult {
    /// Request allowed through
    Allowed,
    /// Request rejected - circuit is open
    Rejected {
        /// When the quarantine will end
        retry_after: Duration,
        /// Reason for rejection
        reason: String,
    },
    /// Request allowed as a test (half-open state)
    TestRequest,
}

/// Information about a quarantined document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarantineInfo {
    /// Document/project ID
    pub document_id: Uuid,
    /// Current circuit state
    pub state: CircuitState,
    /// Number of failures
    pub failure_count: u32,
    /// Time until quarantine ends (if open)
    pub quarantine_remaining: Option<Duration>,
    /// Recent error messages
    pub recent_errors: Vec<String>,
}

/// Circuit breaker for handling poisoned/failing jobs
pub struct CircuitBreaker {
    /// Per-document failure tracking
    records: Arc<RwLock<HashMap<Uuid, FailureRecord>>>,
    /// Configuration
    config: CircuitBreakerConfig,
}

impl CircuitBreaker {
    /// Create a new circuit breaker with default configuration
    pub fn new() -> Self {
        CircuitBreaker {
            records: Arc::new(RwLock::new(HashMap::new())),
            config: CircuitBreakerConfig::default(),
        }
    }
    
    /// Create a circuit breaker with custom configuration
    pub fn with_config(config: CircuitBreakerConfig) -> Self {
        CircuitBreaker {
            records: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }
    
    /// Check if a request should be allowed
    pub async fn check(&self, document_id: Uuid) -> CircuitCheckResult {
        let mut records = self.records.write().await;
        let record = records.entry(document_id).or_default();
        
        // Update state based on time
        self.update_state(record);
        
        match record.state {
            CircuitState::Closed => CircuitCheckResult::Allowed,
            CircuitState::Open => {
                let retry_after = record.quarantine_until
                    .map(|until| until.saturating_duration_since(Instant::now()))
                    .unwrap_or_default();
                
                CircuitCheckResult::Rejected {
                    retry_after,
                    reason: format!(
                        "Document quarantined after {} consecutive failures. \
                         Recent errors: {:?}",
                        record.failure_count,
                        record.error_messages.last()
                    ),
                }
            }
            CircuitState::HalfOpen => CircuitCheckResult::TestRequest,
        }
    }
    
    /// Record a successful compilation
    pub async fn record_success(&self, document_id: Uuid) {
        let mut records = self.records.write().await;
        
        if let Some(record) = records.get_mut(&document_id) {
            // Reset on success
            record.failure_count = 0;
            record.state = CircuitState::Closed;
            record.quarantine_until = None;
            record.error_messages.clear();
            
            tracing::info!(
                document_id = %document_id,
                "Circuit breaker reset after successful compilation"
            );
        }
    }
    
    /// Record a failed compilation
    pub async fn record_failure(&self, document_id: Uuid, error_message: String) {
        let mut records = self.records.write().await;
        let record = records.entry(document_id).or_default();
        
        record.failure_count += 1;
        record.last_failure = Instant::now();
        
        // Keep recent error messages
        record.error_messages.push(error_message);
        if record.error_messages.len() > self.config.max_error_messages {
            record.error_messages.remove(0);
        }
        
        // Check if we should open the circuit
        if record.failure_count >= self.config.max_failures {
            record.state = CircuitState::Open;
            record.quarantine_until = Some(Instant::now() + self.config.quarantine_duration);
            
            tracing::warn!(
                document_id = %document_id,
                failures = record.failure_count,
                quarantine_duration = ?self.config.quarantine_duration,
                "Circuit breaker opened - document quarantined"
            );
        }
    }
    
    /// Get quarantine information for a document
    pub async fn get_quarantine_info(&self, document_id: Uuid) -> Option<QuarantineInfo> {
        let records = self.records.read().await;
        records.get(&document_id).map(|record| {
            let quarantine_remaining = record.quarantine_until
                .filter(|&until| until > Instant::now())
                .map(|until| until.saturating_duration_since(Instant::now()));
            
            QuarantineInfo {
                document_id,
                state: record.state,
                failure_count: record.failure_count,
                quarantine_remaining,
                recent_errors: record.error_messages.clone(),
            }
        })
    }
    
    /// Manually release a document from quarantine
    pub async fn release_quarantine(&self, document_id: Uuid) {
        let mut records = self.records.write().await;
        
        if let Some(record) = records.get_mut(&document_id) {
            record.state = CircuitState::Closed;
            record.quarantine_until = None;
            record.failure_count = 0;
            record.error_messages.clear();
            
            tracing::info!(
                document_id = %document_id,
                "Circuit breaker manually released from quarantine"
            );
        }
    }
    
    /// Get all quarantined documents
    pub async fn get_all_quarantined(&self) -> Vec<QuarantineInfo> {
        let records = self.records.read().await;
        
        records.iter()
            .filter(|(_, record)| record.state == CircuitState::Open)
            .map(|(id, record)| {
                let quarantine_remaining = record.quarantine_until
                    .filter(|&until| until > Instant::now())
                    .map(|until| until.saturating_duration_since(Instant::now()));
                
                QuarantineInfo {
                    document_id: *id,
                    state: record.state,
                    failure_count: record.failure_count,
                    quarantine_remaining,
                    recent_errors: record.error_messages.clone(),
                }
            })
            .collect()
    }
    
    /// Cleanup old records that have been idle
    pub async fn cleanup_idle_records(&self, idle_threshold: Duration) {
        let mut records = self.records.write().await;
        let now = Instant::now();
        
        records.retain(|_, record| {
            let is_recent = now.duration_since(record.last_failure) < idle_threshold;
            let is_quarantined = record.state == CircuitState::Open;
            is_recent || is_quarantined
        });
    }
    
    /// Update state based on elapsed time
    fn update_state(&self, record: &mut FailureRecord) {
        let now = Instant::now();
        
        match record.state {
            CircuitState::Open => {
                // Check if quarantine has expired
                if let Some(until) = record.quarantine_until {
                    if now >= until {
                        record.state = CircuitState::HalfOpen;
                        tracing::info!("Circuit breaker transitioned to half-open state");
                    }
                }
            }
            CircuitState::Closed => {
                // Check if we should reset failure count after cooldown
                if now.duration_since(record.last_failure) > self.config.cooldown_period {
                    record.failure_count = 0;
                }
            }
            CircuitState::HalfOpen => {
                // State unchanged - waiting for test result
            }
        }
    }
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_initial_check_allowed() {
        let breaker = CircuitBreaker::new();
        let doc_id = Uuid::new_v4();
        
        let result = breaker.check(doc_id).await;
        assert!(matches!(result, CircuitCheckResult::Allowed));
    }

    #[tokio::test]
    async fn test_failures_open_circuit() {
        let breaker = CircuitBreaker::new();
        let doc_id = Uuid::new_v4();
        
        // Record 3 failures (default max)
        for i in 0..3 {
            breaker.record_failure(doc_id, format!("Error {}", i)).await;
        }
        
        let result = breaker.check(doc_id).await;
        assert!(matches!(result, CircuitCheckResult::Rejected { .. }));
    }

    #[tokio::test]
    async fn test_success_resets_circuit() {
        let breaker = CircuitBreaker::new();
        let doc_id = Uuid::new_v4();
        
        // Record failures
        for _ in 0..3 {
            breaker.record_failure(doc_id, "Error".to_string()).await;
        }
        
        // Verify quarantined
        assert!(matches!(breaker.check(doc_id).await, CircuitCheckResult::Rejected { .. }));
        
        // Record success (should reset)
        breaker.record_success(doc_id).await;
        
        // Should be allowed again
        assert!(matches!(breaker.check(doc_id).await, CircuitCheckResult::Allowed));
    }

    #[tokio::test]
    async fn test_quarantine_info() {
        let breaker = CircuitBreaker::new();
        let doc_id = Uuid::new_v4();
        
        // Initially no info
        let info = breaker.get_quarantine_info(doc_id).await;
        assert!(info.is_none());
        
        // After failures
        for i in 0..3 {
            breaker.record_failure(doc_id, format!("Error {}", i)).await;
        }
        
        let info = breaker.get_quarantine_info(doc_id).await.unwrap();
        assert_eq!(info.state, CircuitState::Open);
        assert_eq!(info.failure_count, 3);
        assert_eq!(info.recent_errors.len(), 3);
    }

    #[tokio::test]
    async fn test_manual_release() {
        let breaker = CircuitBreaker::new();
        let doc_id = Uuid::new_v4();
        
        // Quarantine
        for _ in 0..3 {
            breaker.record_failure(doc_id, "Error".to_string()).await;
        }
        
        // Manual release
        breaker.release_quarantine(doc_id).await;
        
        // Should be allowed
        assert!(matches!(breaker.check(doc_id).await, CircuitCheckResult::Allowed));
    }

    #[tokio::test]
    async fn test_partial_failures_allowed() {
        let breaker = CircuitBreaker::new();
        let doc_id = Uuid::new_v4();
        
        // Record 2 failures (below max)
        breaker.record_failure(doc_id, "Error 1".to_string()).await;
        breaker.record_failure(doc_id, "Error 2".to_string()).await;
        
        // Should still be allowed
        assert!(matches!(breaker.check(doc_id).await, CircuitCheckResult::Allowed));
    }

    #[tokio::test]
    async fn test_custom_config() {
        let config = CircuitBreakerConfig {
            max_failures: 5,
            quarantine_duration: Duration::from_secs(60),
            cooldown_period: Duration::from_secs(30),
            max_error_messages: 5,
        };
        
        let breaker = CircuitBreaker::with_config(config);
        let doc_id = Uuid::new_v4();
        
        // Need 5 failures with custom config
        for _ in 0..4 {
            breaker.record_failure(doc_id, "Error".to_string()).await;
        }
        
        // Still allowed with 4 failures
        assert!(matches!(breaker.check(doc_id).await, CircuitCheckResult::Allowed));
        
        // 5th failure opens circuit
        breaker.record_failure(doc_id, "Error".to_string()).await;
        assert!(matches!(breaker.check(doc_id).await, CircuitCheckResult::Rejected { .. }));
    }

    #[tokio::test]
    async fn test_get_all_quarantined() {
        let breaker = CircuitBreaker::new();
        
        let doc1 = Uuid::new_v4();
        let doc2 = Uuid::new_v4();
        let doc3 = Uuid::new_v4();
        
        // Quarantine doc1 and doc2
        for _ in 0..3 {
            breaker.record_failure(doc1, "Error".to_string()).await;
            breaker.record_failure(doc2, "Error".to_string()).await;
        }
        
        // doc3 has failures but not quarantined
        breaker.record_failure(doc3, "Error".to_string()).await;
        
        let quarantined = breaker.get_all_quarantined().await;
        assert_eq!(quarantined.len(), 2);
    }
}
