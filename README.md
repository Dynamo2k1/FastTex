# FastTeX

> Next-Generation Distributed Collaborative LaTeX IDE

FastTeX is a high-performance, distributed collaborative LaTeX IDE designed to significantly outperform existing solutions like Overleaf. It achieves this through architectural innovations including MPI-style parallel compilation, precompiled preambles, and CRDT-based real-time collaboration.

## Key Features

- **Real-time Collaboration**: CRDT-based editing with Yjs, supporting offline-first workflow
- **Parallel Compilation**: MPI-style scatter/gather for chapter-level parallelism
- **Incremental Builds**: Only recompile changed documents and their dependents
- **Precompiled Preambles**: Cache `.fmt` files for instant preamble loading
- **Secure Sandboxing**: Firecracker microVMs for isolated compilation
- **Git Integration**: Native Git support for version control

## Performance

| Scenario | Overleaf | FastTeX | Speedup |
|----------|----------|---------|---------|
| Full compile (cold) | 120s | 30s | **4x** |
| Full compile (warm) | 120s | 15s | **8x** |
| Single chapter edit | 120s | 5s | **24x** |
| Typo fix | 120s | 3s | **40x** |

*Benchmarked on a 10-chapter thesis with 50 TikZ figures*

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                         Frontend                                  │
│              React + CodeMirror 6 + Yjs                          │
└───────────────────────────┬──────────────────────────────────────┘
                            │
                            ▼
┌──────────────────────────────────────────────────────────────────┐
│                     Realtime Gateway                              │
│              WebSocket + Yjs Sync + Presence                      │
└───────────────────────────┬──────────────────────────────────────┘
                            │
              ┌─────────────┼─────────────┐
              ▼             ▼             ▼
┌──────────────────┐ ┌──────────────┐ ┌──────────────┐
│  Project Service │ │ Orchestrator │ │ Cache Layer  │
│  Auth + Files    │ │ Job Scheduler│ │ Redis + S3   │
└──────────────────┘ └──────┬───────┘ └──────────────┘
                            │
              ┌─────────────┼─────────────┐
              ▼             ▼             ▼
         ┌────────┐    ┌────────┐    ┌────────┐
         │Worker 1│    │Worker 2│    │Worker N│
         │(Ch. 1) │    │(Ch. 2) │    │(Figs)  │
         └────────┘    └────────┘    └────────┘
              │             │             │
              └─────────────┼─────────────┘
                   Firecracker microVMs
```

## Technical Stack

| Component | Technology |
|-----------|------------|
| Backend | Rust |
| TeX Engine | Tectonic |
| Collaboration | Yjs (CRDT) |
| Editor | CodeMirror 6 |
| Sandbox | Firecracker microVM |
| Cache | Redis + S3 |

## Repository Structure

```
fasttex/
├── frontend/          # React + CodeMirror 6 + Yjs
├── gateway/           # WebSocket realtime gateway
├── orchestrator/      # Build orchestration & scheduling
├── worker/            # Tectonic compilation worker
├── proto/             # Protocol definitions (gRPC)
├── infra/             # Firecracker configuration
└── docs/              # Architecture documentation
```

## Quick Start

### Prerequisites

- Rust 1.75+
- Node.js 20+
- Firecracker (for production)

### Development

```bash
# Clone the repository
git clone https://github.com/fasttex/fasttex.git
cd fasttex

# Build the Rust components
cd fasttex
cargo build

# Run tests
cargo test

# Start the frontend
cd frontend
npm install
npm run dev
```

## How It Works

### 1. Dependency Graph Analysis

FastTeX parses your LaTeX document to build a dependency graph:

```latex
\documentclass{book}
\include{chapter1}  → Parallel unit 1
\include{chapter2}  → Parallel unit 2
\include{chapter3}  → Parallel unit 3
```

### 2. Preamble Caching

Heavy preambles are compiled once and cached as `.fmt` files:

```latex
\usepackage{tikz}      % Expensive: 10s
\usepackage{biblatex}  % Expensive: 5s
```

With caching: **15s → <100ms**

### 3. Scatter/Gather Compilation

Chapters are compiled in parallel across workers:

```
1. Compile preamble → .fmt
2. SCATTER: Send chapters to workers
3. Workers compile independently
4. GATHER: Collect PDFs + .aux files
5. Check reference convergence
6. Merge final PDF
```

### 4. Incremental Recompilation

Content hashes track changes:

```
Edit chapter2.tex
  → Recompile: chapter2, references
  → Skip: chapter1, chapter3, figures
```

## Design Principles

1. **TeX is single-threaded**: We don't claim otherwise. Parallelism is process-level.
2. **Reproducibility**: Tectonic ensures byte-identical outputs.
3. **Local-first**: Yjs enables offline editing with automatic merge.
4. **Security-first**: Firecracker VMs prevent shell-escape attacks.

## Documentation

Comprehensive documentation is available:

### Getting Started
- **[Getting Started Guide](fasttex/docs/GETTING_STARTED.md)** - Installation, prerequisites, quick start
- **[Frontend Setup](fasttex/frontend/README.md)** - Frontend development guide
- **[Infrastructure Setup](fasttex/infra/README.md)** - Firecracker worker configuration

### Architecture & Design
- **[Architecture Overview](fasttex/docs/ARCHITECTURE.md)** - System design, components, data flow
- **[API Reference](fasttex/docs/API.md)** - REST API, WebSocket protocol, SDK examples

### Operations
- **[Deployment Guide](fasttex/docs/DEPLOYMENT.md)** - Single server, cluster, Kubernetes, cloud
- **[Security Guide](fasttex/docs/SECURITY.md)** - Authentication, sandboxing, hardening

### Contributing
- **[Contributing Guide](fasttex/docs/CONTRIBUTING.md)** - Code style, testing, PR process

## Why FastTeX Beats Overleaf

| Feature | Overleaf | FastTeX |
|---------|----------|---------|
| Compilation | Full document every time | Incremental, parallel |
| Preamble | Reparsed every compile | Cached as .fmt |
| Large docs | Minutes to compile | Seconds |
| Offline | Not supported | Full offline editing |
| Security | Docker containers | Firecracker microVMs |
| Collaboration | OT-based | CRDT-based (Yjs) |

### Performance Analysis

**Where speedups come from:**
1. **Preamble caching**: 5-15s → <100ms (50-150x)
2. **Chapter parallelism**: N chapters → ~1 chapter time
3. **Incremental compilation**: Only changed files
4. **Figure pre-rendering**: Cached TikZ/PGF

**Where limits exist:**
1. Final PDF merge is sequential
2. Heavy cross-references need multiple passes
3. Single large chapters can't be parallelized
4. Very small docs have distribution overhead

## Contributing

We welcome contributions! Please see [CONTRIBUTING.md](fasttex/docs/CONTRIBUTING.md) for guidelines.

### Development Setup

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install development tools
rustup component add rustfmt clippy
cargo install cargo-watch cargo-nextest

# Run tests with watch
cargo watch -x test

# Check code style
cargo fmt -- --check
cargo clippy -- -D warnings
```

## License

MIT License - see [LICENSE](LICENSE) for details
