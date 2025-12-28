#!/bin/bash
# Watch script for melon binary - auto-rebuilds on file changes
# Requires: cargo install cargo-watch

echo "Starting watch mode for melon binary..."
echo "This will automatically rebuild melon when source files change."
echo "Press Ctrl+C to stop."
echo ""

cargo watch -x "build --bin melon"

