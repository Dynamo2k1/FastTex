//! FastTeX Worker
//!
//! This module implements the Tectonic-based compilation worker that runs
//! inside Firecracker microVMs. It handles:
//! - Receiving compilation jobs
//! - Executing Tectonic
//! - Managing .fmt preamble files
//! - Returning compilation results

pub mod compiler;
pub mod protocol;

pub use compiler::{TectonicCompiler, CompileOptions, CompileResult};
pub use protocol::{WorkerRequest, WorkerResponse};
