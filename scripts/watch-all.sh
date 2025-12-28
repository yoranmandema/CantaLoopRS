#!/bin/bash
# Watch script for both melon and LSP - auto-rebuilds on file changes
# Requires: cargo install cargo-watch
# 
# This uses GNU parallel or runs them in separate terminal windows
# For LSP: After rebuild, restart extension host (Ctrl+Shift+P -> "Developer: Restart Extension Host")

echo "Starting watch mode for both melon and LSP..."
echo "This will automatically rebuild both binaries when source files change."
echo "For LSP: After rebuild, restart extension host:"
echo "  Ctrl+Shift+P -> 'Developer: Restart Extension Host'"
echo ""
echo "Note: This script runs both watchers. You may want to run them in separate terminals:"
echo "  Terminal 1: ./watch-melon.sh"
echo "  Terminal 2: ./watch-lsp.sh"
echo ""

# Check if parallel is available
if command -v parallel &> /dev/null; then
    echo "Using GNU parallel to run both watchers..."
    parallel ::: \
        "cargo watch -x 'build --bin melon' | sed 's/^/[MELON] /'" \
        "cargo watch -x 'build --bin cantaloop-lsp' -s 'cp target/debug/cantaloop-lsp .cantaloop-language/server/cantaloop-lsp && echo -e \"\\n[LSP] Rebuilt! Restart extension host.\"' | sed 's/^/[LSP] /'"
else
    echo "GNU parallel not found. Running LSP watcher (melon watcher would need separate terminal)."
    echo "Install parallel with: sudo apt-get install parallel (Linux) or brew install parallel (macOS)"
    echo ""
    SERVER_PATH="target/debug/cantaloop-lsp"
    EXTENSION_SERVER_PATH=".cantaloop-language/server/cantaloop-lsp"
    cargo watch -x "build --bin cantaloop-lsp" -s "cp $SERVER_PATH $EXTENSION_SERVER_PATH && echo -e '\n[LSP] Rebuilt! Restart extension host to use new binary.'"
fi

