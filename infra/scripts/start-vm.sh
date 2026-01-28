#!/bin/bash
# start-vm.sh - Start a Firecracker microVM for FastTeX compilation
#
# Usage: ./start-vm.sh <job_id> [config_override.json]
#
# This script:
# 1. Creates a work filesystem for the job
# 2. Starts Firecracker with the configured settings
# 3. Handles cleanup on exit

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INFRA_DIR="${SCRIPT_DIR}/.."
CONFIG_TEMPLATE="${INFRA_DIR}/firecracker/config.json"

# Check arguments
if [[ $# -lt 1 ]]; then
    echo "Usage: $0 <job_id> [config_override.json]"
    exit 1
fi

JOB_ID="$1"
CONFIG_OVERRIDE="${2:-}"

# Paths
JOB_DIR="/tmp/fasttex/${JOB_ID}"
WORK_FS="${JOB_DIR}/work.ext4"
SOCKET_PATH="${JOB_DIR}/firecracker.sock"
LOG_PATH="${JOB_DIR}/firecracker.log"
METRICS_PATH="${JOB_DIR}/metrics.fifo"
CONFIG_PATH="${JOB_DIR}/config.json"

# Cleanup function
cleanup() {
    echo "Cleaning up job ${JOB_ID}..."
    
    # Kill Firecracker if running
    if [[ -S "${SOCKET_PATH}" ]]; then
        curl -s --unix-socket "${SOCKET_PATH}" \
            -X PUT "http://localhost/actions" \
            -H "Content-Type: application/json" \
            -d '{"action_type": "SendCtrlAltDel"}' || true
        sleep 1
    fi
    
    # Remove the socket
    rm -f "${SOCKET_PATH}"
    
    # Note: We don't remove JOB_DIR immediately to allow log collection
    echo "Cleanup complete. Logs available at ${LOG_PATH}"
}

# Create job directory
echo "Setting up job ${JOB_ID}..."
mkdir -p "${JOB_DIR}"

# Create work filesystem (64MB for compilation artifacts)
echo "Creating work filesystem..."
dd if=/dev/zero of="${WORK_FS}" bs=1M count=64 2>/dev/null
mkfs.ext4 -q "${WORK_FS}"

# Create metrics FIFO
mkfifo "${METRICS_PATH}" 2>/dev/null || true

# Generate config from template
echo "Generating Firecracker config..."
sed -e "s/{job_id}/${JOB_ID}/g" "${CONFIG_TEMPLATE}" > "${CONFIG_PATH}"

# Apply config override if provided
if [[ -n "${CONFIG_OVERRIDE}" && -f "${CONFIG_OVERRIDE}" ]]; then
    echo "Applying config override..."
    # In production, use jq to merge configs
    # jq -s '.[0] * .[1]' "${CONFIG_PATH}" "${CONFIG_OVERRIDE}" > "${CONFIG_PATH}.tmp"
    # mv "${CONFIG_PATH}.tmp" "${CONFIG_PATH}"
fi

# Check for Firecracker binary
if ! command -v firecracker &> /dev/null; then
    echo "Error: firecracker binary not found in PATH"
    echo "Download from: https://github.com/firecracker-microvm/firecracker/releases"
    exit 1
fi

# Check for required files
KERNEL_PATH="/srv/fasttex/vmlinux"
ROOTFS_PATH="/srv/fasttex/rootfs.ext4"

if [[ ! -f "${KERNEL_PATH}" ]]; then
    echo "Error: Kernel not found at ${KERNEL_PATH}"
    echo "Download a Firecracker-compatible kernel from:"
    echo "https://github.com/firecracker-microvm/firecracker/blob/main/docs/getting-started.md"
    exit 1
fi

if [[ ! -f "${ROOTFS_PATH}" ]]; then
    echo "Error: Rootfs not found at ${ROOTFS_PATH}"
    echo "Run ./build-rootfs.sh to create it"
    exit 1
fi

# Set up cleanup trap
trap cleanup EXIT

# Start Firecracker
echo "Starting Firecracker microVM..."
firecracker \
    --api-sock "${SOCKET_PATH}" \
    --config-file "${CONFIG_PATH}" \
    &

FIRECRACKER_PID=$!
echo "Firecracker started with PID ${FIRECRACKER_PID}"

# Wait for API socket
echo "Waiting for API socket..."
for i in {1..30}; do
    if [[ -S "${SOCKET_PATH}" ]]; then
        echo "API socket ready"
        break
    fi
    sleep 0.1
done

if [[ ! -S "${SOCKET_PATH}" ]]; then
    echo "Error: API socket not created"
    exit 1
fi

# Start the VM
echo "Starting VM instance..."
curl -s --unix-socket "${SOCKET_PATH}" \
    -X PUT "http://localhost/actions" \
    -H "Content-Type: application/json" \
    -d '{"action_type": "InstanceStart"}'

echo ""
echo "VM started successfully!"
echo "  Job ID: ${JOB_ID}"
echo "  Socket: ${SOCKET_PATH}"
echo "  Logs: ${LOG_PATH}"
echo ""
echo "To stop the VM: curl --unix-socket ${SOCKET_PATH} -X PUT http://localhost/actions -d '{\"action_type\": \"SendCtrlAltDel\"}'"

# Wait for Firecracker to exit
wait ${FIRECRACKER_PID}
