//! LaTeX Document Parser for Dependency Analysis
//!
//! This module parses LaTeX documents to extract dependency information:
//! - \include{} and \input{} commands
//! - \includeonly{} directives
//! - tikzexternalize figures
//! - standalone document references
//! - bibliography files

use std::collections::HashSet;
use std::path::PathBuf;

use regex::Regex;
use thiserror::Error;

/// Errors that can occur during parsing
#[derive(Error, Debug)]
pub enum ParseError {
    #[error("File not found: {0}")]
    FileNotFound(PathBuf),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Invalid LaTeX syntax: {0}")]
    InvalidSyntax(String),
}

/// Represents a parsed LaTeX document structure
#[derive(Debug, Clone)]
pub struct ParsedDocument {
    /// Path to the main document
    pub main_path: PathBuf,
    /// Document class
    pub document_class: Option<String>,
    /// Included packages
    pub packages: Vec<PackageUsage>,
    /// Included chapters (\include{})
    pub includes: Vec<IncludeDirective>,
    /// Input files (\input{})
    pub inputs: Vec<PathBuf>,
    /// Bibliography files
    pub bibliographies: Vec<PathBuf>,
    /// External figures (tikzexternalize)
    pub external_figures: Vec<PathBuf>,
    /// \includeonly filter (if present)
    pub include_only: Option<Vec<String>>,
    /// Whether the document uses tikzexternalize
    pub uses_tikz_externalize: bool,
    /// Custom preamble end position (for .fmt generation)
    pub preamble_end: Option<usize>,
}

/// Package usage with options
#[derive(Debug, Clone)]
pub struct PackageUsage {
    pub name: String,
    pub options: Vec<String>,
}

/// An \include directive
#[derive(Debug, Clone)]
pub struct IncludeDirective {
    /// Path to the included file (without .tex extension)
    pub path: PathBuf,
    /// Whether this is filtered out by \includeonly
    pub is_filtered: bool,
    /// Line number in source
    pub line: usize,
}

/// Parser for LaTeX documents
pub struct LatexParser {
    // Compiled regex patterns
    include_re: Regex,
    input_re: Regex,
    include_only_re: Regex,
    document_class_re: Regex,
    use_package_re: Regex,
    bibliography_re: Regex,
    tikz_external_re: Regex,
    begin_document_re: Regex,
}

impl Default for LatexParser {
    fn default() -> Self {
        Self::new()
    }
}

impl LatexParser {
    /// Create a new LaTeX parser with compiled regex patterns
    pub fn new() -> Self {
        LatexParser {
            include_re: Regex::new(r"\\include\{([^}]+)\}").unwrap(),
            input_re: Regex::new(r"\\input\{([^}]+)\}").unwrap(),
            include_only_re: Regex::new(r"\\includeonly\{([^}]+)\}").unwrap(),
            document_class_re: Regex::new(r"\\documentclass(?:\[([^\]]*)\])?\{([^}]+)\}").unwrap(),
            use_package_re: Regex::new(r"\\usepackage(?:\[([^\]]*)\])?\{([^}]+)\}").unwrap(),
            bibliography_re: Regex::new(r"\\bibliography\{([^}]+)\}|\\addbibresource\{([^}]+)\}").unwrap(),
            tikz_external_re: Regex::new(r"\\tikzexternalize").unwrap(),
            begin_document_re: Regex::new(r"\\begin\{document\}").unwrap(),
        }
    }

