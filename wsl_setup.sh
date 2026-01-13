#!/bin/bash
#
# Semantic OS - WSL Setup Script
# ==============================
#
# Run this inside WSL to set up Semantic OS:
#   cd /mnt/c/path/to/markdown_viewer
#   bash wsl_setup.sh
#

echo "========================================"
echo "  Semantic OS - WSL Setup"
echo "========================================"
echo

# Check if we're in WSL
if [ ! -f /proc/version ] || ! grep -qi microsoft /proc/version; then
    echo "Warning: This doesn't look like WSL"
    echo "Continue anyway? (y/n)"
    read -r response
    if [ "$response" != "y" ]; then
        exit 1
    fi
fi

# Check Python
echo "[1/4] Checking Python..."
if ! command -v python3 &> /dev/null; then
    echo "Python3 not found. Installing..."
    sudo apt update && sudo apt install -y python3 python3-pip
else
    echo "  Python3: $(python3 --version)"
fi

# Install required packages
echo
echo "[2/4] Installing Python packages..."
pip3 install --user sentence-transformers 2>/dev/null || echo "  (sentence-transformers optional)"

# Create data directory
echo
echo "[3/4] Creating data directory..."
mkdir -p ~/.semantic_os
echo "  Created: ~/.semantic_os"

# Test the tracer
echo
echo "[4/4] Testing syscall tracer..."
echo

cd "$(dirname "$0")"

# Quick test
python3 -c "
import sys
sys.path.insert(0, '.')

print('Testing kernel import...')
from kernel.memory import SemanticMemory, MemoryType
print('  Memory: OK')

print('Testing tracer import...')
from kernel.linux_tracer import SyscallTracer, Syscall
print('  Tracer: OK')

print()
print('All imports successful!')
" 2>&1

if [ $? -eq 0 ]; then
    echo
    echo "========================================"
    echo "  Setup Complete!"
    echo "========================================"
    echo
    echo "To start Semantic OS, run:"
    echo
    echo "  python3 semantic_linux.py"
    echo
    echo "Or trace a single command:"
    echo
    echo "  python3 semantic_linux.py ls -la"
    echo "  python3 semantic_linux.py cat /etc/passwd"
    echo
else
    echo
    echo "Setup encountered errors. Check the output above."
    exit 1
fi
