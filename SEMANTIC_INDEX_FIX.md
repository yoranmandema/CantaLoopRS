# Semantic Index Fix: CST → HIR Symbol Mapping

## Problem

Current semantic index building uses name-based matching:
```rust
if cst_identifier.text == hir_symbol.name { ... }
```

This fails because:
- Multiple symbols can have the same name (shadowing, closures, modules)
- Name lookup cannot disambiguate: `add` as function vs `add` as variable vs `add` as parameter
- Coverage is ~40% (only definitions match correctly)

## Solution: Option B - CST → HIR Binding Table

During lowering (AST → HIR), build a mapping table:
```
Span → SymbolId
```

### Key Insight

When lowering creates a `HirExpression::Identifier(var_id)` or `HirExpression::FunctionCall { function_id }`,
we know:
1. The AST node (which has a span)
2. The HIR symbol ID (var_id or function_id)

We must record: `(span, symbol_id)` during lowering.

### Implementation Plan

#### Phase 1: Add span tracking to lowering

1. Add a `SymbolOccurrence` struct to track spans during lowering:
   ```rust
   struct SymbolOccurrence {
       symbol_id: SymbolId,
       span: HirSpan,
       role: SymbolRole, // Definition, Read, Call, Type, etc.
   }
   ```

2. Add `symbol_occurrences: Vec<SymbolOccurrence>` field to `HirBuilder` (or similar)

3. During lowering, when creating:
   - `HirExpression::Identifier(var_id)` → record `(var_id, span, Read)`
   - `HirExpression::FunctionCall { function_id }` → record `(function_id, span, Call)`
   - `let x = ...` → record `(x_id, span, Definition)` (already tracked)

#### Phase 2: Build semantic index from occurrences

1. In `build_semantic_index`, iterate over `symbol_occurrences` instead of name matching
2. Build `span_to_symbol` directly from occurrences
3. Remove name-based matching logic

#### Phase 3: Handle edge cases

- Member access: `obj.field` → track `field` span
- Pipeline stages: `x |> f` → track `f` call span
- Partial application: `add(?, 5)` → track `add` call span
- Struct types: `State { ... }` → track `State` type span

## Current Status

- ✅ Problem diagnosed correctly
- ✅ Architecture identified (Option B)
- ✅ SymbolOccurrence types created
- ⏳ Implementation pending (requires AST→HIR lowering changes)

## Implementation Complexity

The implementation is more complex than initially anticipated because:

1. **AST nodes don't have spans**: The AST layer (between CST and HIR) doesn't preserve spans
2. **Lowering pipeline**: CST → AST → HIR means we need to track spans across two transformations
3. **Large codebase**: `HirBuilder` is 4000+ lines and lowering logic is complex

## Recommended Approach

This fix requires:
1. Either: Add span information to AST nodes (changes AST structure)
2. Or: Build a side table during CST→AST and AST→HIR transformations
3. Or: Pass span information through the lowering process explicitly

This is a **significant architectural refactor** that should be done carefully with:
- Comprehensive testing
- Incremental implementation
- Clear separation of concerns

## Interim Solution

For now, we have:
- ✅ Safety checks that warn when coverage < 80%
- ✅ Detailed diagnostics showing which identifiers don't match
- ✅ Clear quantification of the problem (27/66 = 40.9% coverage)

This allows the LSP to work (with reduced highlighting) while the proper fix is implemented.

## Notes

- Option A (spans in HIR nodes) would require changing `HirExpression` enum, which affects bytecode compilation
- Option B (side table) is less invasive but requires careful integration
- The mapping table should be part of `HirAst` or `CompilerSnapshot`
