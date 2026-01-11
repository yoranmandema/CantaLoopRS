# CST ID Refactoring Status

## Current State

This is a **large but correct architectural refactoring**. We're adding `CstId` to all CST nodes to enable tracking nodes through lowering for accurate symbol binding.

## Progress

### ✅ Completed
- `CstId` type and `CstIdGenerator` created
- `Spanned<T>` updated to include `id: CstId` field
- `build_cst_program` - Creates id_gen, passes to blocks
- `build_cst_block` - Accepts id_gen, generates IDs for blocks
- `build_cst_statement` - Signature updated, generates ID before creating Spanned
- `build_cst_expression_from_pair` - Signature updated
- `build_cst_expression_from_text` - Signature updated, PRATT_PARSER work in progress
- `build_cst_atom` - Signature updated

### ⏳ In Progress
- PRATT_PARSER closures - Complex because need mutable access to id_gen in closures
  - Using temporary local generator for now (temporary solution)
  - Proper fix requires restructuring or API changes

### ⏳ Remaining
- Update all statement builders to accept id_gen parameter (~15 functions)
- Update all expression builders (build_cst_primary, build_cst_value, etc.)
- Update all 193+ `Spanned::new()` calls to include `id` parameter
- Fix PRATT_PARSER id_gen threading (requires API/structuring changes)

## Complexity Notes

1. **PRATT_PARSER closures**: Can't easily capture `&mut id_gen` in closures. Current workaround uses local generator. Proper fix would require restructuring.

2. **Scale**: 193+ `Spanned::new` calls need updating. This is mechanical but extensive.

3. **Function signatures**: ~50+ builder functions need `id_gen` parameter added.

## Strategy

1. Continue systematically updating function signatures
2. Use Python to help identify patterns and generate fixes
3. Apply fixes carefully with compilation checks
4. Note PRATT_PARSER limitation for proper fix later

## Next Steps

1. Fix remaining compilation errors in PRATT_PARSER section
2. Update statement builders that call build_cst_expression_from_text
3. Update all expression builders
4. Systematically update all Spanned::new calls
5. Test compilation after each major section
