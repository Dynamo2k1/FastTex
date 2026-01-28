//! MPI-Style Job Scheduler for Distributed LaTeX Compilation
//!
//! This module implements the scatter/gather job scheduling logic that
//! distributes compilation tasks across worker nodes and collects results.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::{mpsc, RwLock, Mutex};
use uuid::Uuid;

use crate::dependency_graph::{DependencyGraph, NodeId, NodeType, ContentHash};

/// Unique identifier for a compilation job
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JobId(pub Uuid);

impl JobId {
    pub fn new() -> Self {
        JobId(Uuid::new_v4())
    }
}

impl Default for JobId {
    fn default() -> Self {
        Self::new()
    }
}

/// Status of a compilation job
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobStatus {
    /// Job is waiting to be scheduled
    Pending,
    /// Job is queued for execution
    Queued,
    /// Job is currently being compiled
    Compiling,
    /// Job completed successfully
    Completed,
    /// Job failed
    Failed(String),
    /// Job was cancelled
    Cancelled,
}

/// Priority levels for jobs
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum JobPriority {
    /// Background compilation
    Low = 0,
    /// Normal compilation request
    Normal = 1,
    /// User-triggered recompile
    High = 2,
    /// Preamble compilation (needed by others)
    Critical = 3,
}

/// A compilation job to be executed by a worker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileJob {
    /// Unique job identifier
    pub id: JobId,
    /// Project identifier
    pub project_id: Uuid,
    /// Node in the dependency graph
    pub node_id: NodeId,
    /// Type of compilation
    pub node_type: NodeType,
    /// Path to the target file
    pub target_path: PathBuf,
    /// Content hash for cache lookup
    pub content_hash: ContentHash,
    /// Jobs this job depends on (must complete first)
    pub dependencies: Vec<JobId>,
    /// Current status
    pub status: JobStatus,
    /// Priority level
    pub priority: JobPriority,
    /// MPI-style rank (0 = master)
    pub rank: u32,
}

impl CompileJob {
    /// Create a new compile job from a graph node
    pub fn from_node(
        project_id: Uuid,
        node_id: NodeId,
        node_type: NodeType,
        target_path: PathBuf,
        content_hash: ContentHash,
        rank: u32,
    ) -> Self {
        let priority = match node_type {
            NodeType::Preamble => JobPriority::Critical,
            NodeType::MainDocument => JobPriority::High,
            _ => JobPriority::Normal,
        };

        CompileJob {
            id: JobId::new(),
            project_id,
            node_id,
            node_type,
            target_path,
            content_hash,
            dependencies: Vec::new(),
            status: JobStatus::Pending,
            priority,
            rank,
        }
    }
}

/// Result of a compilation job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobResult {
    /// Job identifier
    pub job_id: JobId,
    /// Whether compilation succeeded
    pub success: bool,
    /// PDF output (if applicable)
    pub pdf_path: Option<PathBuf>,
    /// Updated .aux files
    pub aux_files: HashMap<PathBuf, Vec<u8>>,
    /// Compilation log
    pub log: String,
    /// Warnings and errors
    pub diagnostics: Vec<Diagnostic>,
    /// Time taken
    pub duration: Duration,
}

/// A compilation diagnostic (error or warning)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub file: Option<PathBuf>,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
}

/// Error type for scheduler operations
#[derive(Error, Debug)]
pub enum SchedulerError {
    #[error("Job not found: {0:?}")]
    JobNotFound(JobId),
    
    #[error("No workers available")]
    NoWorkersAvailable,
    
    #[error("Dependency cycle detected")]
    DependencyCycle,
    
    #[error("Compilation failed: {0}")]
    CompilationFailed(String),
    
    #[error("Timeout waiting for workers")]
    Timeout,
    
    #[error("Graph error: {0}")]
    GraphError(#[from] crate::dependency_graph::GraphError),
}

/// Worker status
#[derive(Debug, Clone)]
pub struct WorkerInfo {
    pub id: Uuid,
    pub address: String,
    pub is_available: bool,
    pub current_job: Option<JobId>,
    pub jobs_completed: u64,
    pub last_heartbeat: Instant,
}

/// Configuration for the job scheduler
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// Maximum concurrent jobs
    pub max_concurrent_jobs: usize,
    /// Maximum time to wait for a worker
    pub worker_timeout: Duration,
    /// Maximum compilation time per job
    pub job_timeout: Duration,
    /// Maximum iterations for reference convergence
    pub max_convergence_iterations: u32,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        SchedulerConfig {
            max_concurrent_jobs: 16,
            worker_timeout: Duration::from_secs(30),
            job_timeout: Duration::from_secs(300),
            max_convergence_iterations: 5,
        }
    }
}

