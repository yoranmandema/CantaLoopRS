#!/bin/bash
# Rebuild and package the CantaLoop language extension
# This script:
# 1. Builds the LSP server in release mode
# 2. Copies it to the extension directory
# 3. Packages the extension as a .vsix file

set -e

echo "Building LSP server..."
cargo build --release --bin cantaloop-lsp

if [ $? -ne 0 ]; then
    echo "Build failed!"
    exit 1
fi

echo "Copying LSP server to extension directory..."
# Detect OS and set appropriate paths
if [[ "$OSTYPE" == "msys" || "$OSTYPE" == "win32" || -n "$WINDIR" ]]; then
    SERVER_PATH="target/release/cantaloop-lsp.exe"
    EXTENSION_SERVER_PATH=".cantaloop-language/server/cantaloop-lsp.exe"
else
    SERVER_PATH="target/release/cantaloop-lsp"
    EXTENSION_SERVER_PATH=".cantaloop-language/server/cantaloop-lsp"
fi

if [ -f "$SERVER_PATH" ]; then
    cp "$SERVER_PATH" "$EXTENSION_SERVER_PATH"
    echo "LSP server copied successfully to $EXTENSION_SERVER_PATH"
else
    echo "LSP server binary not found at $SERVER_PATH"
    exit 1
fi

echo "Packaging extension..."
cd .cantaloop-language
vsce package --out cantaloop-language-0.0.1.vsix
if [ $? -eq 0 ]; then
    echo "Extension packaged successfully: .cantaloop-language/cantaloop-language-0.0.1.vsix"
else
    echo "Packaging failed!"
    exit 1
fi
cd ..

echo ""
echo "Done! Extension ready for installation."

