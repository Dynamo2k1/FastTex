# FastTeX Infrastructure - Firecracker Configuration

> Complete guide to setting up Firecracker microVM workers for secure LaTeX compilation.

## Table of Contents

1. [Overview](#overview)
2. [Why Firecracker?](#why-firecracker)
3. [Prerequisites](#prerequisites)
4. [Installation](#installation)
5. [Configuration](#configuration)
6. [Building the Worker Image](#building-the-worker-image)
7. [Running Workers](#running-workers)
8. [Security Considerations](#security-considerations)
9. [Monitoring](#monitoring)
10. [Troubleshooting](#troubleshooting)

---

## Overview

FastTeX uses Firecracker microVMs to provide secure, isolated environments for LaTeX compilation. Each compilation job runs in a fresh, ephemeral VM that is destroyed after completion.

**Key Benefits:**
- Hardware-level isolation via KVM
- Sub-second VM startup times (~125ms)
- Minimal attack surface (~50k LoC VMM)
- Complete network isolation
- Strict resource limits

---

## Why Firecracker?

### Comparison with Alternatives

| Feature | Firecracker | Docker | gVisor | Kata |
|---------|-------------|--------|--------|------|
| Isolation | Hardware (KVM) | Namespaces | Syscall filter | Hardware |
| Startup time | ~125ms | ~1s | ~500ms | ~2s |
| Memory overhead | ~5MB | ~50MB | ~100MB | ~100MB |
| Attack surface | ~50k LoC | ~500k LoC | ~100k LoC | ~500k LoC |
| Container escape risk | Very low | Moderate | Low | Very low |

### Security Model

Firecracker is superior for FastTeX because:

1. **Shell-escape prevention**: LaTeX's `\write18` cannot affect the host
2. **No container escape**: Hardware virtualization boundary
3. **Network isolation**: Workers have zero network access
4. **Resource enforcement**: Kernel-level limits cannot be bypassed
5. **Ephemeral by design**: No persistent state in workers

---

## Prerequisites

### Hardware Requirements

- **CPU**: x86_64 with VT-x (Intel) or AMD-V (AMD)
- **RAM**: 8GB+ recommended
- **Disk**: 20GB+ for VM images

### Software Requirements

- **Linux kernel**: 5.0+ with KVM support
- **OS**: Ubuntu 22.04+, Debian 12+, Amazon Linux 2
- **Root access**: For KVM and network setup

### Verify KVM Support

```bash
# Check if KVM is available
ls -la /dev/kvm

# If not present, load KVM modules
sudo modprobe kvm
sudo modprobe kvm_intel  # For Intel CPUs
# OR
sudo modprobe kvm_amd    # For AMD CPUs

# Verify modules are loaded
lsmod | grep kvm
```

---

## Installation

### Step 1: Install Firecracker

```bash
# Set version
FIRECRACKER_VERSION="1.5.0"
ARCH="x86_64"

# Download Firecracker
curl -Lo /tmp/firecracker.tgz \
  https://github.com/firecracker-microvm/firecracker/releases/download/v${FIRECRACKER_VERSION}/firecracker-v${FIRECRACKER_VERSION}-${ARCH}.tgz

# Extract
tar -xzf /tmp/firecracker.tgz -C /tmp

# Install binaries
sudo mv /tmp/release-v${FIRECRACKER_VERSION}-${ARCH}/firecracker-v${FIRECRACKER_VERSION}-${ARCH} /usr/local/bin/firecracker
sudo mv /tmp/release-v${FIRECRACKER_VERSION}-${ARCH}/jailer-v${FIRECRACKER_VERSION}-${ARCH} /usr/local/bin/jailer

# Make executable
sudo chmod +x /usr/local/bin/firecracker /usr/local/bin/jailer

# Verify installation
firecracker --version
```

### Step 2: Download Linux Kernel

```bash
# Create directory
sudo mkdir -p /srv/fasttex/kernel

# Download compatible kernel
curl -Lo /srv/fasttex/kernel/vmlinux \
  https://s3.amazonaws.com/spec.ccfc.min/img/quickstart_guide/${ARCH}/kernels/vmlinux.bin

# Or build your own minimal kernel (recommended for production)
# See: https://github.com/firecracker-microvm/firecracker/blob/main/docs/rootfs-and-kernel-setup.md
```

### Step 3: Setup Permissions

```bash
# Create fasttex user
sudo useradd -r -s /bin/false fasttex

# Add to kvm group
sudo usermod -a -G kvm fasttex

# Create directories
sudo mkdir -p /srv/fasttex/{kernel,rootfs,work}
sudo mkdir -p /var/run/fasttex

# Set permissions
sudo chown -R fasttex:fasttex /srv/fasttex
sudo chown fasttex:fasttex /var/run/fasttex
```

---

## Configuration

### VM Configuration (config.json)

The configuration file defines VM resources:

```json
{
  "boot-source": {
    "kernel_image_path": "/srv/fasttex/kernel/vmlinux",
    "boot_args": "console=ttyS0 reboot=k panic=1 pci=off"
  },
  "machine-config": {
    "vcpu_count": 1,
    "mem_size_mib": 512,
    "smt": false,
    "track_dirty_pages": false
  },
  "drives": [
    {
      "drive_id": "rootfs",
      "path_on_host": "/srv/fasttex/rootfs/rootfs.ext4",
      "is_root_device": true,
      "is_read_only": true,
      "partuuid": null
    },
    {
      "drive_id": "work",
      "path_on_host": "/srv/fasttex/work/{JOB_ID}/work.ext4",
      "is_root_device": false,
      "is_read_only": false,
      "partuuid": null
    }
  ],
  "network-interfaces": [],
  "vsock": null,
  "logger": {
    "log_path": "/var/log/fasttex/{JOB_ID}/firecracker.log",
    "level": "Warning",
    "show_level": true,
    "show_log_origin": true
  },
  "metrics": {
    "metrics_path": "/var/log/fasttex/{JOB_ID}/metrics"
  }
}
```

### Resource Limits

| Resource | Default | Maximum | Notes |
|----------|---------|---------|-------|
| vCPUs | 1 | 4 | Single-threaded TeX |
| Memory | 512 MiB | 2048 MiB | Increase for large docs |
| Disk | 1 GiB | 5 GiB | Work volume |
| Time | 120s | 600s | Compilation timeout |

### Configuration by Document Type

| Document Type | vCPUs | Memory | Disk | Timeout |
|---------------|-------|--------|------|---------|
| Simple article | 1 | 256 MiB | 512 MiB | 60s |
| Thesis/book | 1 | 512 MiB | 1 GiB | 180s |
| Heavy TikZ | 2 | 1024 MiB | 2 GiB | 300s |

---

## Building the Worker Image

### Rootfs Requirements

The worker rootfs should contain:

1. **Tectonic** - Rust-based TeX engine
2. **BusyBox** - Minimal utilities
3. **Minimal libc** - Runtime library

### Build Script

```bash
#!/bin/bash
# build-rootfs.sh
set -euo pipefail

ROOTFS_SIZE_MB=256
ROOTFS_DIR=$(mktemp -d)
ROOTFS_IMAGE="/srv/fasttex/rootfs/rootfs.ext4"

echo "Building FastTeX worker rootfs..."

# Create directory structure
mkdir -p ${ROOTFS_DIR}/{bin,lib,lib64,usr/bin,etc,var/work,proc,sys,dev,tmp}

# Install BusyBox (statically linked)
curl -Lo ${ROOTFS_DIR}/bin/busybox \
  https://busybox.net/downloads/binaries/1.35.0-x86_64-linux-musl/busybox
chmod +x ${ROOTFS_DIR}/bin/busybox

# Create symlinks for common utilities
for cmd in sh ls cat cp mv rm mkdir echo; do
  ln -s busybox ${ROOTFS_DIR}/bin/$cmd
done

# Download Tectonic (statically linked)
TECTONIC_VERSION="0.15.0"
curl -Lo /tmp/tectonic.tar.gz \
  https://github.com/tectonic-typesetting/tectonic/releases/download/tectonic%40${TECTONIC_VERSION}/tectonic-${TECTONIC_VERSION}-x86_64-unknown-linux-musl.tar.gz
tar -xzf /tmp/tectonic.tar.gz -C ${ROOTFS_DIR}/usr/bin/

# Create minimal /etc files
cat > ${ROOTFS_DIR}/etc/passwd << EOF
root:x:0:0:root:/:/bin/sh
worker:x:1000:1000:worker:/var/work:/bin/sh
EOF

cat > ${ROOTFS_DIR}/etc/group << EOF
root:x:0:
worker:x:1000:
EOF

# Create init script
cat > ${ROOTFS_DIR}/init << 'EOF'
#!/bin/sh
mount -t proc proc /proc
mount -t sysfs sysfs /sys
mount -t devtmpfs devtmpfs /dev

# Run as worker user
exec su -s /bin/sh worker -c "cd /var/work && /usr/bin/tectonic $@"
EOF
chmod +x ${ROOTFS_DIR}/init

# Create ext4 image
echo "Creating ext4 image (${ROOTFS_SIZE_MB}MB)..."
dd if=/dev/zero of=${ROOTFS_IMAGE} bs=1M count=${ROOTFS_SIZE_MB}
mkfs.ext4 ${ROOTFS_IMAGE}

# Mount and copy files
MOUNT_DIR=$(mktemp -d)
sudo mount ${ROOTFS_IMAGE} ${MOUNT_DIR}
sudo cp -r ${ROOTFS_DIR}/* ${MOUNT_DIR}/
sudo umount ${MOUNT_DIR}

# Optimize filesystem
e2fsck -f ${ROOTFS_IMAGE}
tune2fs -O ^has_journal ${ROOTFS_IMAGE}

# Cleanup
rm -rf ${ROOTFS_DIR} ${MOUNT_DIR}

echo "Rootfs built: ${ROOTFS_IMAGE}"
echo "Size: $(du -h ${ROOTFS_IMAGE} | cut -f1)"
```

### Verify Rootfs

```bash
# Check contents
sudo mount /srv/fasttex/rootfs/rootfs.ext4 /mnt
ls -la /mnt
/mnt/usr/bin/tectonic --version
sudo umount /mnt
```

---

## Running Workers

### Start Script

```bash
#!/bin/bash
# start-vm.sh
set -euo pipefail

JOB_ID=${1:-$(uuidgen)}
SOCKET_PATH="/var/run/fasttex/${JOB_ID}.socket"
CONFIG_PATH="/srv/fasttex/config.json"

echo "Starting Firecracker VM for job: ${JOB_ID}"

# Create work volume
WORK_DIR="/srv/fasttex/work/${JOB_ID}"
mkdir -p ${WORK_DIR}
dd if=/dev/zero of=${WORK_DIR}/work.ext4 bs=1M count=100
mkfs.ext4 ${WORK_DIR}/work.ext4

# Create log directory
LOG_DIR="/var/log/fasttex/${JOB_ID}"
mkdir -p ${LOG_DIR}

# Prepare config
CONFIG=$(cat ${CONFIG_PATH} | sed "s/{JOB_ID}/${JOB_ID}/g")

# Start Firecracker
firecracker \
  --api-sock ${SOCKET_PATH} \
  --config-file <(echo "${CONFIG}") &

FC_PID=$!
echo "Firecracker PID: ${FC_PID}"

# Wait for socket
while [ ! -S ${SOCKET_PATH} ]; do
  sleep 0.1
done

echo "VM ready at socket: ${SOCKET_PATH}"

# Start the VM
curl --unix-socket ${SOCKET_PATH} \
  -X PUT "http://localhost/actions" \
  -H "Content-Type: application/json" \
  -d '{"action_type": "InstanceStart"}'

echo "VM started"
```

### Stop Script

```bash
#!/bin/bash
# stop-vm.sh
set -euo pipefail

JOB_ID=$1
SOCKET_PATH="/var/run/fasttex/${JOB_ID}.socket"

echo "Stopping VM for job: ${JOB_ID}"

# Send shutdown signal
curl --unix-socket ${SOCKET_PATH} \
  -X PUT "http://localhost/actions" \
  -H "Content-Type: application/json" \
  -d '{"action_type": "SendCtrlAltDel"}'

# Wait and force kill if needed
sleep 2
pkill -f "firecracker.*${JOB_ID}" || true

# Cleanup
rm -f ${SOCKET_PATH}
rm -rf /srv/fasttex/work/${JOB_ID}

echo "VM stopped and cleaned up"
```

### Using Jailer (Production)

For production, use jailer for additional security:

```bash
#!/bin/bash
# start-jailed-vm.sh

JOB_ID=$(uuidgen)
JAIL_PATH="/srv/jailer/firecracker/${JOB_ID}"

sudo jailer \
  --id ${JOB_ID} \
  --exec-file /usr/local/bin/firecracker \
  --chroot-base-dir /srv/jailer \
  --uid 1000 \
  --gid 1000 \
  -- \
  --config-file /config.json \
  --api-sock /run/firecracker.socket
```

---

## Security Considerations

### Network Isolation

Workers have NO network access by default:
- No `network-interfaces` in config
- No vsock device
- Cannot make outbound connections

### Filesystem Security

1. **Read-only rootfs**: Base system cannot be modified
2. **Separate work volume**: Only work data is writable
3. **No persistence**: VMs destroyed after each job

### Shell Escape Prevention

Even with Firecracker isolation, we also:
1. Run Tectonic with `--untrusted` flag
2. Don't set `shell_escape` environment variable
3. Use restricted PATH

### Resource Limits

```bash
# CPU time limit (cgroups)
echo "1000000" > /sys/fs/cgroup/cpu/fasttex/${JOB_ID}/cpu.cfs_quota_us

# Memory limit
echo "512M" > /sys/fs/cgroup/memory/fasttex/${JOB_ID}/memory.limit_in_bytes
```

---

## Monitoring

### Metrics

Firecracker exposes metrics via file:

```bash
# Read metrics
cat /var/log/fasttex/${JOB_ID}/metrics
```

**Key Metrics:**
- `vcpu.exit_count` - Number of vCPU exits
- `block.read_count` - Disk reads
- `block.write_count` - Disk writes
- `net.*` - Network (should be 0)

### Prometheus Integration

```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'firecracker'
    static_configs:
      - targets: ['localhost:9100']
    file_sd_configs:
      - files:
          - /var/log/fasttex/*/metrics
```

### Logging

```bash
# View VM logs
tail -f /var/log/fasttex/${JOB_ID}/firecracker.log

# View all active VMs
ls /var/run/fasttex/*.socket
```

---

## Troubleshooting

### Common Issues

#### 1. "KVM is not available"

```bash
# Check if KVM is loaded
lsmod | grep kvm

# Load KVM module
sudo modprobe kvm_intel  # or kvm_amd

# Check permissions
ls -la /dev/kvm
# Should be: crw-rw---- 1 root kvm ...

# Add user to kvm group
sudo usermod -a -G kvm $(whoami)
newgrp kvm
```

#### 2. "Permission denied" on socket

```bash
# Check socket permissions
ls -la /var/run/fasttex/

# Fix permissions
sudo chown fasttex:fasttex /var/run/fasttex
```

#### 3. VM fails to start

```bash
# Check Firecracker logs
cat /var/log/fasttex/${JOB_ID}/firecracker.log

# Verify kernel exists
ls -la /srv/fasttex/kernel/vmlinux

# Verify rootfs
file /srv/fasttex/rootfs/rootfs.ext4
```

#### 4. Compilation timeout

```bash
# Increase timeout in config
"machine-config": {
  "vcpu_count": 2,
  "mem_size_mib": 1024
}
```

#### 5. Out of disk space

```bash
# Check work volume
df -h /srv/fasttex/work/

# Cleanup old jobs
find /srv/fasttex/work -type d -mmin +60 -exec rm -rf {} +
```

### Getting Help

- **Firecracker Documentation**: https://github.com/firecracker-microvm/firecracker/tree/main/docs
- **FastTeX Issues**: https://github.com/fasttex/fasttex/issues
- **Discord**: https://discord.gg/fasttex
