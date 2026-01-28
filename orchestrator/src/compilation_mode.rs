//! Compilation Mode Management
//!
//! This module implements the hybrid compilation strategy with two modes:
//! - Speculative Parallel Mode: Fast, best-effort parallel compilation
//! - Guaranteed Linear Mode: Correct, sequential compilation
//!
//! Users explicitly choose their preferred trade-off between speed and correctness.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::convergence::{ConvergenceResult, MAX_CONVERGENCE_PASSES};
use crate::dependency_graph::{DependencyGraph, NodeId, NodeType};

/// Estimated time per compilation node in seconds
/// This is a conservative estimate used for planning
const ESTIMATED_SECONDS_PER_NODE: u64 = 10;

/// Overhead in seconds for parallel batch coordination
const PARALLEL_BATCH_OVERHEAD_SECS: u64 = 2;

/// Compilation mode selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompilationMode {
    /// Fast parallel compilation with best-effort correctness
    /// - Chapters compiled in parallel
    /// - May have minor reference inconsistencies
    /// - Suitable for development and iteration
    Speculative,
    
    /// Guaranteed correct sequential compilation
    /// - Full document compiled sequentially
    /// - All references guaranteed correct
    /// - Required for final output
    Guaranteed,
    
    /// Automatic mode selection based on document characteristics
    /// - Uses speculative for simple documents
    /// - Falls back to guaranteed for complex cross-references
    Auto,
}

impl Default for CompilationMode {
    fn default() -> Self {
        CompilationMode::Auto
    }
}

/// Characteristics of a document affecting mode selection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentCharacteristics {
    /// Number of chapters/includes
    pub chapter_count: usize,
    /// Whether document has bibliography
    pub has_bibliography: bool,
    /// Whether document has index
    pub has_index: bool,
    /// Estimated cross-reference complexity (0-1)
    pub reference_complexity: f32,
    /// Whether document uses problematic packages
    pub has_problematic_packages: bool,
    /// Total document size in bytes
    pub total_size: u64,
}

impl DocumentCharacteristics {
    /// Analyze a dependency graph to extract document characteristics
    pub fn from_graph(graph: &DependencyGraph) -> Self {
        let mut chapter_count = 0;
        let mut has_bibliography = false;
        let has_problematic_packages = false;
        
        for node in graph.nodes() {
            match node.node_type {
                NodeType::Chapter => chapter_count += 1,
                NodeType::Bibliography => has_bibliography = true,
                _ => {}
            }
        }
        
        // Estimate reference complexity based on structure
        let reference_complexity = if has_bibliography {
            0.7
        } else if chapter_count > 5 {
            0.5
        } else {
            0.2
        };
        
        DocumentCharacteristics {
            chapter_count,
            has_bibliography,
            has_index: false, // Would be detected by parser
            reference_complexity,
            has_problematic_packages,
            total_size: 0,
        }
    }
    
    /// Determine the recommended compilation mode
    pub fn recommended_mode(&self) -> CompilationMode {
        // Use guaranteed mode for complex documents
        if self.has_bibliography && self.chapter_count > 3 {
            return CompilationMode::Guaranteed;
        }
        
        if self.reference_complexity > 0.6 {
            return CompilationMode::Guaranteed;
        }
        
        if self.has_problematic_packages {
            return CompilationMode::Guaranteed;
        }
        
        // Simple documents can use speculative mode
        CompilationMode::Speculative
    }
}

/// Strategy for executing a compilation
#[derive(Debug, Clone)]
pub struct CompilationStrategy {
    /// Selected mode
    pub mode: CompilationMode,
    /// Maximum parallel workers to use
    pub max_parallel_workers: usize,
    /// Whether to use cached format files
    pub use_format_cache: bool,
    /// Maximum convergence passes
    pub max_passes: u32,
    /// Whether to allow partial results
    pub allow_partial_results: bool,
    /// Timeout for the entire compilation
    pub total_timeout: Duration,
}

