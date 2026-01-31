#!/bin/bash
#
# Add swap space to prevent OOM during parallel test builds
# Run as root: sudo ./add_swap.sh
#

set -e

SWAP_SIZE_GB=4
SWAP_FILE=/swapfile

if [ "$EUID" -ne 0 ]; then
    echo "Please run as root: sudo $0"
    exit 1
fi

# Check if swap already exists
if swapon --show | grep -q "$SWAP_FILE"; then
    echo "Swap file $SWAP_FILE already exists and is active"
    swapon --show
    exit 0
fi

echo "Creating ${SWAP_SIZE_GB}GB swap file at $SWAP_FILE..."

# Create swap file
fallocate -l ${SWAP_SIZE_GB}G $SWAP_FILE || dd if=/dev/zero of=$SWAP_FILE bs=1M count=$((SWAP_SIZE_GB * 1024))

# Set permissions
chmod 600 $SWAP_FILE

# Make it a swap file
mkswap $SWAP_FILE

# Enable swap
swapon $SWAP_FILE

# Verify
echo ""
echo "Swap activated:"
swapon --show
free -h

# Make it permanent
if ! grep -q "$SWAP_FILE" /etc/fstab; then
    echo ""
    echo "Adding to /etc/fstab for persistence..."
    echo "$SWAP_FILE none swap sw 0 0" >> /etc/fstab
    echo "Done! Swap will persist across reboots."
else
    echo "Swap entry already in /etc/fstab"
fi

# Optimize swappiness for build workloads (only swap when really needed)
sysctl vm.swappiness=10
if ! grep -q "vm.swappiness" /etc/sysctl.conf; then
    echo "vm.swappiness=10" >> /etc/sysctl.conf
fi

echo ""
echo "Swap configuration complete!"