/// The main job scheduler implementing MPI-style scatter/gather
pub struct JobScheduler {
    /// Configuration
    config: SchedulerConfig,
    /// All jobs indexed by ID
    jobs: Arc<RwLock<HashMap<JobId, CompileJob>>>,
    /// Jobs waiting to be executed (priority queue)
    pending_queue: Arc<Mutex<VecDeque<JobId>>>,
    /// Jobs currently running
    running_jobs: Arc<RwLock<HashSet<JobId>>>,
    /// Completed jobs awaiting gather
    completed_jobs: Arc<RwLock<HashMap<JobId, JobResult>>>,
    /// Available workers
    workers: Arc<RwLock<Vec<WorkerInfo>>>,
    /// Channel for job completion notifications
    completion_tx: mpsc::Sender<JobResult>,
    completion_rx: Arc<Mutex<mpsc::Receiver<JobResult>>>,
}

impl JobScheduler {
    /// Create a new job scheduler
    pub fn new(config: SchedulerConfig) -> Self {
        let (tx, rx) = mpsc::channel(1000);
        
        JobScheduler {
            config,
            jobs: Arc::new(RwLock::new(HashMap::new())),
            pending_queue: Arc::new(Mutex::new(VecDeque::new())),
            running_jobs: Arc::new(RwLock::new(HashSet::new())),
            completed_jobs: Arc::new(RwLock::new(HashMap::new())),
            workers: Arc::new(RwLock::new(Vec::new())),
            completion_tx: tx,
            completion_rx: Arc::new(Mutex::new(rx)),
        }
    }

    /// Schedule jobs from a dependency graph
    /// 
    /// This implements the MPI-style scatter phase:
    /// 1. Analyze the dependency graph
    /// 2. Identify nodes needing recompilation
    /// 3. Create jobs in topological order
    /// 4. Queue jobs respecting dependencies
    pub async fn schedule_jobs(
        &self,
        project_id: Uuid,
        graph: &DependencyGraph,
    ) -> Result<Vec<JobId>, SchedulerError> {
        // Get nodes in dependency order (dependencies first)
        let parallel_groups = graph.parallel_groups()?;
        
        if parallel_groups.is_empty() {
            tracing::info!("No nodes need recompilation");
            return Ok(Vec::new());
        }
        
        let mut all_jobs = Vec::new();
        let mut node_to_job: HashMap<NodeId, JobId> = HashMap::new();
        let mut rank: u32 = 0;
        
        // Process groups in order (each group can run in parallel)
        for group in &parallel_groups {
            for &node_id in group {
                let node = graph.get_node(node_id)
                    .ok_or(SchedulerError::GraphError(
                        crate::dependency_graph::GraphError::NodeNotFound(node_id)
                    ))?;
                
                // Rank 0 is typically the preamble (master)
                let job_rank = if matches!(node.node_type, NodeType::Preamble) {
                    0
                } else {
                    rank += 1;
                    rank
                };
                
                let mut job = CompileJob::from_node(
                    project_id,
                    node_id,
                    node.node_type.clone(),
                    node.source_path.clone(),
                    node.content_hash.clone(),
                    job_rank,
                );
                
                // Set dependencies based on graph edges
                let deps = graph.dependencies(node_id)?;
                job.dependencies = deps
                    .iter()
                    .filter_map(|dep_id| node_to_job.get(dep_id).copied())
                    .collect();
                
                let job_id = job.id;
                node_to_job.insert(node_id, job_id);
                
                // Add to job store
                {
                    let mut jobs = self.jobs.write().await;
                    jobs.insert(job_id, job);
                }
                
                all_jobs.push(job_id);
            }
        }
        
        // Queue jobs that have no pending dependencies
        self.queue_ready_jobs(&all_jobs).await?;
        
        tracing::info!(
            "Scheduled {} jobs for project {}",
            all_jobs.len(),
            project_id
        );
        
        Ok(all_jobs)
    }

    /// Queue jobs that are ready to execute (all dependencies satisfied)
    async fn queue_ready_jobs(&self, job_ids: &[JobId]) -> Result<(), SchedulerError> {
        let jobs = self.jobs.read().await;
        let completed = self.completed_jobs.read().await;
        let mut queue = self.pending_queue.lock().await;
        
        let mut ready_jobs: Vec<(JobId, JobPriority)> = Vec::new();
        
        for &job_id in job_ids {
            if let Some(job) = jobs.get(&job_id) {
                if matches!(job.status, JobStatus::Pending) {
                    // Check if all dependencies are completed
                    let deps_satisfied = job.dependencies.iter()
                        .all(|dep_id| completed.contains_key(dep_id));
                    
                    if deps_satisfied {
                        ready_jobs.push((job_id, job.priority));
                    }
                }
            }
        }
        
        // Sort by priority (highest first)
        ready_jobs.sort_by(|a, b| b.1.cmp(&a.1));
        
        // Add to queue
        for (job_id, _) in ready_jobs {
            queue.push_back(job_id);
        }
        
        Ok(())
    }

