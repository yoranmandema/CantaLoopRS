#!/bin/bash

# Phase 1.1: Start LSP server in TCP mode (dev-only)
# This allows restarting the server without reloading Cursor
# Usage: ./scripts/start-lsp-tcp.sh

echo "=== Starting CantaLoop LSP Server in TCP Mode ==="
echo ""
echo "Server will listen on: 127.0.0.1:9257"
echo ""
echo "To connect Cursor to TCP mode:"
echo "  1. Set environment variable: export CANTALOOP_LSP_TCP=1"
echo "  2. Use a TCP-to-stdio bridge tool (socat/nc)"
echo "  3. Or implement TCP support in extension.js"
echo ""
echo "Press Ctrl+C to stop the server"
echo ""

# Start the server in TCP mode
CANTALOOP_LSP_TCP=1 cargo run --bin cantaloop-lsp
