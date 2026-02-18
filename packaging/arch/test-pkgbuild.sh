#!/usr/bin/env bash
# Test PKGBUILD locally using Docker before pushing to AUR
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "=== Testing PKGBUILD in Arch Linux container ==="

# Run makepkg in a clean Arch container
docker run --rm \
  -v "$SCRIPT_DIR/rustledger:/pkg:ro" \
  archlinux:latest \
  bash -c '
    set -euo pipefail

    # Setup
    pacman -Syu --noconfirm
    pacman -S --noconfirm base-devel git sudo

    # Create build user (makepkg refuses to run as root)
    useradd -m builder
    echo "builder ALL=(ALL) NOPASSWD: ALL" >> /etc/sudoers

    # Create build directory and copy PKGBUILD
    mkdir -p /home/builder/build
    cp -r /pkg/* /home/builder/build/
    chown -R builder:builder /home/builder/build
    cd /home/builder/build

    echo ""
    echo "=== Running makepkg ==="
    su builder -c "makepkg -sf --noconfirm"

    echo ""
    echo "=== Build successful ==="
    ls -la *.pkg.tar.zst 2>/dev/null || ls -la *.pkg.tar.*
  '

echo ""
echo "✓ PKGBUILD test passed - safe to push to AUR"