    /// Parse a LaTeX document and extract dependencies
    pub fn parse(&self, content: &str, main_path: PathBuf) -> Result<ParsedDocument, ParseError> {
        let mut doc = ParsedDocument {
            main_path,
            document_class: None,
            packages: Vec::new(),
            includes: Vec::new(),
            inputs: Vec::new(),
            bibliographies: Vec::new(),
            external_figures: Vec::new(),
            include_only: None,
            uses_tikz_externalize: false,
            preamble_end: None,
        };

        // Remove comments for parsing
        let content_no_comments = self.strip_comments(content);

        // Parse document class
        if let Some(caps) = self.document_class_re.captures(&content_no_comments) {
            doc.document_class = Some(caps.get(2).map(|m| m.as_str().to_string()).unwrap_or_default());
        }

        // Parse packages
        for caps in self.use_package_re.captures_iter(&content_no_comments) {
            let options: Vec<String> = caps.get(1)
                .map(|m| m.as_str().split(',').map(|s| s.trim().to_string()).collect())
                .unwrap_or_default();
            
            // Package names can be comma-separated
            let package_names = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            for name in package_names.split(',') {
                doc.packages.push(PackageUsage {
                    name: name.trim().to_string(),
                    options: options.clone(),
                });
            }
        }

        // Parse \includeonly
        if let Some(caps) = self.include_only_re.captures(&content_no_comments) {
            let files: Vec<String> = caps.get(1)
                .map(|m| m.as_str().split(',').map(|s| s.trim().to_string()).collect())
                .unwrap_or_default();
            doc.include_only = Some(files);
        }

        // Parse \include directives
        for (line_num, line) in content.lines().enumerate() {
            for caps in self.include_re.captures_iter(line) {
                if let Some(path_match) = caps.get(1) {
                    let path_str = path_match.as_str().trim();
                    let is_filtered = doc.include_only.as_ref()
                        .map(|only| !only.iter().any(|f| f == path_str))
                        .unwrap_or(false);
                    
                    doc.includes.push(IncludeDirective {
                        path: PathBuf::from(path_str),
                        is_filtered,
                        line: line_num + 1,
                    });
                }
            }
        }

        // Parse \input directives
        for caps in self.input_re.captures_iter(&content_no_comments) {
            if let Some(path_match) = caps.get(1) {
                doc.inputs.push(PathBuf::from(path_match.as_str().trim()));
            }
        }

        // Parse bibliography
        for caps in self.bibliography_re.captures_iter(&content_no_comments) {
            let bib_file = caps.get(1).or_else(|| caps.get(2));
            if let Some(bib_match) = bib_file {
                for bib in bib_match.as_str().split(',') {
                    let mut path = PathBuf::from(bib.trim());
                    if path.extension().is_none() {
                        path.set_extension("bib");
                    }
                    doc.bibliographies.push(path);
                }
            }
        }

        // Check for tikzexternalize
        doc.uses_tikz_externalize = self.tikz_external_re.is_match(&content_no_comments);

        // Find preamble end
        if let Some(mat) = self.begin_document_re.find(&content_no_comments) {
            doc.preamble_end = Some(mat.start());
        }

        Ok(doc)
    }

    /// Strip LaTeX comments from content
    fn strip_comments(&self, content: &str) -> String {
        let mut result = String::with_capacity(content.len());
        
        for line in content.lines() {
            // Find % that isn't escaped
            let mut escaped = false;
            let mut comment_start = None;
            
            for (i, ch) in line.char_indices() {
                if ch == '\\' {
                    escaped = !escaped;
                } else if ch == '%' && !escaped {
                    comment_start = Some(i);
                    break;
                } else {
                    escaped = false;
                }
            }
            
            match comment_start {
                Some(pos) => {
                    result.push_str(&line[..pos]);
                    result.push('\n');
                }
                None => {
                    result.push_str(line);
                    result.push('\n');
                }
            }
        }
        
        result
    }

    /// Extract external TikZ figure references
    pub fn find_external_figures(&self, content: &str, externalize_prefix: Option<&str>) -> Vec<PathBuf> {
        let mut figures = Vec::new();
        
        // Look for tikzpicture environments that would be externalized
        let tikz_re = Regex::new(r"\\begin\{tikzpicture\}").unwrap();
        let tikz_name_re = Regex::new(r"\\tikzsetnextfilename\{([^}]+)\}").unwrap();
        
        let prefix = externalize_prefix.unwrap_or("figure");
        let mut figure_count = 0;
        
        for caps in tikz_name_re.captures_iter(content) {
            if let Some(name) = caps.get(1) {
                figures.push(PathBuf::from(format!("{}.pdf", name.as_str())));
            }
        }
        
        // Count unnamed figures
        for _ in tikz_re.find_iter(content) {
            figure_count += 1;
        }
        
        // Add generic names for figures without explicit names
        let named_count = figures.len();
        for i in named_count..figure_count {
            figures.push(PathBuf::from(format!("{}{}.pdf", prefix, i)));
        }
        
        figures
    }

