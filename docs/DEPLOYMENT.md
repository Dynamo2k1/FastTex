# FastTeX Deployment Guide

> Production deployment guide for FastTeX - from single server to distributed cluster.

## Table of Contents

1. [Deployment Options](#deployment-options)
2. [Single Server Deployment](#single-server-deployment)
3. [Distributed Cluster](#distributed-cluster)
4. [Kubernetes Deployment](#kubernetes-deployment)
5. [Cloud Providers](#cloud-providers)
6. [Monitoring & Observability](#monitoring--observability)
7. [Backup & Recovery](#backup--recovery)
8. [Scaling Guidelines](#scaling-guidelines)

---

## Deployment Options

| Option | Users | Complexity | Best For |
|--------|-------|------------|----------|
| Single Server | 1-50 | Low | Small teams, testing |
| Multi-Server | 50-500 | Medium | Medium organizations |
| Kubernetes | 500+ | High | Large scale, auto-scaling |
| Managed Cloud | Any | Low | Teams without DevOps |

---

## Single Server Deployment

### Requirements

- **CPU**: 8+ cores
- **RAM**: 32GB+
- **Disk**: 200GB SSD
- **OS**: Ubuntu 22.04 LTS or Debian 12

### Step-by-Step Installation

#### 1. System Preparation

```bash
# Update system
sudo apt update && sudo apt upgrade -y

# Install dependencies
sudo apt install -y \
  build-essential \
  pkg-config \
  libssl-dev \
  curl \
  git \
  nginx \
  certbot \
  python3-certbot-nginx

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Install Node.js 20
curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash -
sudo apt install -y nodejs
```

#### 2. Database Setup

```bash
# Install PostgreSQL
sudo apt install -y postgresql postgresql-contrib

# Create database and user
sudo -u postgres psql << EOF
CREATE USER fasttex WITH PASSWORD 'secure-password-here';
CREATE DATABASE fasttex OWNER fasttex;
GRANT ALL PRIVILEGES ON DATABASE fasttex TO fasttex;
EOF
```

#### 3. Redis Setup

```bash
# Install Redis
sudo apt install -y redis-server

# Configure Redis
sudo tee /etc/redis/redis.conf.d/fasttex.conf << EOF
maxmemory 2gb
maxmemory-policy allkeys-lru
EOF

# Restart Redis
sudo systemctl restart redis
```

#### 4. Firecracker Setup

```bash
# Install Firecracker
FIRECRACKER_VERSION="1.5.0"
curl -Lo /tmp/firecracker.tgz \
  https://github.com/firecracker-microvm/firecracker/releases/download/v${FIRECRACKER_VERSION}/firecracker-v${FIRECRACKER_VERSION}-x86_64.tgz

tar -xzf /tmp/firecracker.tgz -C /tmp
sudo mv /tmp/release-v${FIRECRACKER_VERSION}-x86_64/firecracker-v${FIRECRACKER_VERSION}-x86_64 /usr/local/bin/firecracker
sudo chmod +x /usr/local/bin/firecracker

# Verify KVM is available
ls -la /dev/kvm

# Add user to kvm group
sudo usermod -a -G kvm $(whoami)
```

#### 5. Build FastTeX

```bash
# Clone repository
git clone https://github.com/fasttex/fasttex.git /opt/fasttex-src
cd /opt/fasttex-src/fasttex

# Build release binaries
cargo build --release

# Create installation directory
sudo mkdir -p /opt/fasttex/bin
sudo cp target/release/{gateway,orchestrator,worker} /opt/fasttex/bin/

# Build frontend
cd frontend
npm install
npm run build
sudo mkdir -p /var/www/fasttex
sudo cp -r dist/* /var/www/fasttex/
```

#### 6. Configuration

```bash
# Create configuration directory
sudo mkdir -p /etc/fasttex

# Create configuration file
sudo tee /etc/fasttex/config.toml << EOF
[server]
api_port = 8080
ws_port = 8081
host = "127.0.0.1"

[database]
url = "postgres://fasttex:secure-password-here@localhost/fasttex"
max_connections = 20

[redis]
url = "redis://localhost:6379"

[orchestrator]
max_concurrent_jobs = 8
job_timeout_seconds = 300

[worker]
pool_size = 4
memory_mb = 512
cpu_count = 1

[storage]
type = "local"
path = "/var/lib/fasttex/artifacts"

[auth]
jwt_secret = "$(openssl rand -hex 32)"
EOF

sudo chmod 600 /etc/fasttex/config.toml
```

#### 7. Systemd Services

```bash
# Create fasttex user
sudo useradd -r -s /bin/false fasttex
sudo mkdir -p /var/lib/fasttex/{artifacts,logs}
sudo chown -R fasttex:fasttex /var/lib/fasttex

# Gateway service
sudo tee /etc/systemd/system/fasttex-gateway.service << EOF
[Unit]
Description=FastTeX Gateway
After=network.target postgresql.service redis.service

[Service]
Type=simple
User=fasttex
Group=fasttex
ExecStart=/opt/fasttex/bin/gateway --config /etc/fasttex/config.toml
Restart=always
RestartSec=5
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
EOF

# Orchestrator service
sudo tee /etc/systemd/system/fasttex-orchestrator.service << EOF
[Unit]
Description=FastTeX Orchestrator
After=network.target postgresql.service redis.service

[Service]
Type=simple
User=fasttex
Group=fasttex
ExecStart=/opt/fasttex/bin/orchestrator --config /etc/fasttex/config.toml
Restart=always
RestartSec=5
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
EOF

# Enable and start services
sudo systemctl daemon-reload
sudo systemctl enable fasttex-gateway fasttex-orchestrator
sudo systemctl start fasttex-gateway fasttex-orchestrator
```

#### 8. Nginx Configuration

```bash
# Configure Nginx
sudo tee /etc/nginx/sites-available/fasttex << EOF
upstream fasttex_api {
    server 127.0.0.1:8080;
    keepalive 32;
}

upstream fasttex_ws {
    server 127.0.0.1:8081;
    keepalive 32;
}

server {
    listen 80;
    server_name fasttex.yourdomain.com;
    return 301 https://\$server_name\$request_uri;
}

server {
    listen 443 ssl http2;
    server_name fasttex.yourdomain.com;

    ssl_certificate /etc/letsencrypt/live/fasttex.yourdomain.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/fasttex.yourdomain.com/privkey.pem;
    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_ciphers ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256;
    ssl_prefer_server_ciphers off;

    # Security headers
    add_header X-Frame-Options "SAMEORIGIN" always;
    add_header X-Content-Type-Options "nosniff" always;
    add_header X-XSS-Protection "1; mode=block" always;

    # Frontend
    root /var/www/fasttex;
    index index.html;

    location / {
        try_files \$uri \$uri/ /index.html;
    }

    # API
    location /api/ {
        proxy_pass http://fasttex_api;
        proxy_http_version 1.1;
        proxy_set_header Host \$host;
        proxy_set_header X-Real-IP \$remote_addr;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto \$scheme;
        proxy_connect_timeout 60s;
        proxy_send_timeout 60s;
        proxy_read_timeout 60s;
    }

    # WebSocket
    location /ws/ {
        proxy_pass http://fasttex_ws;
        proxy_http_version 1.1;
        proxy_set_header Upgrade \$http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host \$host;
        proxy_set_header X-Real-IP \$remote_addr;
        proxy_read_timeout 86400;
    }

    # Static assets caching
    location ~* \.(js|css|png|jpg|jpeg|gif|ico|svg|woff|woff2)$ {
        expires 1y;
        add_header Cache-Control "public, immutable";
    }
}
EOF

# Enable site
sudo ln -s /etc/nginx/sites-available/fasttex /etc/nginx/sites-enabled/

# Get SSL certificate
sudo certbot --nginx -d fasttex.yourdomain.com

# Restart Nginx
sudo systemctl restart nginx
```

---

## Distributed Cluster

### Architecture

```
                    ┌─────────────────────┐
                    │   Load Balancer     │
                    │   (HAProxy/Nginx)   │
                    └─────────┬───────────┘
                              │
          ┌───────────────────┼───────────────────┐
          │                   │                   │
    ┌─────▼─────┐       ┌─────▼─────┐       ┌─────▼─────┐
    │  Gateway  │       │  Gateway  │       │  Gateway  │
    │  Node 1   │       │  Node 2   │       │  Node 3   │
    └─────┬─────┘       └─────┬─────┘       └─────┬─────┘
          │                   │                   │
          └───────────────────┼───────────────────┘
                              │
                    ┌─────────▼─────────┐
                    │   Message Queue   │
                    │   (Redis/NATS)    │
                    └─────────┬─────────┘
                              │
          ┌───────────────────┼───────────────────┐
          │                   │                   │
    ┌─────▼─────┐       ┌─────▼─────┐       ┌─────▼─────┐
    │Orchestrator│      │Orchestrator│      │Orchestrator│
    │  Node 1   │       │  Node 2   │       │  Node 3   │
    └─────┬─────┘       └─────┬─────┘       └─────┬─────┘
          │                   │                   │
    ┌─────▼─────┐       ┌─────▼─────┐       ┌─────▼─────┐
    │  Workers  │       │  Workers  │       │  Workers  │
    │  (VMs)    │       │  (VMs)    │       │  (VMs)    │
    └───────────┘       └───────────┘       └───────────┘
```

### Multi-Node Setup

#### Load Balancer (HAProxy)

```bash
# Install HAProxy
sudo apt install -y haproxy

# Configure HAProxy
sudo tee /etc/haproxy/haproxy.cfg << EOF
global
    maxconn 50000
    log /dev/log local0
    log /dev/log local1 notice

defaults
    mode http
    log global
    timeout connect 5s
    timeout client 50s
    timeout server 50s

frontend http_front
    bind *:80
    redirect scheme https code 301

frontend https_front
    bind *:443 ssl crt /etc/ssl/fasttex/combined.pem
    
    acl is_websocket hdr(Upgrade) -i WebSocket
    acl is_api path_beg /api/
    acl is_ws path_beg /ws/

    use_backend ws_backend if is_websocket
    use_backend ws_backend if is_ws
    use_backend api_backend if is_api
    default_backend static_backend

backend static_backend
    server static1 127.0.0.1:3000 check

backend api_backend
    balance roundrobin
    option httpchk GET /health
    server api1 gateway1:8080 check
    server api2 gateway2:8080 check
    server api3 gateway3:8080 check

backend ws_backend
    balance leastconn
    option httpchk GET /health
    server ws1 gateway1:8081 check
    server ws2 gateway2:8081 check
    server ws3 gateway3:8081 check
EOF

sudo systemctl restart haproxy
```

#### Redis Cluster

```bash
# On each Redis node, configure clustering
# redis1.conf
sudo tee /etc/redis/redis.conf << EOF
port 6379
cluster-enabled yes
cluster-config-file nodes.conf
cluster-node-timeout 5000
appendonly yes
EOF

# Create cluster (run on one node)
redis-cli --cluster create \
  redis1:6379 redis2:6379 redis3:6379 \
  redis4:6379 redis5:6379 redis6:6379 \
  --cluster-replicas 1
```

#### PostgreSQL with Replication

```bash
# Primary node configuration
sudo tee -a /etc/postgresql/15/main/postgresql.conf << EOF
listen_addresses = '*'
wal_level = replica
max_wal_senders = 10
synchronous_commit = on
EOF

# Create replication user
sudo -u postgres psql << EOF
CREATE ROLE replicator WITH REPLICATION LOGIN PASSWORD 'repl-password';
EOF

# Replica node setup
sudo -u postgres pg_basebackup -h primary-host -D /var/lib/postgresql/15/main -U replicator -P -v -R -X stream -C -S replica1
```

---

## Kubernetes Deployment

### Helm Chart

```bash
# Add FastTeX Helm repository
helm repo add fasttex https://charts.fasttex.io
helm repo update

# Install FastTeX
helm install fasttex fasttex/fasttex \
  --namespace fasttex \
  --create-namespace \
  --values values.yaml
```

### values.yaml

```yaml
global:
  domain: fasttex.yourdomain.com
  storageClass: gp3

gateway:
  replicas: 3
  resources:
    requests:
      cpu: 500m
      memory: 512Mi
    limits:
      cpu: 2000m
      memory: 2Gi
  autoscaling:
    enabled: true
    minReplicas: 3
    maxReplicas: 10
    targetCPUUtilization: 70

orchestrator:
  replicas: 3
  resources:
    requests:
      cpu: 1000m
      memory: 1Gi
    limits:
      cpu: 4000m
      memory: 4Gi

worker:
  replicas: 10
  resources:
    requests:
      cpu: 1000m
      memory: 1Gi
    limits:
      cpu: 2000m
      memory: 2Gi
  # For Firecracker workers, use privileged pods or bare metal
  privileged: true

postgresql:
  enabled: true
  primary:
    persistence:
      size: 100Gi
  readReplicas:
    replicaCount: 2
    persistence:
      size: 100Gi

redis:
  enabled: true
  architecture: replication
  sentinel:
    enabled: true
  replica:
    replicaCount: 3

ingress:
  enabled: true
  className: nginx
  annotations:
    cert-manager.io/cluster-issuer: letsencrypt-prod
  tls:
    - hosts:
        - fasttex.yourdomain.com
      secretName: fasttex-tls

storage:
  type: s3
  bucket: fasttex-artifacts
  region: us-east-1
```

### Kubernetes Manifests (Manual)

```yaml
# namespace.yaml
apiVersion: v1
kind: Namespace
metadata:
  name: fasttex
---
# configmap.yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: fasttex-config
  namespace: fasttex
data:
  config.toml: |
    [server]
    api_port = 8080
    ws_port = 8081
    host = "0.0.0.0"
    
    [orchestrator]
    max_concurrent_jobs = 16
    job_timeout_seconds = 300
---
# gateway-deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: fasttex-gateway
  namespace: fasttex
spec:
  replicas: 3
  selector:
    matchLabels:
      app: fasttex-gateway
  template:
    metadata:
      labels:
        app: fasttex-gateway
    spec:
      containers:
        - name: gateway
          image: fasttex/gateway:latest
          ports:
            - containerPort: 8080
              name: http
            - containerPort: 8081
              name: websocket
          resources:
            requests:
              cpu: 500m
              memory: 512Mi
            limits:
              cpu: 2000m
              memory: 2Gi
          env:
            - name: DATABASE_URL
              valueFrom:
                secretKeyRef:
                  name: fasttex-secrets
                  key: database-url
            - name: REDIS_URL
              value: "redis://redis-master:6379"
          volumeMounts:
            - name: config
              mountPath: /etc/fasttex
      volumes:
        - name: config
          configMap:
            name: fasttex-config
---
# gateway-service.yaml
apiVersion: v1
kind: Service
metadata:
  name: fasttex-gateway
  namespace: fasttex
spec:
  selector:
    app: fasttex-gateway
  ports:
    - name: http
      port: 8080
      targetPort: 8080
    - name: websocket
      port: 8081
      targetPort: 8081
```

---

## Cloud Providers

### AWS

#### Recommended Architecture

- **Compute**: ECS Fargate for Gateway/Orchestrator, EC2 bare metal for Workers
- **Database**: RDS PostgreSQL with Multi-AZ
- **Cache**: ElastiCache Redis Cluster
- **Storage**: S3 for artifacts
- **CDN**: CloudFront for frontend

#### Terraform Example

```hcl
# main.tf
terraform {
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
  }
}

provider "aws" {
  region = "us-east-1"
}

# VPC
module "vpc" {
  source = "terraform-aws-modules/vpc/aws"
  
  name = "fasttex-vpc"
  cidr = "10.0.0.0/16"
  
  azs             = ["us-east-1a", "us-east-1b", "us-east-1c"]
  private_subnets = ["10.0.1.0/24", "10.0.2.0/24", "10.0.3.0/24"]
  public_subnets  = ["10.0.101.0/24", "10.0.102.0/24", "10.0.103.0/24"]
  
  enable_nat_gateway = true
}

# RDS
resource "aws_db_instance" "fasttex" {
  identifier     = "fasttex-db"
  engine         = "postgres"
  engine_version = "15"
  instance_class = "db.r6g.large"
  
  allocated_storage     = 100
  max_allocated_storage = 500
  storage_type          = "gp3"
  
  db_name  = "fasttex"
  username = "fasttex"
  password = var.db_password
  
  multi_az               = true
  db_subnet_group_name   = aws_db_subnet_group.fasttex.name
  vpc_security_group_ids = [aws_security_group.db.id]
  
  backup_retention_period = 7
  skip_final_snapshot     = false
}

# ElastiCache
resource "aws_elasticache_replication_group" "fasttex" {
  replication_group_id       = "fasttex-redis"
  description                = "FastTeX Redis cluster"
  node_type                  = "cache.r6g.large"
  num_node_groups            = 3
  replicas_per_node_group    = 2
  
  automatic_failover_enabled = true
  multi_az_enabled           = true
  
  subnet_group_name  = aws_elasticache_subnet_group.fasttex.name
  security_group_ids = [aws_security_group.redis.id]
}

# S3 Bucket
resource "aws_s3_bucket" "artifacts" {
  bucket = "fasttex-artifacts-${var.environment}"
}

resource "aws_s3_bucket_versioning" "artifacts" {
  bucket = aws_s3_bucket.artifacts.id
  versioning_configuration {
    status = "Enabled"
  }
}
```

### Google Cloud

- **Compute**: Cloud Run for Gateway, GKE for Orchestrator/Workers
- **Database**: Cloud SQL for PostgreSQL
- **Cache**: Memorystore for Redis
- **Storage**: Cloud Storage

### Azure

- **Compute**: Azure Container Apps for Gateway, AKS for Workers
- **Database**: Azure Database for PostgreSQL
- **Cache**: Azure Cache for Redis
- **Storage**: Azure Blob Storage

---

## Monitoring & Observability

### Prometheus & Grafana

```yaml
# prometheus.yml
global:
  scrape_interval: 15s

scrape_configs:
  - job_name: 'fasttex-gateway'
    static_configs:
      - targets: ['gateway:9090']
    
  - job_name: 'fasttex-orchestrator'
    static_configs:
      - targets: ['orchestrator:9090']

  - job_name: 'fasttex-workers'
    static_configs:
      - targets: ['worker1:9090', 'worker2:9090', 'worker3:9090']
```

### Key Metrics

| Metric | Description | Alert Threshold |
|--------|-------------|-----------------|
| `fasttex_compile_duration_seconds` | Compilation time | > 300s |
| `fasttex_compile_queue_length` | Jobs waiting | > 100 |
| `fasttex_worker_pool_available` | Available workers | < 2 |
| `fasttex_websocket_connections` | Active connections | > 10000 |
| `fasttex_cache_hit_ratio` | Cache effectiveness | < 0.7 |

### Logging (ELK Stack)

```yaml
# filebeat.yml
filebeat.inputs:
  - type: container
    paths:
      - '/var/lib/docker/containers/*/*.log'

output.elasticsearch:
  hosts: ["elasticsearch:9200"]
  index: "fasttex-%{+yyyy.MM.dd}"
```

---

## Backup & Recovery

### Database Backup

```bash
#!/bin/bash
# backup-db.sh

DATE=$(date +%Y%m%d_%H%M%S)
BACKUP_DIR="/backups/postgres"

# Create backup
pg_dump -h localhost -U fasttex -F c fasttex > ${BACKUP_DIR}/fasttex_${DATE}.dump

# Upload to S3
aws s3 cp ${BACKUP_DIR}/fasttex_${DATE}.dump s3://fasttex-backups/postgres/

# Clean old backups (keep 30 days)
find ${BACKUP_DIR} -name "*.dump" -mtime +30 -delete
```

### Disaster Recovery

1. **RPO (Recovery Point Objective)**: 1 hour
2. **RTO (Recovery Time Objective)**: 4 hours

```bash
# Restore database
pg_restore -h localhost -U fasttex -d fasttex -c fasttex_backup.dump

# Restore artifacts from S3
aws s3 sync s3://fasttex-backups/artifacts/ /var/lib/fasttex/artifacts/
```

---

## Scaling Guidelines

### Horizontal Scaling

| Component | Scale Trigger | Max Instances |
|-----------|---------------|---------------|
| Gateway | CPU > 70% | 10 |
| Orchestrator | Queue > 50 | 5 |
| Workers | Queue > 100 | 50 |

### Vertical Scaling

| Component | Min | Recommended | Max |
|-----------|-----|-------------|-----|
| Gateway | 2 CPU, 2GB | 4 CPU, 4GB | 8 CPU, 8GB |
| Orchestrator | 4 CPU, 4GB | 8 CPU, 8GB | 16 CPU, 16GB |
| Worker | 1 CPU, 512MB | 2 CPU, 1GB | 4 CPU, 2GB |

### Performance Tuning

```toml
# High-throughput configuration
[orchestrator]
max_concurrent_jobs = 32
job_timeout_seconds = 180

[worker]
pool_size = 16
memory_mb = 1024
cpu_count = 2

[cache]
fmt_ttl_days = 14
max_size_gb = 50
```
