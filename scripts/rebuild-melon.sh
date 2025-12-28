#!/bin/bash
# Quick rebuild script for melon binary (debug mode - faster builds)
# Use this when developing the melon CLI tool

echo "Building melon binary (debug mode)..."
cargo build --bin melon

if [ $? -ne 0 ]; then
    echo "Build failed!"
    exit 1
fi

echo "melon binary rebuilt successfully!"
echo "Location: target/debug/melon"

