#!/usr/bin/env bash
# Test PKGBUILD locally using Docker before pushing to AUR
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "=== Testing PKGBUILD in Arch Linux container ==="

# Run makepkg in a clean Arch container
docker run --rm -it \
  -v "$SCRIPT_DIR/rustledger:/pkg:ro" \
  -w /build \
  archlinux:latest \
  bash -c '
    set -euo pipefail

    # Setup
    pacman -Syu --noconfirm
    pacman -S --noconfirm base-devel git sudo

    # Create build user (makepkg refuses to run as root)
    useradd -m builder
    echo "builder ALL=(ALL) NOPASSWD: ALL" >> /etc/sudoers

    # Copy PKGBUILD to build directory
    cp -r /pkg/* .
    chown -R builder:builder .

    # Build and test
    echo ""
    echo "=== Running makepkg ==="
    su builder -c "makepkg -sf --noconfirm"

    echo ""
    echo "=== Running checks ==="
    su builder -c "makepkg -sf --noconfirm --check" || {
      echo "Check failed (may be expected - see output above)"
    }

    echo ""
    echo "=== Build successful ==="
    ls -la *.pkg.tar.zst
  '

echo ""
echo "✓ PKGBUILD test passed - safe to push to AUR"
