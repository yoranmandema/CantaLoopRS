# Phase 1.1: Start LSP server in TCP mode (dev-only)
# This allows restarting the server without reloading Cursor
# Usage: .\scripts\start-lsp-tcp.ps1

Write-Host "=== Starting CantaLoop LSP Server in TCP Mode ==="
Write-Host ""
Write-Host "Server will listen on: 127.0.0.1:9257"
Write-Host ""
Write-Host "To connect Cursor to TCP mode:"
Write-Host "  1. Set environment variable: `$env:CANTALOOP_LSP_TCP='1'"
Write-Host "  2. Use a TCP-to-stdio bridge tool (PowerShell/socat)"
Write-Host "  3. Or implement TCP support in extension.js"
Write-Host ""
Write-Host "Press Ctrl+C to stop the server"
Write-Host ""

# Start the server in TCP mode
$env:CANTALOOP_LSP_TCP = "1"
cargo run --bin cantaloop-lsp
