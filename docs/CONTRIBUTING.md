# Contributing to FastTeX

Thank you for your interest in contributing to FastTeX! This document provides guidelines for contributing to the project.

## Table of Contents

1. [Code of Conduct](#code-of-conduct)
2. [Getting Started](#getting-started)
3. [Development Workflow](#development-workflow)
4. [Code Style](#code-style)
5. [Testing](#testing)
6. [Documentation](#documentation)
7. [Pull Request Process](#pull-request-process)
8. [Issue Guidelines](#issue-guidelines)

---

## Code of Conduct

### Our Pledge

We are committed to providing a welcoming and inclusive environment for everyone. We pledge to:

- Be welcoming and friendly
- Be respectful of differing viewpoints
- Accept constructive criticism gracefully
- Focus on what is best for the community
- Show empathy towards other community members

### Unacceptable Behavior

- Harassment, trolling, or personal attacks
- Discriminatory jokes or language
- Publishing others' private information
- Other conduct which could be considered inappropriate

### Enforcement

Violations should be reported to security@fasttex.io. All reports will be reviewed and investigated.

---

## Getting Started

### Prerequisites

1. **Rust 1.75+**
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   rustup update
   ```

2. **Node.js 20+**
   ```bash
   # Using nvm
   nvm install 20
   nvm use 20
   ```

3. **Additional Tools**
   ```bash
   # Rust tooling
   rustup component add rustfmt clippy
   cargo install cargo-watch cargo-audit
   
   # For testing
   cargo install cargo-nextest
   ```

### Fork and Clone

1. Fork the repository on GitHub
2. Clone your fork:
   ```bash
   git clone https://github.com/YOUR_USERNAME/fasttex.git
   cd fasttex
   ```

3. Add upstream remote:
   ```bash
   git remote add upstream https://github.com/fasttex/fasttex.git
   ```

### Build the Project

```bash
cd fasttex

# Build all components
cargo build

# Run tests
cargo test

# Build frontend
cd frontend
npm install
npm run build
```

---

## Development Workflow

### Branch Naming

| Type | Pattern | Example |
|------|---------|---------|
| Feature | `feature/description` | `feature/add-synctex-support` |
| Bug Fix | `fix/description` | `fix/compile-timeout-handling` |
| Documentation | `docs/description` | `docs/api-reference-update` |
| Refactor | `refactor/description` | `refactor/scheduler-module` |
| Performance | `perf/description` | `perf/cache-optimization` |

### Development Flow

1. **Sync with upstream:**
   ```bash
   git fetch upstream
   git checkout main
   git merge upstream/main
   ```

2. **Create feature branch:**
   ```bash
   git checkout -b feature/my-feature
   ```

3. **Make changes and commit:**
   ```bash
   git add .
   git commit -m "feat: add synctex support for bidirectional navigation"
   ```

4. **Push and create PR:**
   ```bash
   git push origin feature/my-feature
   # Create PR on GitHub
   ```

### Commit Messages

We follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

**Types:**
| Type | Description |
|------|-------------|
| `feat` | New feature |
| `fix` | Bug fix |
| `docs` | Documentation only |
| `style` | Code style (formatting, etc.) |
| `refactor` | Code refactoring |
| `perf` | Performance improvement |
| `test` | Adding/updating tests |
| `chore` | Build process, tools |
| `ci` | CI configuration |

**Examples:**
```
feat(orchestrator): add parallel chapter compilation

Implement MPI-style scatter/gather for chapter-level parallelism.
Uses dependency graph to determine safe parallelization.

Closes #123
```

```
fix(worker): handle compilation timeout correctly

Previously, timeout would leave zombie processes. Now properly
terminates the Tectonic process and cleans up temp files.

Fixes #456
```

---

## Code Style

### Rust

We follow the Rust style guide with some additions:

```rust
// Use descriptive variable names
let compilation_result = compiler.compile(&options)?;  // Good
let res = c.comp(&o)?;  // Bad

// Document public APIs
/// Compiles a LaTeX document using Tectonic.
///
/// # Arguments
///
/// * `options` - Compilation options including input path and timeout
///
/// # Returns
///
/// Returns `CompileResult` on success, or `CompileError` on failure.
///
/// # Examples
///
/// ```
/// let result = compiler.compile(&CompileOptions::default())?;
/// println!("PDF at: {:?}", result.pdf_path);
/// ```
pub fn compile(&self, options: &CompileOptions) -> Result<CompileResult, CompileError> {
    // ...
}

// Use meaningful error messages
return Err(CompileError::Timeout {
    path: options.input_path.clone(),
    elapsed: start.elapsed(),
    limit: options.timeout,
});  // Good

return Err(CompileError::Timeout);  // Less informative

// Group imports
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::cache::ArtifactCache;
use crate::scheduler::JobScheduler;
```

**Formatting:**
```bash
# Format code
cargo fmt

# Check formatting
cargo fmt -- --check
```

**Linting:**
```bash
# Run clippy
cargo clippy -- -D warnings

# Fix auto-fixable issues
cargo clippy --fix
```

### TypeScript/React

```typescript
// Use TypeScript strict mode
// tsconfig.json: "strict": true

// Define interfaces for props
interface EditorProps {
  projectId: string;
  initialContent: string;
  onSave: (content: string) => Promise<void>;
}

// Use functional components with hooks
export const Editor: React.FC<EditorProps> = ({
  projectId,
  initialContent,
  onSave,
}) => {
  const [content, setContent] = useState(initialContent);
  const editorRef = useRef<EditorView | null>(null);

  // Use callbacks for event handlers
  const handleSave = useCallback(async () => {
    await onSave(content);
  }, [content, onSave]);

  return (
    <div className="editor-container">
      {/* ... */}
    </div>
  );
};

// Use custom hooks for reusable logic
export function useYjsDocument(projectId: string) {
  const [doc, setDoc] = useState<Y.Doc | null>(null);
  // ...
  return { doc, connected, error };
}
```

**Formatting:**
```bash
cd frontend

# Format
npm run format

# Lint
npm run lint

# Type check
npm run typecheck
```

---

## Testing

### Unit Tests

**Rust:**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_hash_consistency() {
        let content = b"Hello, World!";
        let hash1 = ContentHash::from_content(content);
        let hash2 = ContentHash::from_content(content);
        
        assert_eq!(hash1, hash2);
    }

    #[tokio::test]
    async fn test_scheduler_job_ordering() {
        let scheduler = JobScheduler::new(SchedulerConfig::default());
        // ...
    }

    #[test]
    fn test_dependency_graph_cycle_detection() {
        let mut graph = DependencyGraph::new(PathBuf::from("/test"));
        // Create cycle
        // ...
        assert!(matches!(
            graph.topological_order(),
            Err(GraphError::CycleDetected)
        ));
    }
}
```

**Run tests:**
```bash
# Run all tests
cargo test

# Run specific test
cargo test test_content_hash

# Run with output
cargo test -- --nocapture

# Use nextest for parallel execution
cargo nextest run
```

### Integration Tests

```rust
// tests/integration/compile_test.rs
use fasttex_orchestrator::*;
use fasttex_worker::*;

#[tokio::test]
async fn test_full_compilation_flow() {
    // Setup
    let project = setup_test_project().await;
    
    // Create document
    write_test_file(&project, "main.tex", r#"
        \documentclass{article}
        \begin{document}
        Hello, World!
        \end{document}
    "#).await;
    
    // Compile
    let result = compile_project(&project).await.unwrap();
    
    // Verify
    assert!(result.success);
    assert!(result.pdf_path.is_some());
    
    // Cleanup
    cleanup_test_project(project).await;
}
```

### Frontend Tests

```typescript
// src/components/Editor.test.tsx
import { render, screen, fireEvent } from '@testing-library/react';
import { Editor } from './Editor';

describe('Editor', () => {
  it('renders with initial content', () => {
    render(
      <Editor
        projectId="test-123"
        initialContent="\\documentclass{article}"
        onSave={jest.fn()}
      />
    );
    
    expect(screen.getByText(/documentclass/)).toBeInTheDocument();
  });

  it('calls onSave when save shortcut is pressed', async () => {
    const onSave = jest.fn();
    render(
      <Editor
        projectId="test-123"
        initialContent=""
        onSave={onSave}
      />
    );
    
    fireEvent.keyDown(document, { key: 's', ctrlKey: true });
    
    expect(onSave).toHaveBeenCalled();
  });
});
```

**Run frontend tests:**
```bash
cd frontend
npm test
npm test -- --coverage
```

### Test Coverage

We aim for:
- **Unit tests**: 80%+ coverage
- **Integration tests**: All critical paths
- **End-to-end tests**: Main user flows

```bash
# Generate coverage report
cargo tarpaulin --out Html

# View report
open tarpaulin-report.html
```

---

## Documentation

### Code Documentation

All public APIs must be documented:

```rust
/// A node in the compilation dependency graph.
///
/// Each node represents a compilable unit (chapter, figure, etc.)
/// and tracks its dependencies and compilation state.
///
/// # Examples
///
/// ```
/// use fasttex_orchestrator::CompileNode;
///
/// let node = CompileNode::new(
///     NodeType::Chapter,
///     PathBuf::from("chapter1.tex"),
///     b"Chapter content",
/// );
/// assert!(node.needs_recompile());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileNode {
    /// Unique identifier for this node
    pub id: NodeId,
    
    /// Type of compilation unit
    pub node_type: NodeType,
    
    /// Path to source file relative to project root
    pub source_path: PathBuf,
    
    // ...
}
```

### Documentation Updates

When making changes:
1. Update relevant doc comments
2. Update README if API changes
3. Update architecture docs for significant changes
4. Add examples for new features

### Building Documentation

```bash
# Generate Rust docs
cargo doc --no-deps --open

# Build documentation site (if using mdBook)
mdbook build docs/
```

---

## Pull Request Process

### Before Submitting

1. **Ensure tests pass:**
   ```bash
   cargo test
   cargo clippy -- -D warnings
   cargo fmt -- --check
   ```

2. **Update documentation** if needed

3. **Write meaningful commit messages**

4. **Keep PRs focused** - one feature/fix per PR

### PR Template

```markdown
## Description

Brief description of changes.

## Type of Change

- [ ] Bug fix
- [ ] New feature
- [ ] Breaking change
- [ ] Documentation update

## How Has This Been Tested?

Describe the tests you ran.

## Checklist

- [ ] Code follows style guidelines
- [ ] Self-reviewed code
- [ ] Comments added for complex code
- [ ] Documentation updated
- [ ] Tests added/updated
- [ ] All tests pass
```

### Review Process

1. **Automated checks** must pass
2. **At least one approval** required
3. **Address feedback** promptly
4. **Squash and merge** preferred

### After Merge

- Delete your feature branch
- Update local main:
  ```bash
  git checkout main
  git pull upstream main
  ```

---

## Issue Guidelines

### Bug Reports

Use the bug report template:

```markdown
**Describe the bug**
A clear description of the bug.

**To Reproduce**
Steps to reproduce:
1. Go to '...'
2. Click on '...'
3. See error

**Expected behavior**
What you expected to happen.

**Screenshots**
If applicable.

**Environment:**
- OS: [e.g., Ubuntu 22.04]
- Rust version: [e.g., 1.75.0]
- FastTeX version: [e.g., 0.1.0]

**Additional context**
Any other relevant information.
```

### Feature Requests

```markdown
**Is your feature request related to a problem?**
A clear description of the problem.

**Describe the solution you'd like**
Clear description of desired behavior.

**Describe alternatives you've considered**
Other solutions you've thought about.

**Additional context**
Any other context or screenshots.
```

### Labels

| Label | Description |
|-------|-------------|
| `bug` | Something isn't working |
| `enhancement` | New feature request |
| `documentation` | Documentation improvements |
| `good first issue` | Good for newcomers |
| `help wanted` | Extra attention needed |
| `priority: high` | High priority |
| `priority: low` | Low priority |

---

## Getting Help

- **Discord**: [Join our community](https://discord.gg/fasttex)
- **Discussions**: [GitHub Discussions](https://github.com/fasttex/fasttex/discussions)
- **Email**: dev@fasttex.io

## Recognition

Contributors are recognized in:
- CONTRIBUTORS.md file
- Release notes
- Annual contributor appreciation

Thank you for contributing to FastTeX! 🎉
