# CST ID Refactoring - Complexity Note

## Current Status

This refactoring is large but correct:
- **193+ Spanned::new calls** need IDs
- **50+ builder functions** need id_gen parameter  
- **PRATT_PARSER closures** need id_gen captured (complex)

## Progress

- ✅ CstId type created
- ✅ Spanned<T> updated to include id field
- ✅ build_cst_program, build_cst_block updated
- ✅ build_cst_statement signature updated + ID generation
- ✅ build_cst_expression_from_pair signature updated
- ⏳ build_cst_expression_from_text needs id_gen threaded through PRATT_PARSER
- ⏳ build_cst_atom and all atom/primary builders need id_gen
- ⏳ All statement builders need id_gen parameter
- ⏳ All 193+ Spanned::new calls need ID parameter

## Complexity

The PRATT_PARSER usage is particularly complex because:
- It uses closures (map_primary, map_prefix, map_infix, map_postfix)
- These closures create Spanned nodes
- id_gen must be captured in closures (requires move or shared reference)
- Rust's closure capture rules make this tricky

## Recommendation

This is a **correct architectural change** but is **very large** (193+ locations).
Consider:
1. Complete systematically (mechanical but tedious)
2. Or do incrementally with compilation checkpoints
3. Or use a more automated approach if possible

The pattern is established - the rest is mechanical application.
