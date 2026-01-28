//! FastTeX Build Orchestrator
//! 
//! This module implements the core build orchestration logic for FastTeX,
//! including dependency graph construction, MPI-style job scheduling,
//! and scatter/gather compilation coordination.

pub mod dependency_graph;
pub mod scheduler;
pub mod cache;
pub mod parser;

pub use dependency_graph::{DependencyGraph, CompileNode, NodeId, NodeType};
pub use scheduler::{JobScheduler, CompileJob, JobStatus};
pub use cache::ArtifactCache;
