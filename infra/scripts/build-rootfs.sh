#!/bin/bash
# build-rootfs.sh - Build the FastTeX worker root filesystem
#
# This script creates a minimal ext4 filesystem containing:
# - Alpine Linux base
# - Tectonic LaTeX engine
# - FastTeX worker binary
# - Required fonts and packages
#
# The resulting rootfs is read-only and contains no secrets.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOTFS_SIZE_MB=1024
OUTPUT_DIR="${SCRIPT_DIR}/../firecracker"
ROOTFS_PATH="${OUTPUT_DIR}/rootfs.ext4"
MOUNT_POINT="/tmp/fasttex-rootfs-$$"

# Check if running as root
if [[ $EUID -ne 0 ]]; then
    echo "This script must be run as root"
    exit 1
fi

echo "Building FastTeX worker rootfs..."

# Create the ext4 filesystem
echo "Creating ${ROOTFS_SIZE_MB}MB ext4 filesystem..."
dd if=/dev/zero of="${ROOTFS_PATH}" bs=1M count=${ROOTFS_SIZE_MB}
mkfs.ext4 "${ROOTFS_PATH}"

# Mount the filesystem
mkdir -p "${MOUNT_POINT}"
mount -o loop "${ROOTFS_PATH}" "${MOUNT_POINT}"

# Ensure cleanup on exit
cleanup() {
    umount "${MOUNT_POINT}" 2>/dev/null || true
    rmdir "${MOUNT_POINT}" 2>/dev/null || true
}
trap cleanup EXIT

# Install Alpine Linux base system
echo "Installing Alpine Linux base..."
ALPINE_VERSION="3.19"
ALPINE_MIRROR="https://dl-cdn.alpinelinux.org/alpine/v${ALPINE_VERSION}/main"

# Download and extract Alpine minirootfs
ARCH="x86_64"
ALPINE_ROOTFS="alpine-minirootfs-${ALPINE_VERSION}.0-${ARCH}.tar.gz"
ALPINE_URL="${ALPINE_MIRROR}/../releases/${ARCH}/${ALPINE_ROOTFS}"

if [[ ! -f "/tmp/${ALPINE_ROOTFS}" ]]; then
    echo "Downloading Alpine rootfs..."
    curl -Lo "/tmp/${ALPINE_ROOTFS}" "${ALPINE_URL}"
fi

tar xzf "/tmp/${ALPINE_ROOTFS}" -C "${MOUNT_POINT}"

# Configure Alpine
cat > "${MOUNT_POINT}/etc/resolv.conf" << EOF
nameserver 8.8.8.8
nameserver 8.8.4.4
EOF

# Create init script
cat > "${MOUNT_POINT}/sbin/init" << 'EOF'
#!/bin/sh
# FastTeX Worker Init Script

# Mount essential filesystems
mount -t proc proc /proc
mount -t sysfs sysfs /sys
mount -t devtmpfs devtmpfs /dev

# Mount the work filesystem
mkdir -p /work
mount /dev/vdb /work

# Set up environment
export HOME=/root
export PATH=/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin
export TEXMFHOME=/work/.texmf

# Execute the worker binary
exec /usr/local/bin/fasttex-worker
EOF
chmod +x "${MOUNT_POINT}/sbin/init"

# Install required packages via chroot
echo "Installing packages..."
cat > "${MOUNT_POINT}/install-packages.sh" << 'EOF'
#!/bin/sh
apk update
apk add --no-cache \
    ca-certificates \
    libstdc++ \
    fontconfig \
    freetype \
    harfbuzz \
    icu-libs \
    libpng \
    zlib

# Create necessary directories
mkdir -p /usr/local/bin
mkdir -p /work
EOF
chmod +x "${MOUNT_POINT}/install-packages.sh"
chroot "${MOUNT_POINT}" /install-packages.sh
rm "${MOUNT_POINT}/install-packages.sh"

# Copy Tectonic binary (would be built separately)
echo "Note: Tectonic binary must be copied to ${MOUNT_POINT}/usr/local/bin/tectonic"

# Copy FastTeX worker binary (would be built separately)
echo "Note: Worker binary must be copied to ${MOUNT_POINT}/usr/local/bin/fasttex-worker"

# Clean up
echo "Cleaning up..."
rm -rf "${MOUNT_POINT}/var/cache/apk"/*

# Set permissions
chmod 755 "${MOUNT_POINT}"

echo "Rootfs created successfully at ${ROOTFS_PATH}"
echo "Size: $(du -h "${ROOTFS_PATH}" | cut -f1)"

# Verify
echo ""
echo "Rootfs contents:"
ls -la "${MOUNT_POINT}"
