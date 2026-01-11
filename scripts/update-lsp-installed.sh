#!/bin/bash
# Complete workflow to build and update the LSP executable in the installed extension
# This script:
# 1. Builds the LSP server
# 2. Copies to local dev directory (always works)
# 3. Attempts to copy to installed extension directory (may fail if Cursor is running)

set -e

echo "=========================================="
echo "LSP Update Workflow"
echo "=========================================="
echo ""

# Step 1: Build the LSP
echo "Step 1: Building LSP server (debug mode)..."
cargo build --bin cantaloop-lsp

if [ $? -ne 0 ]; then
    echo "❌ Build failed!"
    exit 1
fi

echo "✅ Build successful!"
echo ""

# Detect OS and set paths
if [[ "$OSTYPE" == "msys" || "$OSTYPE" == "win32" || -n "$WINDIR" ]]; then
    SERVER_PATH="target/debug/cantaloop-lsp.exe"
    LOCAL_EXT_PATH=".cantaloop-language/server/cantaloop-lsp.exe"
    INSTALLED_EXT_PATH="$HOME/.cursor/extensions/yoran.cantaloop-language-0.0.1/server/cantaloop-lsp.exe"
else
    SERVER_PATH="target/debug/cantaloop-lsp"
    LOCAL_EXT_PATH=".cantaloop-language/server/cantaloop-lsp"
    INSTALLED_EXT_PATH="$HOME/.cursor/extensions/yoran.cantaloop-language-0.0.1/server/cantaloop-lsp"
fi

# Step 2: Copy to local dev directory (always works)
echo "Step 2: Copying to local dev directory..."
if [ -f "$SERVER_PATH" ]; then
    mkdir -p "$(dirname "$LOCAL_EXT_PATH")"
    cp -f "$SERVER_PATH" "$LOCAL_EXT_PATH"
    echo "✅ Copied to: $LOCAL_EXT_PATH"
else
    echo "❌ LSP server binary not found at $SERVER_PATH"
    exit 1
fi
echo ""

# Step 3: Attempt to copy to installed extension directory
echo "Step 3: Copying to installed extension directory..."
if [ -f "$INSTALLED_EXT_PATH" ]; then
    # Check if file is locked (on Windows, this is tricky, so we just try)
    if cp -f "$SERVER_PATH" "$INSTALLED_EXT_PATH" 2>/dev/null; then
        echo "✅ Copied to: $INSTALLED_EXT_PATH"
        echo ""
        echo "=========================================="
        echo "✅ SUCCESS! LSP updated in installed extension"
        echo "=========================================="
        echo ""
        echo "Next steps:"
        echo "  1. Restart Cursor completely"
        echo "  2. Check LSP output log for the new Build ID"
        echo ""
    else
        echo "⚠️  Copy failed - file is locked (Cursor is probably running)"
        echo ""
        echo "=========================================="
        echo "⚠️  MANUAL STEP REQUIRED"
        echo "=========================================="
        echo ""
        echo "The LSP was built successfully and copied to the local dev directory."
        echo "However, the installed extension directory is locked."
        echo ""
        echo "To complete the update:"
        echo "  1. Close Cursor completely (all windows)"
        echo "  2. Run this command:"
        echo "     cp $SERVER_PATH $INSTALLED_EXT_PATH"
        echo "  3. Or run the task: 'Copy LSP to Extension'"
        echo "  4. Restart Cursor"
        echo ""
        echo "The executable is ready at: $SERVER_PATH"
        echo ""
    fi
else
    echo "⚠️  Installed extension directory not found: $(dirname "$INSTALLED_EXT_PATH")"
    echo "   (This is normal if the extension isn't installed)"
    echo ""
    echo "✅ LSP updated in local dev directory only"
    echo ""
fi
