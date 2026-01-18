# LSP Span and Symbol Lookup Fixes

## Critical Issues Fixed

### 1. Span Matching - Half-Open Intervals

**Problem**: The code was using inclusive end positions (`span.start <= offset && offset <= span.end`), but LSP and most text systems use half-open intervals `[start, end)` where the end is exclusive.

**Fix**: Changed all span matching to use `<` instead of `<=` for the end position:
- `src/core/lsp_api.rs`: `symbols_at_offset()` - Fixed span matching
- `src/core/lsp_api.rs`: `find_cst_nodes_at_offset()` - Fixed CST node lookup
- All other span matching code updated

**Impact**: This was causing symbols to match incorrectly, especially at boundary positions, leading to unreliable hover and wrong symbol highlighting.

### 2. Token Filtering - Too Aggressive

**Problem**: The code was filtering out all tokens less than 2 characters long, removing valid single-character identifiers, operators, and variables.

**Fix**: Changed to only filter truly invalid spans (zero-length or end < start):
```rust
// Before: len >= 2
// After: len > 0 && span.start < span.end
```

**Impact**: Single-character tokens (like `x`, `i`, operators) are now properly highlighted.

### 3. Overlap Detection and Deduplication

**Problem**: The overlap detection logic was correct but didn't handle cases where a more specific (smaller) token should replace a less specific (larger) one.

**Fix**: Improved deduplication to:
- Detect overlaps correctly using half-open interval logic
- Prefer smaller (more specific) spans when tokens overlap
- Prefer higher-priority token types when spans are equal size

**Impact**: More accurate syntax highlighting, especially for nested expressions.

### 4. Semantic Token Length Calculation

**Problem**: Multi-line token length calculation was incorrect, and the logic didn't properly handle edge cases.

**Fix**: 
- Properly calculate length for single-line tokens: `end_col - start_col`
- For multi-line tokens, only highlight the first line (LSP requirement)
- Fixed the calculation to use proper line end detection

**Impact**: Tokens spanning multiple lines are now correctly highlighted on the first line only.

### 5. Token Enhancement Logic

**Problem**: When enhancing CST tokens with semantic information, the code was removing tokens with exact span matches, but the comparison might have been too strict.

**Fix**: Improved the logic to:
- Only remove tokens that exactly match the semantic token's span
- Use precise start/end comparison instead of span equality
- Preserve non-matching tokens

**Impact**: Better integration between CST-based tokens (keywords, literals) and semantic tokens (functions, variables).

## Key Principles from rust-analyzer

1. **Half-Open Intervals**: All spans are `[start, end)` - end is exclusive
2. **Most Specific Match**: When multiple symbols match at a position, prefer the smallest span
3. **Single-Line Tokens**: LSP semantic tokens must be single-line; multi-line spans only highlight the first line
4. **Incremental Building**: Tokens are built incrementally with relative positions (deltas)

## Testing Recommendations

1. Test hover on:
   - Single-character identifiers
   - Identifiers at line boundaries
   - Nested expressions
   - Function calls

2. Test syntax highlighting on:
   - Single-character variables
   - Operators
   - Nested function calls
   - Multi-line expressions (should only highlight first line)

3. Verify:
   - No random highlighting
   - Hover shows correct symbol information
   - Go-to definition works reliably
   - References are found correctly

## Remaining Potential Issues

1. **Span Accuracy**: Verify that CST spans match the actual source text positions correctly
2. **Symbol Resolution**: Ensure all identifiers are properly resolved to symbols
3. **Cross-File Support**: Current implementation is single-file; cross-file support may need similar fixes
