#!/bin/bash
# Check that LSP code doesn't contain panic-prone patterns
# This script enforces the "no panic in LSP builds" rule

set -e

echo "=== LSP No-Panic Check ==="
echo

FAILED=0

# Check for panic! macros in LSP
echo "Checking for panic! in src/lsp/..."
if grep -r "panic!" src/lsp/ 2>/dev/null; then
    echo "❌ ERROR: Found panic! in LSP code"
    FAILED=1
else
    echo "✓ No panic! found"
fi
echo

# Check for unwrap() calls in LSP
echo "Checking for .unwrap() in src/lsp/..."
if grep -r "\.unwrap()" src/lsp/ 2>/dev/null; then
    echo "❌ ERROR: Found .unwrap() in LSP code"
    FAILED=1
else
    echo "✓ No .unwrap() found"
fi
echo

# Check for expect() calls in LSP
echo "Checking for .expect( in src/lsp/..."
if grep -r "\.expect(" src/lsp/ 2>/dev/null; then
    echo "❌ ERROR: Found .expect() in LSP code"
    FAILED=1
else
    echo "✓ No .expect() found"
fi
echo

# Check for unreachable! macros in LSP
echo "Checking for unreachable! in src/lsp/..."
if grep -r "unreachable!" src/lsp/ 2>/dev/null; then
    echo "❌ ERROR: Found unreachable! in LSP code"
    FAILED=1
else
    echo "✓ No unreachable! found"
fi
echo

# Check for unimplemented! macros in LSP
echo "Checking for unimplemented! in src/lsp/..."
if grep -r "unimplemented!" src/lsp/ 2>/dev/null; then
    echo "❌ ERROR: Found unimplemented! in LSP code"
    FAILED=1
else
    echo "✓ No unimplemented! found"
fi
echo

# Check CST builder
echo "Checking src/core/cst/builder.rs..."
if grep "panic!\|\.unwrap()\|\.expect(" src/core/cst/builder.rs 2>/dev/null; then
    echo "❌ ERROR: Found panic-prone patterns in CST builder"
    FAILED=1
else
    echo "✓ CST builder is panic-free"
fi
echo

if [ $FAILED -eq 1 ]; then
    echo "❌ LSP No-Panic check FAILED"
    echo
    echo "The LSP must never panic on user input. Please replace:"
    echo "  - panic!() with proper error handling"
    echo "  - .unwrap() with .unwrap_or_else() or proper error propagation"
    echo "  - .expect() with .ok_or_else() or proper error propagation"
    echo "  - unreachable!() with proper error handling"
    echo "  - unimplemented!() with proper error handling or feature flags"
    exit 1
else
    echo "✅ All checks passed - LSP is panic-free!"
    exit 0
fi
