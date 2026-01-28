//! Convergence Detection and Handling
//!
//! This module implements detection and handling of reference convergence in LaTeX
//! compilation. It tracks auxiliary file states across compilation passes and
//! determines when references have stabilized or when to give up.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Maximum number of passes for reference convergence
pub const MAX_CONVERGENCE_PASSES: u32 = 5;

/// Maximum time for the entire convergence process
pub const MAX_CONVERGENCE_TIME: Duration = Duration::from_secs(150); // 2.5 min

/// Error types for convergence operations
#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum ConvergenceError {
    #[error("Convergence not reached after {passes} passes")]
    MaxPassesExceeded { passes: u32 },
    
    #[error("Convergence timeout after {elapsed_secs} seconds")]
    Timeout { elapsed_secs: u64 },
    
    #[error("Oscillation detected: aux files alternating between states")]
    OscillationDetected,
    
    #[error("Compilation error prevents convergence: {message}")]
    CompilationError { message: String },
}

/// State of an auxiliary file for tracking changes
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AuxFileState {
    /// Hash of the file content
    pub content_hash: String,
    /// Size in bytes
    pub size: usize,
}

impl AuxFileState {
    /// Create state from file content
    pub fn from_content(content: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(content);
        let hash = hex::encode(hasher.finalize());
        
        AuxFileState {
            content_hash: hash,
            size: content.len(),
        }
    }
}

/// Result of convergence analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConvergenceResult {
    /// References have converged (stable state reached)
    Converged {
        /// Number of passes required
        passes: u32,
        /// Total time taken
        duration: Duration,
    },
    /// References did not converge but a stable output was produced
    PartialConvergence {
        /// Number of passes attempted
        passes: u32,
        /// Total time taken
        duration: Duration,
        /// Files that are still changing
        unstable_files: Vec<PathBuf>,
        /// Warning message for user
        warning: String,
    },
    /// Convergence failed entirely
    Failed {
        /// Error that caused failure
        error: ConvergenceError,
        /// Last known state
        last_state: Option<ConvergenceState>,
    },
}

impl ConvergenceResult {
    /// Check if convergence was successful (full or partial)
    pub fn is_successful(&self) -> bool {
        matches!(self, ConvergenceResult::Converged { .. } | ConvergenceResult::PartialConvergence { .. })
    }
    
    /// Get the number of passes used
    pub fn passes(&self) -> u32 {
        match self {
            ConvergenceResult::Converged { passes, .. } => *passes,
            ConvergenceResult::PartialConvergence { passes, .. } => *passes,
            ConvergenceResult::Failed { .. } => 0,
        }
    }
}

/// State of the convergence process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvergenceState {
    /// Current pass number (1-indexed)
    pub current_pass: u32,
    /// States of aux files at each pass
    pub pass_states: Vec<HashMap<PathBuf, AuxFileState>>,
    /// Start time of convergence process
    #[serde(skip)]
    pub start_time: Option<Instant>,
    /// Whether convergence has been reached
    pub converged: bool,
    /// Files that changed in the last pass
    pub changed_files: Vec<PathBuf>,
}

impl Default for ConvergenceState {
    fn default() -> Self {
        ConvergenceState {
            current_pass: 0,
            pass_states: Vec::new(),
            start_time: None,
            converged: false,
            changed_files: Vec::new(),
        }
    }
}

/// Convergence checker for LaTeX compilation
pub struct ConvergenceChecker {
    /// Maximum passes allowed
    max_passes: u32,
    /// Maximum time allowed
    max_time: Duration,
    /// Current state
    state: ConvergenceState,
}

impl ConvergenceChecker {
    /// Create a new convergence checker with default limits
    pub fn new() -> Self {
        ConvergenceChecker {
            max_passes: MAX_CONVERGENCE_PASSES,
            max_time: MAX_CONVERGENCE_TIME,
            state: ConvergenceState::default(),
        }
    }
    
    /// Create a convergence checker with custom limits
    pub fn with_limits(max_passes: u32, max_time: Duration) -> Self {
        ConvergenceChecker {
            max_passes,
            max_time,
            state: ConvergenceState::default(),
        }
    }
    
    /// Start or reset the convergence process
    pub fn start(&mut self) {
        self.state = ConvergenceState {
            start_time: Some(Instant::now()),
            ..Default::default()
        };
    }
    
