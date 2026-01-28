//! FastTeX Build Orchestrator
//! 
//! This module implements the core build orchestration logic for FastTeX,
//! including dependency graph construction, MPI-style job scheduling,
//! and scatter/gather compilation coordination.
//!
//! ## Architecture
//!
//! The orchestrator supports two compilation modes:
//! - **Speculative Mode**: Fast parallel compilation with best-effort correctness
//! - **Guaranteed Mode**: Sequential compilation with guaranteed correctness
//!
//! ## Key Components
//!
//! - `dependency_graph`: Builds and analyzes document dependency DAGs
//! - `scheduler`: MPI-style scatter/gather job scheduling
//! - `fair_scheduler`: Weighted fair queuing for multi-tenant resource allocation
//! - `convergence`: Reference convergence detection and handling
//! - `circuit_breaker`: Poisoned job detection and quarantine
//! - `quotas`: Per-user resource quota enforcement
//! - `compilation_mode`: Hybrid compilation strategy implementation
//! - `cache`: Artifact caching for preambles, aux files, and PDFs
//! - `parser`: LaTeX document parsing for dependency extraction

pub mod dependency_graph;
pub mod scheduler;
pub mod cache;
pub mod parser;
pub mod quotas;
pub mod convergence;
pub mod circuit_breaker;
pub mod compilation_mode;
pub mod fair_scheduler;

pub use dependency_graph::{DependencyGraph, CompileNode, NodeId, NodeType};
pub use scheduler::{JobScheduler, CompileJob, JobStatus};
pub use cache::ArtifactCache;
pub use quotas::{QuotaManager, QuotaError, QuotaLimits, UserTier};
pub use convergence::{ConvergenceChecker, ConvergenceResult, ConvergenceError};
pub use circuit_breaker::{CircuitBreaker, CircuitCheckResult, QuarantineInfo};
pub use compilation_mode::{CompilationMode, CompilationStrategy, CompilationPlanner, CompilationResult};
pub use fair_scheduler::{FairScheduler, FairSchedulerConfig, ScheduledJob};
