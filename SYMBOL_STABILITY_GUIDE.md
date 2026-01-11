# Symbol Stability Guide

## Overview

This document explains how symbol stability classification works and how to use it in LSP features.

## Stability Levels

### UserDefined ✅

**What**: Symbols written by the user in source code.

**Examples**:
- Functions: `fn myFunc() -> num { ... }`
- Variables: `let x = 42`
- Parameters: `fn f(x: num) { ... }`
- Modules: `mod MyModule { ... }`
- Structs: `struct Point { x: num, y: num }`

**Characteristics**:
- Has a definition span in source code
- Persists across compilation snapshots
- Reliable and predictable

**Safe for**:
- ✅ Rename operations
- ✅ Find all references
- ✅ Code actions
- ✅ Refactoring
- ✅ Hover information
- ✅ Go-to definition

**When to use**: These are the primary symbols that users interact with. Always prefer these for user-facing features.

---

### CompilerGenerated 🔧

**What**: Symbols created by the compiler (currently reserved for future use).

**Future Examples** (when implemented):
- Desugared constructs (e.g., `for` loops → `while` loops)
- Closure captures
- Anonymous functions created by partial application
- Temporary variables introduced during compilation

**Characteristics**:
- Created by compiler, not written by user
- May have generated names (e.g., `__closure_123`)
- Stable within a compilation but representation may change

**Safe for**:
- ✅ Hover information (read-only)
- ✅ Debugging information
- ✅ Find references (within same snapshot)

**NOT safe for**:
- ❌ Rename (user cannot rename compiler internals)
- ❌ Code actions that modify (may break on next compilation)
- ❌ Cross-snapshot reference tracking

**When to use**: When displaying compiler-internal information for debugging or advanced tooling.

---

### Unstable ⚠️

**What**: Symbols that may appear or disappear between snapshots.

**Examples**:
- Symbols from incomplete compilations (syntax errors)
- Error recovery symbols (may not actually exist)
- Symbols from missing source information

**Characteristics**:
- No definition span in source
- May not exist in next snapshot
- Best-effort information only

**Safe for**:
- ✅ Hover information (best-effort, with warnings)

**NOT safe for**:
- ❌ Rename operations
- ❌ Reliable reference finding
- ❌ Code actions
- ❌ Any modification operations

**When to use**: Only for best-effort hover information when a symbol is referenced but definition is missing.

---

## Classification Rules

### Current Implementation

Symbols are classified in `CompilerState::build_semantic_index()`:

```rust
let stability = if symbol.defined_at.is_some() {
    SymbolStability::UserDefined
} else {
    SymbolStability::Unstable
};
```

**Rule**: If a symbol has a definition span → `UserDefined`, otherwise → `Unstable`.

**Future Extension**: When we add compiler-generated symbols (desugaring, closures), we'll check:
- Has definition span in user source → `UserDefined`
- Has definition span but is compiler-created → `CompilerGenerated`
- No definition span → `Unstable`

---

## Using Stability in LSP Features

### Example: Rename

```rust
// Check if symbol can be renamed
if !snapshot.can_rename(symbol_id) {
    return Err("Cannot rename: symbol is not user-defined");
}

// Proceed with rename
let references = snapshot.spans_for_symbol(symbol_id)?;
// ...
```

### Example: References

```rust
// Check if references are reliable
if !snapshot.has_reliable_references(symbol_id) {
    // Warn user that references may be incomplete
    log_warning("References for this symbol may be incomplete");
}

let references = snapshot.spans_for_symbol(symbol_id)?;
// ...
```

### Example: Hover

```rust
let info = snapshot.symbol_info(symbol_id)?;
match info.stability {
    SymbolStability::UserDefined => {
        // Show full information with confidence
    }
    SymbolStability::CompilerGenerated => {
        // Show information but note it's compiler-generated
        add_note("(Compiler-generated symbol)");
    }
    SymbolStability::Unstable => {
        // Show best-effort information with warning
        add_warning("Definition may be incomplete or missing");
    }
}
```

### Example: Code Actions

```rust
// Only suggest code actions for user-defined symbols
if info.stability != SymbolStability::UserDefined {
    return None; // No code actions for compiler symbols
}

// Proceed with code action
// ...
```

---

## Best Practices

1. **Default to UserDefined**: Most symbols should be `UserDefined`. If you're seeing lots of `Unstable`, check span extraction.

2. **Check Before Modify**: Always check `can_rename()` or stability level before allowing rename, refactor, or code actions.

3. **Warn on Unstable**: If using `Unstable` symbols (hover only), warn users that information may be incomplete.

4. **Document CompilerGenerated**: When we add compiler-generated symbols, clearly document which transformations create them.

5. **Test Stability**: Add tests that verify:
   - User symbols are classified as `UserDefined`
   - Symbols without spans are `Unstable`
   - Future: Compiler symbols are `CompilerGenerated`

---

## Future Work

### Planned Extensions

1. **Compiler-Generated Symbol Detection**:
   - Detect closure captures
   - Identify desugared constructs
   - Track temporary variables

2. **Stability Transitions**:
   - Track when symbols become stable/unstable
   - Handle stability changes between snapshots

3. **Refinement Rules**:
   - Module-level stability (cross-file)
   - Imported symbol stability
   - Re-exported symbol stability

---

## Summary

| Stability | Safe for Rename? | Safe for References? | Safe for Code Actions? | Primary Use Case |
|-----------|------------------|----------------------|------------------------|------------------|
| UserDefined | ✅ Yes | ✅ Yes | ✅ Yes | All user-facing features |
| CompilerGenerated | ❌ No | ⚠️ Limited | ❌ No | Debugging, read-only info |
| Unstable | ❌ No | ❌ No | ❌ No | Best-effort hover only |

**Key Principle**: When in doubt, only operate on `UserDefined` symbols. This ensures reliability and prevents user confusion.

---

**Last Updated**: After implementing symbol stability classification.