impl CompilationStrategy {
    /// Create a speculative (parallel) strategy
    pub fn speculative(max_workers: usize) -> Self {
        CompilationStrategy {
            mode: CompilationMode::Speculative,
            max_parallel_workers: max_workers,
            use_format_cache: true,
            max_passes: 3, // Fewer passes for speed
            allow_partial_results: true,
            total_timeout: Duration::from_secs(5 * 60), // 5 min
        }
    }
    
    /// Create a guaranteed (linear) strategy
    pub fn guaranteed() -> Self {
        CompilationStrategy {
            mode: CompilationMode::Guaranteed,
            max_parallel_workers: 1, // Sequential
            use_format_cache: true,
            max_passes: MAX_CONVERGENCE_PASSES,
            allow_partial_results: false,
            total_timeout: Duration::from_secs(20 * 60), // 20 min
        }
    }
    
    /// Create strategy based on auto-detection
    pub fn auto(characteristics: &DocumentCharacteristics, max_workers: usize) -> Self {
        let mode = characteristics.recommended_mode();
        match mode {
            CompilationMode::Speculative | CompilationMode::Auto => Self::speculative(max_workers),
            CompilationMode::Guaranteed => Self::guaranteed(),
        }
    }
    
    /// Check if this strategy allows parallel compilation
    pub fn is_parallel(&self) -> bool {
        self.max_parallel_workers > 1
    }
}

/// Execution plan for a compilation
#[derive(Debug, Clone)]
pub struct CompilationPlan {
    /// Project ID
    pub project_id: Uuid,
    /// Compilation strategy
    pub strategy: CompilationStrategy,
    /// Ordered phases of compilation
    pub phases: Vec<CompilationPhase>,
    /// Estimated duration
    pub estimated_duration: Duration,
}

/// A phase in the compilation process
#[derive(Debug, Clone)]
pub struct CompilationPhase {
    /// Phase name
    pub name: String,
    /// Nodes to compile in this phase
    pub nodes: Vec<NodeId>,
    /// Whether nodes can be compiled in parallel
    pub parallel: bool,
    /// Dependencies (previous phases that must complete)
    pub depends_on: Vec<usize>,
}

/// Builder for creating compilation plans
pub struct CompilationPlanner {
    /// Default maximum workers
    default_max_workers: usize,
}

impl CompilationPlanner {
    /// Create a new planner
    pub fn new(default_max_workers: usize) -> Self {
        CompilationPlanner {
            default_max_workers,
        }
    }
    
    /// Create a compilation plan for a project
    pub fn plan(
        &self,
        project_id: Uuid,
        graph: &DependencyGraph,
        mode: CompilationMode,
    ) -> CompilationPlan {
        let characteristics = DocumentCharacteristics::from_graph(graph);
        
        let strategy = match mode {
            CompilationMode::Auto => CompilationStrategy::auto(&characteristics, self.default_max_workers),
            CompilationMode::Speculative => CompilationStrategy::speculative(self.default_max_workers),
            CompilationMode::Guaranteed => CompilationStrategy::guaranteed(),
        };
        
        let phases = self.build_phases(graph, &strategy);
        let estimated_duration = self.estimate_duration(&phases, &strategy);
        
        CompilationPlan {
            project_id,
            strategy,
            phases,
            estimated_duration,
        }
    }
    
