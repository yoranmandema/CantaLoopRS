#!/bin/bash
# Watch script for LSP server - auto-rebuilds and copies on file changes
# Requires: cargo install cargo-watch
# 
# After rebuild completes, restart the extension host in your Extension Development Host window:
# Ctrl+Shift+P -> "Developer: Restart Extension Host"

echo "Starting watch mode for LSP server..."
echo "This will automatically rebuild and copy the LSP binary when source files change."
echo "After each rebuild, restart the extension host:"
echo "  Ctrl+Shift+P -> 'Developer: Restart Extension Host'"
echo "Press Ctrl+C to stop."
echo ""

SERVER_PATH="target/debug/cantaloop-lsp"
EXTENSION_SERVER_PATH=".cantaloop-language/server/cantaloop-lsp"

cargo watch -x "build --bin cantaloop-lsp" -s "cp $SERVER_PATH $EXTENSION_SERVER_PATH && echo -e '\n[WATCH] LSP rebuilt! Restart extension host to use new binary.'"

