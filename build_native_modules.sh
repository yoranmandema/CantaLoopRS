#!/bin/bash
# Build script template for native modules
# This script compiles native/modules.rs and generates registration code
#
# Usage: Place this in your project root and run: ./build_native_modules.sh

set -e

PROJECT_ROOT="$(pwd)"
NATIVE_DIR="$PROJECT_ROOT/native"
MODULES_RS="$NATIVE_DIR/modules.rs"

if [ ! -f "$MODULES_RS" ]; then
    echo "No native/modules.rs found. Skipping native module build."
    exit 0
fi

echo "Building native modules..."

# This would compile native/modules.rs and generate registration code
# For now, this is a template showing the intended workflow

echo "Native modules build complete."
echo "Note: Automatic loading requires integration with melon build system."
echo "See native/README.md for manual registration instructions."