    /// Get the next job to dispatch to a worker
    pub async fn get_next_job(&self) -> Option<CompileJob> {
        let mut queue = self.pending_queue.lock().await;
        let job_id = queue.pop_front()?;
        
        let mut jobs = self.jobs.write().await;
        let mut running = self.running_jobs.write().await;
        
        if let Some(job) = jobs.get_mut(&job_id) {
            job.status = JobStatus::Compiling;
            running.insert(job_id);
            Some(job.clone())
        } else {
            None
        }
    }

    /// Report job completion (gather phase)
    pub async fn report_completion(&self, result: JobResult) -> Result<(), SchedulerError> {
        let job_id = result.job_id;
        
        // Update job status
        {
            let mut jobs = self.jobs.write().await;
            if let Some(job) = jobs.get_mut(&job_id) {
                job.status = if result.success {
                    JobStatus::Completed
                } else {
                    JobStatus::Failed(result.log.clone())
                };
            }
        }
        
        // Move from running to completed
        {
            let mut running = self.running_jobs.write().await;
            running.remove(&job_id);
        }
        
        {
            let mut completed = self.completed_jobs.write().await;
            completed.insert(job_id, result);
        }
        
        // Check if any pending jobs are now ready
        let all_jobs: Vec<JobId> = {
            let jobs = self.jobs.read().await;
            jobs.keys().copied().collect()
        };
        
        self.queue_ready_jobs(&all_jobs).await?;
        
        Ok(())
    }

    /// Register a new worker
    pub async fn register_worker(&self, worker_id: Uuid, address: String) {
        let mut workers = self.workers.write().await;
        workers.push(WorkerInfo {
            id: worker_id,
            address,
            is_available: true,
            current_job: None,
            jobs_completed: 0,
            last_heartbeat: Instant::now(),
        });
    }

    /// Get statistics about the scheduler
    pub async fn stats(&self) -> SchedulerStats {
        let jobs = self.jobs.read().await;
        let running = self.running_jobs.read().await;
        let completed = self.completed_jobs.read().await;
        let queue = self.pending_queue.lock().await;
        let workers = self.workers.read().await;
        
        SchedulerStats {
            total_jobs: jobs.len(),
            pending_jobs: queue.len(),
            running_jobs: running.len(),
            completed_jobs: completed.len(),
            failed_jobs: jobs.values()
                .filter(|j| matches!(j.status, JobStatus::Failed(_)))
                .count(),
            available_workers: workers.iter().filter(|w| w.is_available).count(),
            total_workers: workers.len(),
        }
    }

    /// Check if all jobs for a project are complete
    pub async fn is_project_complete(&self, project_id: Uuid) -> bool {
        let jobs = self.jobs.read().await;
        let project_jobs: Vec<_> = jobs.values()
            .filter(|j| j.project_id == project_id)
            .collect();
        
        if project_jobs.is_empty() {
            return true;
        }
        
        project_jobs.iter().all(|j| {
            matches!(j.status, JobStatus::Completed | JobStatus::Failed(_))
        })
    }

    /// Get all results for a project (gather phase completion)
    pub async fn gather_results(&self, project_id: Uuid) -> Vec<JobResult> {
        let jobs = self.jobs.read().await;
        let completed = self.completed_jobs.read().await;
        
        jobs.values()
            .filter(|j| j.project_id == project_id)
            .filter_map(|j| completed.get(&j.id).cloned())
            .collect()
    }

    /// Check if .aux files have converged (references stabilized)
    pub fn check_aux_convergence(
        &self,
        previous_aux: &HashMap<PathBuf, Vec<u8>>,
        current_aux: &HashMap<PathBuf, Vec<u8>>,
    ) -> bool {
        if previous_aux.len() != current_aux.len() {
            return false;
        }
        
        for (path, prev_content) in previous_aux {
            match current_aux.get(path) {
                Some(curr_content) if prev_content == curr_content => continue,
                _ => return false,
            }
        }
        
        true
    }
}