    /// Build compilation phases from the dependency graph
    fn build_phases(
        &self,
        graph: &DependencyGraph,
        strategy: &CompilationStrategy,
    ) -> Vec<CompilationPhase> {
        let mut phases = Vec::new();
        
        // Collect nodes by type
        let mut preamble_nodes = Vec::new();
        let mut chapter_nodes = Vec::new();
        let mut bib_nodes = Vec::new();
        let mut main_nodes = Vec::new();
        
        for node in graph.nodes() {
            match node.node_type {
                NodeType::Preamble => preamble_nodes.push(node.id),
                NodeType::Chapter => chapter_nodes.push(node.id),
                NodeType::Bibliography => bib_nodes.push(node.id),
                NodeType::MainDocument => main_nodes.push(node.id),
                _ => {}
            }
        }
        
        // Phase 1: Preamble (always first, sequential)
        if !preamble_nodes.is_empty() {
            phases.push(CompilationPhase {
                name: "Preamble Generation".to_string(),
                nodes: preamble_nodes,
                parallel: false,
                depends_on: vec![],
            });
        }
        
        // Phase 2: Chapters (parallel in speculative mode)
        if !chapter_nodes.is_empty() {
            let parallel = strategy.is_parallel();
            phases.push(CompilationPhase {
                name: if parallel { "Chapter Compilation (Parallel)" } else { "Chapter Compilation" }.to_string(),
                nodes: chapter_nodes,
                parallel,
                depends_on: if phases.is_empty() { vec![] } else { vec![0] },
            });
        }
        
        // Phase 3: Bibliography (sequential)
        if !bib_nodes.is_empty() {
            let chapter_phase = phases.len().saturating_sub(1);
            phases.push(CompilationPhase {
                name: "Bibliography Processing".to_string(),
                nodes: bib_nodes,
                parallel: false,
                depends_on: vec![chapter_phase],
            });
        }
        
        // Phase 4: Main document merge (sequential)
        if !main_nodes.is_empty() {
            let prev_phase = phases.len().saturating_sub(1);
            phases.push(CompilationPhase {
                name: "Final Merge".to_string(),
                nodes: main_nodes,
                parallel: false,
                depends_on: vec![prev_phase],
            });
        }
        
        phases
    }
    
    /// Estimate total compilation duration
    fn estimate_duration(
        &self,
        phases: &[CompilationPhase],
        strategy: &CompilationStrategy,
    ) -> Duration {
        let mut total = Duration::ZERO;
        
        for phase in phases {
            // Base time per node (conservative estimate)
            let time_per_node = Duration::from_secs(ESTIMATED_SECONDS_PER_NODE);
            
            let phase_time = if phase.parallel && strategy.is_parallel() {
                // Parallel: max(times) + overhead
                let parallel_batches = (phase.nodes.len() + strategy.max_parallel_workers - 1) 
                    / strategy.max_parallel_workers;
                time_per_node * parallel_batches as u32 + Duration::from_secs(PARALLEL_BATCH_OVERHEAD_SECS)
            } else {
                // Sequential: sum(times)
                time_per_node * phase.nodes.len() as u32
            };
            
            total += phase_time;
        }
        
        // Add convergence passes
        total *= strategy.max_passes;
        
        total
    }
}

impl Default for CompilationPlanner {
    fn default() -> Self {
        Self::new(4)
    }
}

/// Result of a compilation with mode information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilationResult {
    /// Whether compilation succeeded
    pub success: bool,
    /// Mode used for compilation
    pub mode: CompilationMode,
    /// Convergence result
    pub convergence: ConvergenceResult,
    /// Warnings about potential issues
    pub warnings: Vec<String>,
    /// Path to output PDF
    pub pdf_path: Option<PathBuf>,
    /// Total duration
    pub duration: Duration,
    /// Per-phase timings
    pub phase_timings: HashMap<String, Duration>,
}

impl CompilationResult {
    /// Check if the result has warnings about correctness
    pub fn has_correctness_warnings(&self) -> bool {
        !matches!(self.convergence, ConvergenceResult::Converged { .. })
    }
    