    /// Get all file dependencies for a document
    pub fn get_all_dependencies(&self, doc: &ParsedDocument) -> HashSet<PathBuf> {
        let mut deps = HashSet::new();
        
        for inc in &doc.includes {
            if !inc.is_filtered {
                let mut path = inc.path.clone();
                if path.extension().is_none() {
                    path.set_extension("tex");
                }
                deps.insert(path);
            }
        }
        
        for input in &doc.inputs {
            let mut path = input.clone();
            if path.extension().is_none() {
                path.set_extension("tex");
            }
            deps.insert(path);
        }
        
        for bib in &doc.bibliographies {
            deps.insert(bib.clone());
        }
        
        deps
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_document_class() {
        let parser = LatexParser::new();
        let content = r"\documentclass[12pt,a4paper]{article}";
        let doc = parser.parse(content, PathBuf::from("main.tex")).unwrap();
        
        assert_eq!(doc.document_class, Some("article".to_string()));
    }

    #[test]
    fn test_parse_packages() {
        let parser = LatexParser::new();
        let content = r"
\usepackage{amsmath}
\usepackage[utf8]{inputenc}
\usepackage{tikz,pgfplots}
";
        let doc = parser.parse(content, PathBuf::from("main.tex")).unwrap();
        
        assert_eq!(doc.packages.len(), 4);
        assert_eq!(doc.packages[0].name, "amsmath");
        assert_eq!(doc.packages[1].name, "inputenc");
        assert_eq!(doc.packages[1].options, vec!["utf8"]);
    }

    #[test]
    fn test_parse_includes() {
        let parser = LatexParser::new();
        let content = r"
\include{chapter1}
\include{chapter2}
\include{chapter3}
";
        let doc = parser.parse(content, PathBuf::from("main.tex")).unwrap();
        
        assert_eq!(doc.includes.len(), 3);
        assert_eq!(doc.includes[0].path, PathBuf::from("chapter1"));
        assert!(!doc.includes[0].is_filtered);
    }

    #[test]
    fn test_parse_include_only() {
        let parser = LatexParser::new();
        let content = r"
\includeonly{chapter1,chapter3}
\include{chapter1}
\include{chapter2}
\include{chapter3}
";
        let doc = parser.parse(content, PathBuf::from("main.tex")).unwrap();
        
        assert_eq!(doc.include_only, Some(vec!["chapter1".to_string(), "chapter3".to_string()]));
        assert!(!doc.includes[0].is_filtered); // chapter1
        assert!(doc.includes[1].is_filtered);  // chapter2
        assert!(!doc.includes[2].is_filtered); // chapter3
    }

    #[test]
    fn test_parse_bibliography() {
        let parser = LatexParser::new();
        let content = r"
\bibliography{refs,extra}
\addbibresource{more.bib}
";
        let doc = parser.parse(content, PathBuf::from("main.tex")).unwrap();
        
        assert_eq!(doc.bibliographies.len(), 3);
        assert_eq!(doc.bibliographies[0], PathBuf::from("refs.bib"));
        assert_eq!(doc.bibliographies[1], PathBuf::from("extra.bib"));
        assert_eq!(doc.bibliographies[2], PathBuf::from("more.bib"));
    }

    #[test]
    fn test_parse_tikz_externalize() {
        let parser = LatexParser::new();
        let content = r"
\usepackage{tikz}
\usetikzlibrary{external}
\tikzexternalize
";
        let doc = parser.parse(content, PathBuf::from("main.tex")).unwrap();
        
        assert!(doc.uses_tikz_externalize);
    }

    #[test]
    fn test_strip_comments() {
        let parser = LatexParser::new();
        let content = r"
\documentclass{article} % This is a comment
\usepackage{amsmath}
% \usepackage{unused}
\begin{document}
Hello \% world % Another comment
\end{document}
";
        let stripped = parser.strip_comments(content);
        
        assert!(stripped.contains(r"\documentclass{article}"));
        assert!(!stripped.contains("This is a comment"));
        assert!(!stripped.contains("unused"));
        assert!(stripped.contains(r"\%")); // Escaped percent should remain
    }

    #[test]
    fn test_preamble_end() {
        let parser = LatexParser::new();
        let content = r"
\documentclass{article}
\usepackage{amsmath}
\begin{document}
Hello
\end{document}
";
        let doc = parser.parse(content, PathBuf::from("main.tex")).unwrap();
        
        assert!(doc.preamble_end.is_some());
        let preamble = &content[..doc.preamble_end.unwrap()];
        assert!(preamble.contains(r"\usepackage{amsmath}"));
        assert!(!preamble.contains("Hello"));
    }

    #[test]
    fn test_get_all_dependencies() {
        let parser = LatexParser::new();
        let content = r"
\include{chapter1}
\input{macros}
\bibliography{refs}
";
        let doc = parser.parse(content, PathBuf::from("main.tex")).unwrap();
        let deps = parser.get_all_dependencies(&doc);
        
        assert!(deps.contains(&PathBuf::from("chapter1.tex")));
        assert!(deps.contains(&PathBuf::from("macros.tex")));
        assert!(deps.contains(&PathBuf::from("refs.bib")));
    }
}
