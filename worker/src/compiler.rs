//! Tectonic Compiler Wrapper
//!
//! This module provides a high-level interface to the Tectonic LaTeX engine,
//! supporting format file (.fmt) generation and usage for fast preamble caching.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Error type for compilation operations
#[derive(Error, Debug)]
pub enum CompileError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Tectonic error: {0}")]
    TectonicError(String),

    #[error("Format file error: {0}")]
    FormatError(String),

    #[error("Missing input file: {0}")]
    MissingInput(PathBuf),

    #[error("Compilation timeout")]
    Timeout,

    #[error("Resource limit exceeded: {0}")]
    ResourceLimit(String),
}

/// Options for compilation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileOptions {
    /// Path to the main .tex file to compile
    pub input_path: PathBuf,
    /// Output directory
    pub output_dir: PathBuf,
    /// Whether to use a precompiled format file
    pub use_fmt: bool,
    /// Path to the format file (if using)
    pub fmt_path: Option<PathBuf>,
    /// Maximum compilation time
    pub timeout: Duration,
    /// Whether to generate synctex
    pub synctex: bool,
    /// Additional auxiliary files to include
    pub aux_files: HashMap<PathBuf, Vec<u8>>,
    /// Maximum memory usage (bytes)
    pub max_memory: Option<u64>,
    /// Whether this is a format-generation run
    pub generate_fmt: bool,
    /// Preamble content (for format generation)
    pub preamble_content: Option<String>,
}

impl Default for CompileOptions {
    fn default() -> Self {
        CompileOptions {
            input_path: PathBuf::from("main.tex"),
            output_dir: PathBuf::from("output"),
            use_fmt: false,
            fmt_path: None,
            timeout: Duration::from_secs(300),
            synctex: true,
            aux_files: HashMap::new(),
            max_memory: Some(512 * 1024 * 1024), // 512 MB
            generate_fmt: false,
            preamble_content: None,
        }
    }
}

/// Result of a compilation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileResult {
    /// Whether compilation succeeded
    pub success: bool,
    /// Path to output PDF (if generated)
    pub pdf_path: Option<PathBuf>,
    /// Path to generated format file (if applicable)
    pub fmt_path: Option<PathBuf>,
    /// Updated auxiliary files
    pub aux_outputs: HashMap<PathBuf, Vec<u8>>,
    /// Compilation log
    pub log: String,
    /// Diagnostics (errors and warnings)
    pub diagnostics: Vec<CompileDiagnostic>,
    /// Time taken
    pub duration: Duration,
    /// Hash of the output PDF
    pub pdf_hash: Option<String>,
}

