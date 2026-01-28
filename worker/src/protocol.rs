//! Worker Communication Protocol
//!
//! This module defines the request/response protocol for communication
//! between the orchestrator and worker processes.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::compiler::{CompileDiagnostic, DiagnosticSeverity};

/// A request from the orchestrator to a worker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerRequest {
    /// Unique request/job identifier
    pub job_id: Uuid,
    /// Project identifier
    pub project_id: Uuid,
    /// Type of request
    pub request_type: RequestType,
    /// Source files (path -> content)
    pub source_files: HashMap<PathBuf, Vec<u8>>,
    /// Precompiled format file (if available)
    pub fmt_file: Option<Vec<u8>>,
    /// Auxiliary files from previous runs
    pub aux_files: HashMap<PathBuf, Vec<u8>>,
    /// Main file to compile
    pub compile_target: PathBuf,
    /// Timeout for this job
    pub timeout: Duration,
    /// MPI-style rank (0 = master)
    pub rank: u32,
}

/// Type of compilation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RequestType {
    /// Generate a format file from preamble
    GenerateFormat {
        /// Preamble content
        preamble_content: String,
    },
    /// Compile a chapter with existing format
    CompileChapter,
    /// Compile a standalone figure
    CompileFigure,
    /// Full document compilation (no format)
    CompileFull,
    /// Run bibtex/biber
    ProcessBibliography,
}

/// Response from a worker to the orchestrator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerResponse {
    /// Job identifier (matches request)
    pub job_id: Uuid,
    /// Whether the operation succeeded
    pub success: bool,
    /// Status of the response
    pub status: ResponseStatus,
    /// PDF output (if applicable)
    pub pdf_output: Option<Vec<u8>>,
    /// Generated format file (if applicable)
    pub fmt_output: Option<Vec<u8>>,
    /// Updated auxiliary files
    pub aux_outputs: HashMap<PathBuf, Vec<u8>>,
    /// Compilation log
    pub log: String,
    /// Diagnostics (errors and warnings)
    pub diagnostics: Vec<CompileDiagnostic>,
    /// Time taken for compilation
    pub duration: Duration,
    /// Hash of outputs for cache validation
    pub output_hash: Option<String>,
    /// Worker statistics
    pub stats: WorkerStats,
}

/// Status of a worker response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResponseStatus {
    /// Compilation completed successfully
    Success,
    /// Compilation completed with warnings
    SuccessWithWarnings(usize),
    /// Compilation failed with errors
    Failed(String),
    /// Compilation timed out
    Timeout,
    /// Worker resource limits exceeded
    ResourceExceeded(String),
    /// Internal worker error
    InternalError(String),
}

/// Statistics from worker execution
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkerStats {
    /// Memory used (bytes)
    pub memory_used: u64,
    /// CPU time (milliseconds)
    pub cpu_time_ms: u64,
    /// Number of TeX passes required
    pub tex_passes: u32,
    /// Whether format file was used
    pub used_format: bool,
    /// Whether this was a cache hit
    pub cache_hit: bool,
}

impl WorkerRequest {
    /// Create a new format generation request
    pub fn new_format_request(
        project_id: Uuid,
        preamble_content: String,
        source_files: HashMap<PathBuf, Vec<u8>>,
    ) -> Self {
        WorkerRequest {
            job_id: Uuid::new_v4(),
            project_id,
            request_type: RequestType::GenerateFormat { preamble_content },
            source_files,
            fmt_file: None,
            aux_files: HashMap::new(),
            compile_target: PathBuf::from("preamble.tex"),
            timeout: Duration::from_secs(120),
            rank: 0, // Format generation is always rank 0 (master)
        }
    }

    /// Create a new chapter compilation request
    pub fn new_chapter_request(
        project_id: Uuid,
        chapter_path: PathBuf,
        source_files: HashMap<PathBuf, Vec<u8>>,
        fmt_file: Option<Vec<u8>>,
        aux_files: HashMap<PathBuf, Vec<u8>>,
        rank: u32,
    ) -> Self {
        WorkerRequest {
            job_id: Uuid::new_v4(),
            project_id,
            request_type: RequestType::CompileChapter,
            source_files,
            fmt_file,
            aux_files,
            compile_target: chapter_path,
            timeout: Duration::from_secs(300),
            rank,
        }
    }

    /// Create a new figure compilation request
    pub fn new_figure_request(
        project_id: Uuid,
        figure_path: PathBuf,
        source_files: HashMap<PathBuf, Vec<u8>>,
        rank: u32,
    ) -> Self {
        WorkerRequest {
            job_id: Uuid::new_v4(),
            project_id,
            request_type: RequestType::CompileFigure,
            source_files,
            fmt_file: None,
            aux_files: HashMap::new(),
            compile_target: figure_path,
            timeout: Duration::from_secs(180),
            rank,
        }
    }
}

