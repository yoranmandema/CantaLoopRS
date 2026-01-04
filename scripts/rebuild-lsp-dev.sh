#!/bin/bash
# Quick rebuild script for LSP development (debug mode - faster builds)
# This just rebuilds and copies the LSP binary, skipping packaging
# Use this when developing the LSP server itself

set -e

echo "Building LSP server (debug mode)..."
cargo build --bin cantaloop-lsp

if [ $? -ne 0 ]; then
    echo "Build failed!"
    exit 1
fi

echo "Copying LSP server to extension directory..."
# Detect OS and set appropriate paths
if [[ "$OSTYPE" == "msys" || "$OSTYPE" == "win32" || -n "$WINDIR" ]]; then
    SERVER_PATH="target/debug/cantaloop-lsp.exe"
    EXTENSION_SERVER_PATH=".cantaloop-language/server/cantaloop-lsp.exe"
else
    SERVER_PATH="target/debug/cantaloop-lsp"
    EXTENSION_SERVER_PATH=".cantaloop-language/server/cantaloop-lsp"
fi

if [ -f "$SERVER_PATH" ]; then
    cp "$SERVER_PATH" "$EXTENSION_SERVER_PATH"
    echo "LSP server copied successfully (debug build) to $EXTENSION_SERVER_PATH"
    echo "You can now restart the LSP server in your development extension window"
    echo "Use Ctrl+Shift+P -> 'Developer: Reload Window' or restart the extension host"
else
    echo "LSP server binary not found at $SERVER_PATH"
    exit 1
fi