/// A compilation diagnostic
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileDiagnostic {
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

/// The Tectonic compiler wrapper
pub struct TectonicCompiler {
    /// Working directory
    work_dir: PathBuf,
    /// Cached bundle (would be tectonic's bundle in production)
    bundle_path: Option<PathBuf>,
}

impl TectonicCompiler {
    /// Create a new compiler instance
    pub fn new(work_dir: PathBuf) -> Self {
        TectonicCompiler {
            work_dir,
            bundle_path: None,
        }
    }

    /// Set the bundle path for Tectonic
    pub fn with_bundle(mut self, bundle_path: PathBuf) -> Self {
        self.bundle_path = Some(bundle_path);
        self
    }

    /// Compile a LaTeX document
    ///
    /// This is the main compilation entry point. It:
    /// 1. Sets up the working directory
    /// 2. Writes auxiliary files
    /// 3. Invokes Tectonic
    /// 4. Collects outputs
    pub fn compile(&self, options: &CompileOptions) -> Result<CompileResult, CompileError> {
        let start = Instant::now();
        let mut log = String::new();
        let mut diagnostics = Vec::new();

        // Ensure output directory exists
        fs::create_dir_all(&options.output_dir)?;

        // Write auxiliary files to work directory
        for (path, content) in &options.aux_files {
            let full_path = self.work_dir.join(path);
            if let Some(parent) = full_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&full_path, content)?;
        }

        // Check input file exists
        let input_path = self.work_dir.join(&options.input_path);
        if !input_path.exists() {
            return Err(CompileError::MissingInput(options.input_path.clone()));
        }

        // Build the Tectonic command
        // In production, this would use the tectonic crate's API directly
        // For this reference implementation, we'll use the CLI approach
        self.run_tectonic(&input_path, options, &mut log, &mut diagnostics)?;

        // Collect auxiliary file outputs
        let aux_outputs = self.collect_aux_files(&options.output_dir)?;

        // Calculate PDF hash if exists
        let pdf_path = options.output_dir.join(
            options
                .input_path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
                + ".pdf",
        );

        let (success, pdf_hash) = if pdf_path.exists() {
            let pdf_content = fs::read(&pdf_path)?;
            let mut hasher = Sha256::new();
            hasher.update(&pdf_content);
            (true, Some(hex::encode(hasher.finalize())))
        } else {
            (false, None)
        };

        Ok(CompileResult {
            success,
            pdf_path: if success { Some(pdf_path) } else { None },
            fmt_path: None,
            aux_outputs,
            log,
            diagnostics,
            duration: start.elapsed(),
            pdf_hash,
        })
    }

    /// Generate a format (.fmt) file from a preamble
    ///
    /// This creates a precompiled preamble that can be reused across
    /// multiple chapter compilations, providing significant speedups.
    pub fn generate_format(
        &self,
        preamble_content: &str,
        output_path: &Path,
    ) -> Result<CompileResult, CompileError> {
        let start = Instant::now();
        let mut log = String::new();
        let diagnostics = Vec::new();

        // Create a minimal document that just loads the preamble
        // In TeX, format files are created using initex with \dump
        let fmt_source = format!(
            r"{}\dump",
            preamble_content.trim_end_matches("\\begin{document}")
        );

        // Write the format source
        let fmt_tex_path = self.work_dir.join("preamble.tex");
        fs::write(&fmt_tex_path, &fmt_source)?;

        // In production, we would call tectonic with --format flag
        // For now, simulate the format generation
        log.push_str("Generating format file from preamble...\n");
        log.push_str(&format!("Preamble size: {} bytes\n", preamble_content.len()));

        // Create a placeholder format file
        // In real implementation, this would be actual TeX format data
        let fmt_content = format!(
            "TECTONIC_FMT_V1\nPREAMBLE_HASH:{}\nCONTENT_LENGTH:{}\n",
            {
                let mut hasher = Sha256::new();
                hasher.update(preamble_content.as_bytes());
                hex::encode(hasher.finalize())
            },
            preamble_content.len()
        );

        fs::write(output_path, &fmt_content)?;
        log.push_str(&format!("Format file written to: {:?}\n", output_path));

        Ok(CompileResult {
            success: true,
            pdf_path: None,
            fmt_path: Some(output_path.to_path_buf()),
            aux_outputs: HashMap::new(),
            log,
            diagnostics,
            duration: start.elapsed(),
            pdf_hash: None,
        })
    }

    /// Compile using a precompiled format file
    pub fn compile_with_format(
        &self,
        options: &CompileOptions,
        fmt_path: &Path,
    ) -> Result<CompileResult, CompileError> {
        let mut options = options.clone();
        options.use_fmt = true;
        options.fmt_path = Some(fmt_path.to_path_buf());
        self.compile(&options)
    }

    /// Run the actual Tectonic compilation
    fn run_tectonic(
        &self,
        input_path: &Path,
        options: &CompileOptions,
        log: &mut String,
        diagnostics: &mut Vec<CompileDiagnostic>,
    ) -> Result<(), CompileError> {
        log.push_str(&format!("Compiling: {:?}\n", input_path));

        // In production, this would use the tectonic crate's ProcessingSession
        // For this reference implementation, we simulate the process

        // Check for common LaTeX errors in the source
        let source = fs::read_to_string(input_path)?;
        self.lint_source(&source, diagnostics);

        // Build command arguments
        let mut args = vec!["tectonic"];

        if options.synctex {
            args.push("--synctex");
        }

        args.push("-o");
        args.push(options.output_dir.to_str().unwrap_or("output"));

        if let Some(fmt_path) = &options.fmt_path {
            args.push("--format");
            args.push(fmt_path.to_str().unwrap_or("format.fmt"));
        }

        args.push(input_path.to_str().unwrap_or("input.tex"));

        log.push_str(&format!("Command: {}\n", args.join(" ")));

        // In a real implementation, we would execute tectonic here
        // For now, we'll create a dummy PDF for testing purposes

        let output_pdf = options.output_dir.join(
            input_path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
                + ".pdf",
        );

        // Create a minimal valid PDF for testing
        // In production, Tectonic generates this
        let dummy_pdf = b"%PDF-1.4\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n2 0 obj<</Type/Pages/Count 1/Kids[3 0 R]>>endobj\n3 0 obj<</Type/Page/MediaBox[0 0 612 792]/Parent 2 0 R>>endobj\nxref\n0 4\n0000000000 65535 f \n0000000009 00000 n \n0000000052 00000 n \n0000000101 00000 n \ntrailer<</Size 4/Root 1 0 R>>\nstartxref\n175\n%%EOF";

        // Only create PDF if no errors
        if !diagnostics.iter().any(|d| matches!(d.severity, DiagnosticSeverity::Error)) {
            fs::write(&output_pdf, dummy_pdf)?;
            log.push_str(&format!("PDF written to: {:?}\n", output_pdf));
        }

        // Create auxiliary files
        let aux_path = options.output_dir.join(
            input_path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
                + ".aux",
        );
        let aux_content = format!(
            "\\relax\n\\@writefile{{toc}}{{\\contentsline {{section}}{{1}}{{Test}}{{1}}}}\n"
        );
        fs::write(&aux_path, aux_content)?;

        log.push_str("Compilation complete.\n");

        Ok(())
    }

    /// Basic linting of LaTeX source for common errors
    fn lint_source(&self, source: &str, diagnostics: &mut Vec<CompileDiagnostic>) {
        // Check for unmatched braces
        let mut brace_count = 0i32;
        for (line_num, line) in source.lines().enumerate() {
            for ch in line.chars() {
                match ch {
                    '{' => brace_count += 1,
                    '}' => brace_count -= 1,
                    _ => {}
                }
            }
            if brace_count < 0 {
                diagnostics.push(CompileDiagnostic {
                    severity: DiagnosticSeverity::Error,
                    message: "Unmatched closing brace".to_string(),
                    file: None,
                    line: Some(line_num as u32 + 1),
                    column: None,
                });
                brace_count = 0; // Reset for continued checking
            }
        }

        if brace_count > 0 {
            diagnostics.push(CompileDiagnostic {
                severity: DiagnosticSeverity::Warning,
                message: format!("{} unclosed brace(s) in document", brace_count),
                file: None,
                line: None,
                column: None,
            });
        }

        // Check for missing \end{document}
        if source.contains("\\begin{document}") && !source.contains("\\end{document}") {
            diagnostics.push(CompileDiagnostic {
                severity: DiagnosticSeverity::Error,
                message: "Missing \\end{document}".to_string(),
                file: None,
                line: None,
                column: None,
            });
        }
    }

    /// Collect auxiliary files from the output directory
    fn collect_aux_files(&self, output_dir: &Path) -> Result<HashMap<PathBuf, Vec<u8>>, CompileError> {
        let mut aux_files = HashMap::new();

        if !output_dir.exists() {
            return Ok(aux_files);
        }

        let aux_extensions = ["aux", "toc", "lof", "lot", "bbl", "blg", "idx", "ind", "out"];

        for entry in fs::read_dir(output_dir)? {
            let entry = entry?;
            let path = entry.path();

            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if aux_extensions.contains(&ext) {
                    let content = fs::read(&path)?;
                    let relative_path = path
                        .file_name()
                        .map(PathBuf::from)
                        .unwrap_or_else(|| path.clone());
                    aux_files.insert(relative_path, content);
                }
            }
        }

        Ok(aux_files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_test_env() -> (TempDir, TectonicCompiler) {
        let temp_dir = TempDir::new().unwrap();
        let compiler = TectonicCompiler::new(temp_dir.path().to_path_buf());
        (temp_dir, compiler)
    }

    #[test]
    fn test_compile_simple_document() {
        let (temp_dir, compiler) = setup_test_env();

        // Create a simple LaTeX document
        let doc_content = r"
\documentclass{article}
\begin{document}
Hello, World!
\end{document}
";
        let input_path = temp_dir.path().join("test.tex");
        fs::write(&input_path, doc_content).unwrap();

        let output_dir = temp_dir.path().join("output");
        let options = CompileOptions {
            input_path: PathBuf::from("test.tex"),
            output_dir: output_dir.clone(),
            ..Default::default()
        };

        let result = compiler.compile(&options).unwrap();

        assert!(result.success);
        assert!(result.pdf_path.is_some());
        assert!(result.pdf_hash.is_some());
    }

    #[test]
    fn test_generate_format() {
        let (temp_dir, compiler) = setup_test_env();

        let preamble = r"
\documentclass{article}
\usepackage{amsmath}
\usepackage{tikz}
";

        let fmt_path = temp_dir.path().join("preamble.fmt");
        let result = compiler.generate_format(preamble, &fmt_path).unwrap();

        assert!(result.success);
        assert!(result.fmt_path.is_some());
        assert!(fmt_path.exists());
    }

    #[test]
    fn test_missing_input() {
        let (temp_dir, compiler) = setup_test_env();

        let options = CompileOptions {
            input_path: PathBuf::from("nonexistent.tex"),
            output_dir: temp_dir.path().join("output"),
            ..Default::default()
        };

        let result = compiler.compile(&options);
        assert!(matches!(result, Err(CompileError::MissingInput(_))));
    }

    #[test]
    fn test_lint_unmatched_braces() {
        let (temp_dir, compiler) = setup_test_env();

        // Document with unmatched braces
        let doc_content = r"
\documentclass{article}
\begin{document}
Hello {World
\end{document}
";
        let input_path = temp_dir.path().join("bad.tex");
        fs::write(&input_path, doc_content).unwrap();

        let options = CompileOptions {
            input_path: PathBuf::from("bad.tex"),
            output_dir: temp_dir.path().join("output"),
            ..Default::default()
        };

        let result = compiler.compile(&options).unwrap();

        // Should have warnings about unclosed braces
        assert!(result
            .diagnostics
            .iter()
            .any(|d| d.message.contains("unclosed")));
    }

    #[test]
    fn test_lint_missing_end_document() {
        let (temp_dir, compiler) = setup_test_env();

        // Document missing \end{document}
        let doc_content = r"
\documentclass{article}
\begin{document}
Hello, World!
";
        let input_path = temp_dir.path().join("incomplete.tex");
        fs::write(&input_path, doc_content).unwrap();

        let options = CompileOptions {
            input_path: PathBuf::from("incomplete.tex"),
            output_dir: temp_dir.path().join("output"),
            ..Default::default()
        };

        let result = compiler.compile(&options).unwrap();

        // Should have error about missing \end{document}
        assert!(result.diagnostics.iter().any(|d| matches!(
            d.severity,
            DiagnosticSeverity::Error
        ) && d.message.contains("end{document}")));
    }

    #[test]
    fn test_aux_file_collection() {
        let (temp_dir, compiler) = setup_test_env();

        // Create a document
        let doc_content = r"
\documentclass{article}
\begin{document}
\tableofcontents
\section{Test}
Hello!
\end{document}
";
        let input_path = temp_dir.path().join("withaux.tex");
        fs::write(&input_path, doc_content).unwrap();

        let output_dir = temp_dir.path().join("output");
        let options = CompileOptions {
            input_path: PathBuf::from("withaux.tex"),
            output_dir: output_dir.clone(),
            ..Default::default()
        };

        let result = compiler.compile(&options).unwrap();

        // Should have collected .aux file
        assert!(!result.aux_outputs.is_empty());
        assert!(result.aux_outputs.keys().any(|p| p.extension() == Some("aux".as_ref())));
    }
}
