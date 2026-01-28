# FastTeX: Getting Started Guide

> Complete guide to setting up, configuring, and running FastTeX - the next-generation distributed collaborative LaTeX IDE.

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [Installation](#installation)
3. [Quick Start](#quick-start)
4. [Configuration](#configuration)
5. [Development Setup](#development-setup)
6. [Production Deployment](#production-deployment)
7. [Troubleshooting](#troubleshooting)

---

## Prerequisites

### Required Software

| Software | Minimum Version | Purpose |
|----------|-----------------|---------|
| Rust | 1.75+ | Backend services (orchestrator, gateway, worker) |
| Node.js | 20.0+ | Frontend development |
| npm/yarn | 10.0+ / 4.0+ | Package management |
| Firecracker | 1.5+ | Worker VM isolation (production) |
| KVM | Linux 5.0+ | Hardware virtualization |
| Redis | 7.0+ | Caching and session storage |
| PostgreSQL | 15+ | Database |
| Docker | 24.0+ | Development environment (optional) |

### System Requirements

**Development:**
- CPU: 4+ cores
- RAM: 16GB minimum
- Disk: 50GB SSD
- OS: Linux (Ubuntu 22.04+, Debian 12+, Fedora 38+)

**Production (per node):**
- CPU: 16+ cores (AMD EPYC or Intel Xeon recommended)
- RAM: 64GB+
- Disk: 500GB NVMe SSD
- Network: 10Gbps
- OS: Linux with KVM support

### Check Prerequisites

Run the following to verify your system:

```bash
# Check Rust
rustc --version  # Should be >= 1.75.0

# Check Node.js
node --version  # Should be >= 20.0.0

# Check KVM (Linux only)
ls /dev/kvm  # Should exist
lsmod | grep kvm  # Should show kvm modules

# Check Docker (optional)
docker --version
```

---

## Installation

### Option 1: From Source (Recommended for Development)

```bash
# Clone the repository
git clone https://github.com/fasttex/fasttex.git
cd fasttex

# Install Rust dependencies and build
cd fasttex
cargo build --release

# Install frontend dependencies
cd frontend
npm install

# Return to root
cd ../..
```

### Option 2: Using Docker (Quick Start)

```bash
# Clone the repository
git clone https://github.com/fasttex/fasttex.git
cd fasttex

# Start all services
docker-compose up -d

# View logs
docker-compose logs -f
```

### Option 3: Pre-built Binaries

```bash
# Download the latest release
curl -LO https://github.com/fasttex/fasttex/releases/latest/download/fasttex-linux-amd64.tar.gz

# Extract
tar -xzf fasttex-linux-amd64.tar.gz

# Move to PATH
sudo mv fasttex-* /usr/local/bin/
```

---

## Quick Start

### Step 1: Start Required Services

```bash
# Start Redis
redis-server --daemonize yes

# Start PostgreSQL (if not running)
sudo systemctl start postgresql

# Create database
createdb fasttex
```

### Step 2: Configure Environment

```bash
# Copy example configuration
cp .env.example .env

# Edit configuration
nano .env
```

Minimum required settings:
```env
# Database
DATABASE_URL=postgres://localhost/fasttex

# Redis
REDIS_URL=redis://localhost:6379

# Server
API_PORT=8080
WS_PORT=8081

# Storage (local for development)
STORAGE_TYPE=local
STORAGE_PATH=/tmp/fasttex-artifacts

# JWT Secret (generate a random string)
JWT_SECRET=your-secret-key-change-in-production
```

### Step 3: Run Migrations

```bash
cd fasttex
cargo run --bin migrate
```

### Step 4: Start the Services

**Terminal 1 - Gateway:**
```bash
cd fasttex
cargo run --release --bin gateway
```

**Terminal 2 - Orchestrator:**
```bash
cd fasttex
cargo run --release --bin orchestrator
```

**Terminal 3 - Frontend:**
```bash
cd fasttex/frontend
npm run dev
```

### Step 5: Access FastTeX

Open your browser to: http://localhost:3000

---

## Configuration

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `DATABASE_URL` | PostgreSQL connection string | Required |
| `REDIS_URL` | Redis connection string | `redis://localhost:6379` |
| `API_PORT` | REST API port | `8080` |
| `WS_PORT` | WebSocket port | `8081` |
| `WORKER_POOL_SIZE` | Number of worker VMs | `4` |
| `MAX_COMPILE_TIME` | Max compile duration (seconds) | `300` |
| `FMT_CACHE_TTL` | Format file cache duration | `7d` |
| `JWT_SECRET` | JWT signing key | Required |
| `STORAGE_TYPE` | `local` or `s3` | `local` |
| `S3_BUCKET` | S3 bucket name (if S3) | - |
| `S3_REGION` | S3 region | - |

### Configuration File (config.toml)

```toml
[server]
api_port = 8080
ws_port = 8081
host = "0.0.0.0"

[database]
url = "postgres://localhost/fasttex"
max_connections = 20
min_connections = 5

[redis]
url = "redis://localhost:6379"
pool_size = 10

[orchestrator]
max_concurrent_jobs = 16
job_timeout_seconds = 300
max_convergence_iterations = 5

[worker]
pool_size = 4
memory_mb = 512
cpu_count = 1
timeout_seconds = 120

[cache]
fmt_ttl_days = 7
aux_ttl_hours = 24
pdf_fragment_ttl_hours = 1
max_size_gb = 10

[storage]
type = "local"  # or "s3"
path = "/var/lib/fasttex/artifacts"

[auth]
jwt_secret = "your-secret-key"
token_expiry_hours = 24
```

---

## Development Setup

### IDE Setup

**VS Code:**
1. Install extensions:
   - `rust-analyzer`
   - `CodeLLDB`
   - `ESLint`
   - `Prettier`

2. Configure settings:
```json
{
  "rust-analyzer.checkOnSave.command": "clippy",
  "editor.formatOnSave": true
}
```

**IntelliJ IDEA:**
1. Install Rust plugin
2. Enable Cargo check on save

### Running Tests

```bash
# Run all Rust tests
cd fasttex
cargo test

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test dependency_graph::tests::test_parallel_groups

# Run frontend tests
cd frontend
npm test

# Run integration tests
cargo test --test integration
```

### Debugging

**Backend:**
```bash
# Enable debug logging
RUST_LOG=debug cargo run --bin orchestrator

# Use LLDB (with VS Code)
# Add breakpoints and press F5
```

**Frontend:**
```bash
# Start with React DevTools
npm run dev

# Access at http://localhost:3000
# Open browser DevTools → Components tab
```

### Code Style

**Rust:**
```bash
# Format code
cargo fmt

# Run linter
cargo clippy -- -D warnings

# Check for security issues
cargo audit
```

**TypeScript/JavaScript:**
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

## Production Deployment

### Prerequisites for Production

1. **Firecracker Setup:**
```bash
# Install Firecracker
curl -Lo firecracker https://github.com/firecracker-microvm/firecracker/releases/download/v1.5.0/firecracker-v1.5.0-x86_64
chmod +x firecracker
sudo mv firecracker /usr/local/bin/

# Build worker rootfs
cd fasttex/infra
./scripts/build-rootfs.sh

# Download Linux kernel
curl -Lo vmlinux https://s3.amazonaws.com/spec.ccfc.min/img/quickstart_guide/x86_64/kernels/vmlinux.bin
chmod 644 vmlinux
```

2. **System Configuration:**
```bash
# Enable KVM for non-root user
sudo usermod -a -G kvm $USER

# Configure huge pages (optional, for performance)
echo "vm.nr_hugepages = 1024" | sudo tee -a /etc/sysctl.conf
sudo sysctl -p
```

3. **Security Hardening:**
```bash
# Create dedicated user
sudo useradd -r -s /bin/false fasttex

# Set permissions
sudo chown -R fasttex:fasttex /var/lib/fasttex
sudo chmod 700 /var/lib/fasttex
```

### Systemd Services

**Gateway Service** (`/etc/systemd/system/fasttex-gateway.service`):
```ini
[Unit]
Description=FastTeX Gateway
After=network.target redis.service postgresql.service

[Service]
Type=simple
User=fasttex
Group=fasttex
WorkingDirectory=/opt/fasttex
ExecStart=/opt/fasttex/bin/gateway
Restart=always
RestartSec=5
Environment=RUST_LOG=info
EnvironmentFile=/etc/fasttex/environment

[Install]
WantedBy=multi-user.target
```

**Orchestrator Service** (`/etc/systemd/system/fasttex-orchestrator.service`):
```ini
[Unit]
Description=FastTeX Orchestrator
After=network.target redis.service postgresql.service

[Service]
Type=simple
User=fasttex
Group=fasttex
WorkingDirectory=/opt/fasttex
ExecStart=/opt/fasttex/bin/orchestrator
Restart=always
RestartSec=5
Environment=RUST_LOG=info
EnvironmentFile=/etc/fasttex/environment

[Install]
WantedBy=multi-user.target
```

### Enable and Start Services

```bash
sudo systemctl daemon-reload
sudo systemctl enable fasttex-gateway fasttex-orchestrator
sudo systemctl start fasttex-gateway fasttex-orchestrator
```

### Nginx Configuration

```nginx
upstream fasttex_api {
    server 127.0.0.1:8080;
}

upstream fasttex_ws {
    server 127.0.0.1:8081;
}

server {
    listen 443 ssl http2;
    server_name fasttex.example.com;

    ssl_certificate /etc/letsencrypt/live/fasttex.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/fasttex.example.com/privkey.pem;

    # Frontend
    location / {
        root /var/www/fasttex;
        try_files $uri $uri/ /index.html;
    }

    # API
    location /api/ {
        proxy_pass http://fasttex_api;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
    }

    # WebSocket
    location /ws/ {
        proxy_pass http://fasttex_ws;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_read_timeout 86400;
    }
}
```

---

## Troubleshooting

### Common Issues

#### 1. "KVM not available"
```bash
# Check if KVM is enabled
ls -la /dev/kvm

# If missing, check BIOS settings for virtualization
# For AMD: Enable AMD-V
# For Intel: Enable VT-x

# Load KVM module
sudo modprobe kvm
sudo modprobe kvm_intel  # or kvm_amd
```

#### 2. "Permission denied on /dev/kvm"
```bash
# Add user to kvm group
sudo usermod -a -G kvm $USER

# Re-login or run
newgrp kvm
```

#### 3. "Redis connection refused"
```bash
# Check Redis is running
systemctl status redis

# Start if not running
sudo systemctl start redis

# Check Redis is listening
redis-cli ping  # Should return PONG
```

#### 4. "Database connection failed"
```bash
# Check PostgreSQL is running
systemctl status postgresql

# Verify database exists
psql -l | grep fasttex

# Create if missing
createdb fasttex

# Check connection
psql fasttex -c "SELECT 1"
```

#### 5. "Compilation timeout"
- Increase `MAX_COMPILE_TIME` in configuration
- Check if document has infinite loops
- Verify worker VMs are starting correctly

#### 6. "Out of memory during compilation"
```bash
# Increase worker memory limit
# In config.toml:
[worker]
memory_mb = 1024  # Increase from default 512
```

### Logs

```bash
# View gateway logs
journalctl -u fasttex-gateway -f

# View orchestrator logs
journalctl -u fasttex-orchestrator -f

# View all FastTeX logs
journalctl -u 'fasttex-*' -f

# Debug level logging
RUST_LOG=debug ./fasttex-gateway
```

### Getting Help

- **Documentation**: https://docs.fasttex.io
- **GitHub Issues**: https://github.com/fasttex/fasttex/issues
- **Discord**: https://discord.gg/fasttex
- **Email**: support@fasttex.io

---

## Next Steps

After getting FastTeX running:

1. **Read the [Architecture Guide](./ARCHITECTURE.md)** - Understand how FastTeX works
2. **Read the [API Reference](./API.md)** - Integrate with FastTeX
3. **Read the [Contributing Guide](./CONTRIBUTING.md)** - Help improve FastTeX
4. **Read the [Security Guide](./SECURITY.md)** - Secure your deployment
