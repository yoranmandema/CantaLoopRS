# Contract Freeze Status

## Overview

This document tracks the completion of critical contract freezes before Phase 5 implementation.
These are structural decisions that are hardest to change later.

## ✅ Completed

### 1. CompilerSnapshot Contract (Frozen)

**Status**: ✅ **COMPLETE**

**Changes Made**:
- Made all fields private
- Added public constructor `CompilerSnapshot::new()`
- Added accessor methods:
  - `cst(file_id)` - Get CST for a file
  - `hir()` - Get HIR
  - `diagnostics(file_id)` - Get diagnostics for a file
  - `symbol_at(file_id, span)` - Get symbol at span
  - `spans_for_symbol(symbol_id)` - Get all reference spans
  - `definition_span_for_symbol(symbol_id)` - Get definition span
  - `symbol_info(symbol_id)` - Get symbol metadata
  - `symbols_at_offset(byte_offset)` - Find symbols at byte position
  - `file_ids()` - Get all file IDs
  - `has_diagnostics(file_id)` - Check if file has diagnostics
  - `has_symbols()` - Check if symbol table is available

**Documentation Added**:
- Comprehensive doc comments explaining the stability boundary
- Clear explanation that this defines "what any future IDE backend is allowed to know"
- Notes about immutability and read-only nature

**Files Modified**:
- `src/core/lsp_api.rs` - Made fields private, added accessors
- `src/core/compiler_state.rs` - Updated to use `new()` constructor
- `src/lsp/handlers/goto.rs` - Updated to use `symbols_at_offset()` method
- `src/lsp/handlers/hover.rs` - Updated to use `symbols_at_offset()` method
- `src/lsp/handlers/tokens.rs` - Updated to use `symbol_table()` method

**Result**: CompilerSnapshot is now a true stability boundary with no direct field access.

### 2. Span Semantics Normalization

**Status**: ✅ **COMPLETE**

**Changes Made**:
- Created `src/core/span.rs` with `CanonicalSpan` type
- Defined explicit conversion adapters:
  - `CstSpan` → `CanonicalSpan`
  - `CanonicalSpan` → `CstSpan`
  - `HirSpan` → `CanonicalSpan`
  - `CanonicalSpan` → `HirSpan`
  - `CstSpan` → `HirSpan` (via canonical)
  - `HirSpan` → `CstSpan` (via canonical)
- Added comprehensive documentation about span lifecycle
- Added unit tests for conversions

**Documentation Added**:
- Clear explanation of canonical span as byte-based (usize)
- Conversion path diagram showing explicit adapters
- Notes about when conversions are safe/when they may panic
- Rationale for explicit conversions (prevents subtle bugs)

**Files Created**:
- `src/core/span.rs` - Canonical span type and adapters

**Result**: Single canonical span type with explicit, documented conversion paths.
Conversion bugs are now much harder to introduce.

## ✅ Completed

### 3. Symbol Stability Rules

**Status**: ✅ **COMPLETE**

**Changes Made**:
- ✅ Defined `SymbolStability` enum in `src/core/lsp_api.rs`:
  - `UserDefined`: User-written symbols (functions, variables, modules, structs)
  - `CompilerGenerated`: Compiler-created symbols (reserved for future desugaring, closures)
  - `Unstable`: Symbols that may disappear between snapshots (incomplete compilation, error recovery)
- ✅ Added `stability: SymbolStability` field to `SymbolInfo`
- ✅ Updated `CompilerState::build_semantic_index()` to classify symbols:
  - Symbols with definition spans → `UserDefined`
  - Symbols without definition spans → `Unstable`
  - `CompilerGenerated` reserved for future use
- ✅ Added helper methods to `CompilerSnapshot`:
  - `can_rename(symbol_id)` - Check if symbol is safe for rename (only UserDefined)
  - `has_reliable_references(symbol_id)` - Check if references are reliable (UserDefined or CompilerGenerated)

**Documentation Added**:
- Comprehensive documentation explaining each stability level
- Clear guidance on which operations are safe for each level
- Rationale for classification rules

**Files Modified**:
- `src/core/lsp_api.rs` - Added `SymbolStability` enum, updated `SymbolInfo`, added helper methods
- `src/core/compiler_state.rs` - Updated `build_semantic_index()` to classify symbols

**Impact**: Now we can safely implement rename, filter references, and code actions based on symbol stability.

## 📋 Summary

### Completed (3/3)
- ✅ CompilerSnapshot contract frozen
- ✅ Span semantics normalized
- ✅ Symbol stability rules codified

### Status: **ALL CRITICAL CONTRACTS FROZEN** ✅

### Next Actions
1. Complete symbol stability rules
2. Proceed to Phase 5 (effect diagnostics)

---

**Last Updated**: After completing CompilerSnapshot freeze and span normalization.