/// Statistics about the scheduler state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerStats {
    pub total_jobs: usize,
    pub pending_jobs: usize,
    pub running_jobs: usize,
    pub completed_jobs: usize,
    pub failed_jobs: usize,
    pub available_workers: usize,
    pub total_workers: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dependency_graph::{CompileNode, DependencyGraph};

    fn create_test_node(name: &str, node_type: NodeType) -> CompileNode {
        CompileNode::new(
            node_type,
            PathBuf::from(format!("{}.tex", name)),
            name.as_bytes(),
        )
    }

    #[tokio::test]
    async fn test_schedule_single_job() {
        let scheduler = JobScheduler::new(SchedulerConfig::default());
        let mut graph = DependencyGraph::new(PathBuf::from("/project"));
        
        let main = create_test_node("main", NodeType::MainDocument);
        let main_id = graph.add_node(main);
        graph.set_root(main_id).unwrap();
        
        let jobs = scheduler.schedule_jobs(Uuid::new_v4(), &graph).await.unwrap();
        
        assert_eq!(jobs.len(), 1);
        
        let stats = scheduler.stats().await;
        assert_eq!(stats.total_jobs, 1);
        assert_eq!(stats.pending_jobs, 1);
    }

    #[tokio::test]
    async fn test_schedule_with_dependencies() {
        let scheduler = JobScheduler::new(SchedulerConfig::default());
        let mut graph = DependencyGraph::new(PathBuf::from("/project"));
        
        let preamble = create_test_node("preamble", NodeType::Preamble);
        let chapter1 = create_test_node("chapter1", NodeType::Chapter);
        let chapter2 = create_test_node("chapter2", NodeType::Chapter);
        let main = create_test_node("main", NodeType::MainDocument);
        
        let preamble_id = graph.add_node(preamble);
        let chapter1_id = graph.add_node(chapter1);
        let chapter2_id = graph.add_node(chapter2);
        let main_id = graph.add_node(main);
        
        graph.add_dependency(chapter1_id, preamble_id).unwrap();
        graph.add_dependency(chapter2_id, preamble_id).unwrap();
        graph.add_dependency(main_id, chapter1_id).unwrap();
        graph.add_dependency(main_id, chapter2_id).unwrap();
        graph.set_root(main_id).unwrap();
        
        let jobs = scheduler.schedule_jobs(Uuid::new_v4(), &graph).await.unwrap();
        
        assert_eq!(jobs.len(), 4);
        
        // Only preamble should be immediately queued
        let stats = scheduler.stats().await;
        assert_eq!(stats.pending_jobs, 1);
    }

    #[tokio::test]
    async fn test_job_completion_flow() {
        let scheduler = JobScheduler::new(SchedulerConfig::default());
        let mut graph = DependencyGraph::new(PathBuf::from("/project"));
        
        let preamble = create_test_node("preamble", NodeType::Preamble);
        let chapter = create_test_node("chapter", NodeType::Chapter);
        
        let preamble_id = graph.add_node(preamble);
        let chapter_id = graph.add_node(chapter);
        
        graph.add_dependency(chapter_id, preamble_id).unwrap();
        
        let project_id = Uuid::new_v4();
        let jobs = scheduler.schedule_jobs(project_id, &graph).await.unwrap();
        
        // Get first job (preamble)
        let job1 = scheduler.get_next_job().await.unwrap();
        assert!(matches!(job1.node_type, NodeType::Preamble));
        
        let stats = scheduler.stats().await;
        assert_eq!(stats.running_jobs, 1);
        assert_eq!(stats.pending_jobs, 0);
        
        // Complete preamble job
        scheduler.report_completion(JobResult {
            job_id: job1.id,
            success: true,
            pdf_path: None,
            aux_files: HashMap::new(),
            log: String::new(),
            diagnostics: Vec::new(),
            duration: Duration::from_secs(1),
        }).await.unwrap();
        
        // Chapter should now be queued
        let stats = scheduler.stats().await;
        assert_eq!(stats.completed_jobs, 1);
        assert_eq!(stats.pending_jobs, 1);
        
        // Get chapter job
        let job2 = scheduler.get_next_job().await.unwrap();
        assert!(matches!(job2.node_type, NodeType::Chapter));
    }

    #[test]
    fn test_aux_convergence() {
        let scheduler = JobScheduler::new(SchedulerConfig::default());
        
        let mut prev_aux = HashMap::new();
        prev_aux.insert(PathBuf::from("main.aux"), b"\\citation{foo}".to_vec());
        
        let mut curr_aux = HashMap::new();
        curr_aux.insert(PathBuf::from("main.aux"), b"\\citation{foo}".to_vec());
        
        assert!(scheduler.check_aux_convergence(&prev_aux, &curr_aux));
        
        curr_aux.insert(PathBuf::from("main.aux"), b"\\citation{bar}".to_vec());
        assert!(!scheduler.check_aux_convergence(&prev_aux, &curr_aux));
    }
}