impl WorkerResponse {
    /// Create a success response
    pub fn success(
        job_id: Uuid,
        pdf_output: Option<Vec<u8>>,
        aux_outputs: HashMap<PathBuf, Vec<u8>>,
        log: String,
        diagnostics: Vec<CompileDiagnostic>,
        duration: Duration,
    ) -> Self {
        let warning_count = diagnostics
            .iter()
            .filter(|d| matches!(d.severity, DiagnosticSeverity::Warning))
            .count();

        let status = if warning_count > 0 {
            ResponseStatus::SuccessWithWarnings(warning_count)
        } else {
            ResponseStatus::Success
        };

        WorkerResponse {
            job_id,
            success: true,
            status,
            pdf_output,
            fmt_output: None,
            aux_outputs,
            log,
            diagnostics,
            duration,
            output_hash: None,
            stats: WorkerStats::default(),
        }
    }

    /// Create a failure response
    pub fn failure(
        job_id: Uuid,
        error: String,
        log: String,
        diagnostics: Vec<CompileDiagnostic>,
        duration: Duration,
    ) -> Self {
        WorkerResponse {
            job_id,
            success: false,
            status: ResponseStatus::Failed(error),
            pdf_output: None,
            fmt_output: None,
            aux_outputs: HashMap::new(),
            log,
            diagnostics,
            duration,
            output_hash: None,
            stats: WorkerStats::default(),
        }
    }

    /// Create a timeout response
    pub fn timeout(job_id: Uuid, partial_log: String) -> Self {
        WorkerResponse {
            job_id,
            success: false,
            status: ResponseStatus::Timeout,
            pdf_output: None,
            fmt_output: None,
            aux_outputs: HashMap::new(),
            log: partial_log,
            diagnostics: vec![CompileDiagnostic {
                severity: DiagnosticSeverity::Error,
                message: "Compilation timed out".to_string(),
                file: None,
                line: None,
                column: None,
            }],
            duration: Duration::from_secs(0),
            output_hash: None,
            stats: WorkerStats::default(),
        }
    }
}

/// Message types for internal worker communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkerMessage {
    /// Heartbeat to indicate worker is alive
    Heartbeat { worker_id: Uuid },
    /// Worker is ready to accept jobs
    Ready { worker_id: Uuid },
    /// Worker is busy with a job
    Busy { worker_id: Uuid, job_id: Uuid },
    /// Progress update during long compilation
    Progress {
        job_id: Uuid,
        stage: CompileStage,
        progress: f32,
    },
    /// Shutdown request
    Shutdown { worker_id: Uuid },
}

/// Compilation stages for progress reporting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompileStage {
    Initializing,
    LoadingFormat,
    Compiling { pass: u32 },
    ProcessingBibliography,
    Finalizing,
    Complete,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_request_creation() {
        let project_id = Uuid::new_v4();
        let preamble = r"\documentclass{article}\usepackage{amsmath}".to_string();

        let request = WorkerRequest::new_format_request(project_id, preamble.clone(), HashMap::new());

        assert_eq!(request.project_id, project_id);
        assert_eq!(request.rank, 0);
        assert!(matches!(
            request.request_type,
            RequestType::GenerateFormat { preamble_content } if preamble_content == preamble
        ));
    }

    #[test]
    fn test_chapter_request_creation() {
        let project_id = Uuid::new_v4();
        let chapter_path = PathBuf::from("chapters/intro.tex");

        let request = WorkerRequest::new_chapter_request(
            project_id,
            chapter_path.clone(),
            HashMap::new(),
            Some(vec![1, 2, 3]),
            HashMap::new(),
            1,
        );

        assert_eq!(request.project_id, project_id);
        assert_eq!(request.compile_target, chapter_path);
        assert_eq!(request.rank, 1);
        assert!(request.fmt_file.is_some());
    }

    #[test]
    fn test_success_response() {
        let job_id = Uuid::new_v4();
        let diagnostics = vec![CompileDiagnostic {
            severity: DiagnosticSeverity::Warning,
            message: "Overfull hbox".to_string(),
            file: None,
            line: Some(42),
            column: None,
        }];

        let response = WorkerResponse::success(
            job_id,
            Some(vec![1, 2, 3]),
            HashMap::new(),
            "Compilation log".to_string(),
            diagnostics,
            Duration::from_secs(5),
        );

        assert!(response.success);
        assert!(matches!(
            response.status,
            ResponseStatus::SuccessWithWarnings(1)
        ));
    }

    #[test]
    fn test_failure_response() {
        let job_id = Uuid::new_v4();

        let response = WorkerResponse::failure(
            job_id,
            "Undefined control sequence".to_string(),
            "Error log".to_string(),
            Vec::new(),
            Duration::from_secs(1),
        );

        assert!(!response.success);
        assert!(matches!(response.status, ResponseStatus::Failed(_)));
    }

    #[test]
    fn test_timeout_response() {
        let job_id = Uuid::new_v4();

        let response = WorkerResponse::timeout(job_id, "Partial log...".to_string());

        assert!(!response.success);
        assert!(matches!(response.status, ResponseStatus::Timeout));
        assert_eq!(response.diagnostics.len(), 1);
    }

    #[test]
    fn test_serialization() {
        let project_id = Uuid::new_v4();
        let request = WorkerRequest::new_format_request(
            project_id,
            "\\documentclass{article}".to_string(),
            HashMap::new(),
        );

        // Test JSON serialization
        let json = serde_json::to_string(&request).unwrap();
        let deserialized: WorkerRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(request.job_id, deserialized.job_id);
        assert_eq!(request.project_id, deserialized.project_id);
    }
}