    /// Record the state after a compilation pass
    /// Returns whether another pass is needed
    pub fn record_pass(
        &mut self,
        aux_files: &HashMap<PathBuf, Vec<u8>>,
    ) -> Result<bool, ConvergenceError> {
        // Check timeout
        if let Some(start) = self.state.start_time {
            if start.elapsed() > self.max_time {
                return Err(ConvergenceError::Timeout {
                    elapsed_secs: start.elapsed().as_secs(),
                });
            }
        }
        
        self.state.current_pass += 1;
        
        // Convert to state representation
        let current_state: HashMap<PathBuf, AuxFileState> = aux_files
            .iter()
            .map(|(path, content)| (path.clone(), AuxFileState::from_content(content)))
            .collect();
        
        // Check for convergence against previous pass
        if let Some(previous_state) = self.state.pass_states.last() {
            let changed: Vec<PathBuf> = current_state
                .iter()
                .filter(|(path, state)| {
                    previous_state.get(*path).map(|prev| prev != *state).unwrap_or(true)
                })
                .map(|(path, _)| path.clone())
                .collect();
            
            self.state.changed_files = changed;
            
            if self.state.changed_files.is_empty() {
                // Converged!
                self.state.converged = true;
                self.state.pass_states.push(current_state);
                return Ok(false); // No more passes needed
            }
        }
        
        // Check for oscillation (state matches a state from 2+ passes ago)
        if self.state.pass_states.len() >= 2 {
            for (i, old_state) in self.state.pass_states.iter().rev().skip(1).enumerate() {
                if &current_state == old_state {
                    // Oscillation detected - we're cycling between states
                    tracing::warn!(
                        "Oscillation detected: current state matches state from {} passes ago",
                        i + 2
                    );
                    self.state.pass_states.push(current_state);
                    return Err(ConvergenceError::OscillationDetected);
                }
            }
        }
        
        self.state.pass_states.push(current_state);
        
        // Check max passes
        if self.state.current_pass >= self.max_passes {
            return Err(ConvergenceError::MaxPassesExceeded {
                passes: self.state.current_pass,
            });
        }
        
        Ok(true) // More passes needed
    }
    
    /// Get the final convergence result
    pub fn finalize(&self) -> ConvergenceResult {
        let duration = self.state.start_time
            .map(|s| s.elapsed())
            .unwrap_or_default();
        
        if self.state.converged {
            ConvergenceResult::Converged {
                passes: self.state.current_pass,
                duration,
            }
        } else if self.state.current_pass > 0 {
            let warning = format!(
                "References did not fully converge after {} passes. \
                 The following files are still changing: {:?}. \
                 Consider using linear mode for guaranteed correctness.",
                self.state.current_pass,
                self.state.changed_files
            );
            
            ConvergenceResult::PartialConvergence {
                passes: self.state.current_pass,
                duration,
                unstable_files: self.state.changed_files.clone(),
                warning,
            }
        } else {
            ConvergenceResult::Failed {
                error: ConvergenceError::MaxPassesExceeded { passes: 0 },
                last_state: Some(self.state.clone()),
            }
        }
    }
    
    /// Handle a convergence error gracefully
    pub fn handle_error(&self, error: ConvergenceError) -> ConvergenceResult {
        let duration = self.state.start_time
            .map(|s| s.elapsed())
            .unwrap_or_default();
        
        match &error {
            ConvergenceError::MaxPassesExceeded { .. } | ConvergenceError::OscillationDetected => {
                // Return partial result instead of failing
                let warning = match &error {
                    ConvergenceError::MaxPassesExceeded { passes } => format!(
                        "Maximum passes ({}) exceeded. Using last compilation result.",
                        passes
                    ),
                    ConvergenceError::OscillationDetected => 
                        "Reference oscillation detected. Document has unstable cross-references. \
                         Using last compilation result.".to_string(),
                    _ => error.to_string(),
                };
                
                ConvergenceResult::PartialConvergence {
                    passes: self.state.current_pass,
                    duration,
                    unstable_files: self.state.changed_files.clone(),
                    warning,
                }
            }
            ConvergenceError::Timeout { .. } | ConvergenceError::CompilationError { .. } => {
                ConvergenceResult::Failed {
                    error,
                    last_state: Some(self.state.clone()),
                }
            }
        }
    }
    
    /// Get current pass number
    pub fn current_pass(&self) -> u32 {
        self.state.current_pass
    }
    
    /// Check if more passes are allowed
    pub fn can_continue(&self) -> bool {
        if let Some(start) = self.state.start_time {
            if start.elapsed() > self.max_time {
                return false;
            }
        }
        self.state.current_pass < self.max_passes && !self.state.converged
    }
    
    /// Get the current state
    pub fn state(&self) -> &ConvergenceState {
        &self.state
    }
}