    /// Get a summary message for the user
    pub fn summary(&self) -> String {
        if !self.success {
            return "Compilation failed".to_string();
        }
        
        let mode_str = match self.mode {
            CompilationMode::Speculative => "speculative (parallel)",
            CompilationMode::Guaranteed => "guaranteed (sequential)",
            CompilationMode::Auto => "auto-selected",
        };
        
        let convergence_str = match &self.convergence {
            ConvergenceResult::Converged { passes, .. } => 
                format!("converged in {} passes", passes),
            ConvergenceResult::PartialConvergence { passes, warning, .. } =>
                format!("partial convergence ({} passes). {}", passes, warning),
            ConvergenceResult::Failed { error, .. } =>
                format!("convergence failed: {}", error),
        };
        
        format!(
            "Compilation successful ({}): {} in {:?}",
            mode_str,
            convergence_str,
            self.duration
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dependency_graph::CompileNode;

    fn create_test_graph() -> DependencyGraph {
        let mut graph = DependencyGraph::new(PathBuf::from("/project"));
        
        let preamble = CompileNode::new(
            NodeType::Preamble,
            PathBuf::from("preamble.tex"),
            b"preamble",
        );
        let chapter1 = CompileNode::new(
            NodeType::Chapter,
            PathBuf::from("ch1.tex"),
            b"chapter1",
        );
        let chapter2 = CompileNode::new(
            NodeType::Chapter,
            PathBuf::from("ch2.tex"),
            b"chapter2",
        );
        let main = CompileNode::new(
            NodeType::MainDocument,
            PathBuf::from("main.tex"),
            b"main",
        );
        
        let preamble_id = graph.add_node(preamble);
        let ch1_id = graph.add_node(chapter1);
        let ch2_id = graph.add_node(chapter2);
        let main_id = graph.add_node(main);
        
        graph.add_dependency(ch1_id, preamble_id).unwrap();
        graph.add_dependency(ch2_id, preamble_id).unwrap();
        graph.add_dependency(main_id, ch1_id).unwrap();
        graph.add_dependency(main_id, ch2_id).unwrap();
        
        graph
    }

    #[test]
    fn test_document_characteristics() {
        let graph = create_test_graph();
        let chars = DocumentCharacteristics::from_graph(&graph);
        
        assert_eq!(chars.chapter_count, 2);
        assert!(!chars.has_bibliography);
    }

    #[test]
    fn test_mode_recommendation_simple() {
        let chars = DocumentCharacteristics {
            chapter_count: 2,
            has_bibliography: false,
            has_index: false,
            reference_complexity: 0.2,
            has_problematic_packages: false,
            total_size: 10000,
        };
        
        assert_eq!(chars.recommended_mode(), CompilationMode::Speculative);
    }

    #[test]
    fn test_mode_recommendation_complex() {
        let chars = DocumentCharacteristics {
            chapter_count: 5,
            has_bibliography: true,
            has_index: true,
            reference_complexity: 0.8,
            has_problematic_packages: false,
            total_size: 100000,
        };
        
        assert_eq!(chars.recommended_mode(), CompilationMode::Guaranteed);
    }

    #[test]
    fn test_speculative_strategy() {
        let strategy = CompilationStrategy::speculative(4);
        
        assert_eq!(strategy.mode, CompilationMode::Speculative);
        assert_eq!(strategy.max_parallel_workers, 4);
        assert!(strategy.allow_partial_results);
        assert!(strategy.is_parallel());
    }

    #[test]
    fn test_guaranteed_strategy() {
        let strategy = CompilationStrategy::guaranteed();
        
        assert_eq!(strategy.mode, CompilationMode::Guaranteed);
        assert_eq!(strategy.max_parallel_workers, 1);
        assert!(!strategy.allow_partial_results);
        assert!(!strategy.is_parallel());
    }

    #[test]
    fn test_compilation_plan() {
        let graph = create_test_graph();
        let planner = CompilationPlanner::new(4);
        
        let plan = planner.plan(Uuid::new_v4(), &graph, CompilationMode::Speculative);
        
        assert_eq!(plan.strategy.mode, CompilationMode::Speculative);
        assert!(!plan.phases.is_empty());
        
        // First phase should be preamble
        assert!(plan.phases[0].name.contains("Preamble"));
        assert!(!plan.phases[0].parallel);
    }

    #[test]
    fn test_compilation_result_summary() {
        let result = CompilationResult {
            success: true,
            mode: CompilationMode::Speculative,
            convergence: ConvergenceResult::Converged {
                passes: 2,
                duration: Duration::from_secs(10),
            },
            warnings: vec![],
            pdf_path: Some(PathBuf::from("output.pdf")),
            duration: Duration::from_secs(15),
            phase_timings: HashMap::new(),
        };
        
        let summary = result.summary();
        assert!(summary.contains("successful"));
        assert!(summary.contains("speculative"));
        assert!(summary.contains("converged"));
    }
}
