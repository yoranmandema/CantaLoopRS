# Pratt Parser Refactoring Status

## Goal
Replace closure-based `pest::pratt_parser::PrattParser` with a custom function-based parser that threads `&mut CstIdGenerator` through, ensuring all CST nodes get IDs from the same ID space.

## Completed ✅
1. Created `src/core/cst/pratt.rs` with custom Pratt parser implementation
2. Made `build_cst_atom` public so it can be used by pratt module
3. Updated `build_cst_expression_from_text` to use `parse_expression` instead of `PRATT_PARSER`
4. Threaded `&mut CstIdGenerator` through parser functions
5. Implemented precedence climbing algorithm structure

## In Progress ⏳
- Algorithm refinement to match pest's behavior exactly
- Fixing compilation errors (113 remaining)
- Adding `id_gen` parameters to functions that create CST nodes
- Updating all `Spanned::new()` calls to include IDs (69 locations)

## Remaining Work
This is a large cascading refactor. The critical path:
1. `build_cst_primary` - needs `id_gen` parameter (called by `build_cst_atom`)
2. Functions called by `build_cst_primary` that create `Spanned` nodes
3. Statement builders that call `build_cst_expression_from_text`
4. All other functions in the cascade

## Notes
- The architecture is correct - function-based parser with `id_gen` threading
- The algorithm may need refinement based on how pest's expression rule structure works
- This is a large refactor (3368 lines in builder.rs, 69 Spanned::new calls, 113 errors)
- Progress is systematic but will take many iterations