impl Default for ConvergenceChecker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_immediate_convergence() {
        let mut checker = ConvergenceChecker::new();
        checker.start();
        
        let mut aux_files = HashMap::new();
        aux_files.insert(PathBuf::from("main.aux"), b"\\relax".to_vec());
        
        // First pass
        let needs_more = checker.record_pass(&aux_files).unwrap();
        assert!(needs_more); // Need at least one more pass to confirm
        
        // Second pass with same content
        let needs_more = checker.record_pass(&aux_files).unwrap();
        assert!(!needs_more); // Converged
        
        let result = checker.finalize();
        assert!(matches!(result, ConvergenceResult::Converged { passes: 2, .. }));
    }

    #[test]
    fn test_convergence_after_changes() {
        let mut checker = ConvergenceChecker::new();
        checker.start();
        
        let mut aux_files = HashMap::new();
        
        // Pass 1
        aux_files.insert(PathBuf::from("main.aux"), b"pass1".to_vec());
        checker.record_pass(&aux_files).unwrap();
        
        // Pass 2 - different
        aux_files.insert(PathBuf::from("main.aux"), b"pass2".to_vec());
        checker.record_pass(&aux_files).unwrap();
        
        // Pass 3 - same as pass 2
        let needs_more = checker.record_pass(&aux_files).unwrap();
        assert!(!needs_more);
        
        let result = checker.finalize();
        assert!(matches!(result, ConvergenceResult::Converged { passes: 3, .. }));
    }

    #[test]
    fn test_max_passes_exceeded() {
        let mut checker = ConvergenceChecker::with_limits(3, Duration::from_secs(300));
        checker.start();
        
        // Each pass has different content
        for i in 0..3 {
            let mut aux_files = HashMap::new();
            aux_files.insert(
                PathBuf::from("main.aux"),
                format!("pass{}", i).into_bytes(),
            );
            let result = checker.record_pass(&aux_files);
            
            if i == 2 {
                // Should fail on 3rd pass
                assert!(matches!(result, Err(ConvergenceError::MaxPassesExceeded { .. })));
            }
        }
    }

    #[test]
    fn test_oscillation_detection() {
        let mut checker = ConvergenceChecker::new();
        checker.start();
        
        let state_a: HashMap<PathBuf, Vec<u8>> = [(PathBuf::from("main.aux"), b"state_a".to_vec())]
            .into_iter()
            .collect();
        let state_b: HashMap<PathBuf, Vec<u8>> = [(PathBuf::from("main.aux"), b"state_b".to_vec())]
            .into_iter()
            .collect();
        
        // A -> B -> A (oscillation)
        checker.record_pass(&state_a).unwrap();
        checker.record_pass(&state_b).unwrap();
        let result = checker.record_pass(&state_a);
        
        assert!(matches!(result, Err(ConvergenceError::OscillationDetected)));
    }

    #[test]
    fn test_graceful_error_handling() {
        let mut checker = ConvergenceChecker::with_limits(2, Duration::from_secs(300));
        checker.start();
        
        let mut aux_files = HashMap::new();
        aux_files.insert(PathBuf::from("main.aux"), b"changing".to_vec());
        checker.record_pass(&aux_files).unwrap();
        
        aux_files.insert(PathBuf::from("main.aux"), b"still changing".to_vec());
        let err = checker.record_pass(&aux_files).unwrap_err();
        
        // Should produce partial result, not hard failure
        let result = checker.handle_error(err);
        assert!(matches!(result, ConvergenceResult::PartialConvergence { .. }));
        assert!(result.is_successful()); // Partial success is still success
    }

    #[test]
    fn test_aux_file_state() {
        let state1 = AuxFileState::from_content(b"test content");
        let state2 = AuxFileState::from_content(b"test content");
        let state3 = AuxFileState::from_content(b"different content");
        
        assert_eq!(state1, state2);
        assert_ne!(state1, state3);
    }

    #[test]
    fn test_multiple_files_convergence() {
        let mut checker = ConvergenceChecker::new();
        checker.start();
        
        let mut aux_files = HashMap::new();
        aux_files.insert(PathBuf::from("main.aux"), b"main".to_vec());
        aux_files.insert(PathBuf::from("chapter1.aux"), b"ch1".to_vec());
        aux_files.insert(PathBuf::from("chapter2.aux"), b"ch2".to_vec());
        
        checker.record_pass(&aux_files).unwrap();
        
        // Same content
        let needs_more = checker.record_pass(&aux_files).unwrap();
        assert!(!needs_more);
        
        let result = checker.finalize();
        assert!(result.is_successful());
    }
}
