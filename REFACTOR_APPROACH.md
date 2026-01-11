# CST ID Refactoring - Systematic Approach

Given the large scope (193+ Spanned::new calls), here's the systematic approach:

## Strategy

1. **Top-down signature updates**: Add `id_gen: &mut CstIdGenerator` parameters to all builder functions
2. **Bottom-up ID generation**: Generate IDs just before creating Spanned nodes
3. **Mechanical replacements**: Every `Spanned::new(span, node)` → `Spanned::new(id_gen.next(), span, node)`

## Key Functions (in dependency order)

1. ✅ `build_cst_program` - Creates id_gen, passes to blocks
2. ✅ `build_cst_block` - Accepts id_gen, passes to statements
3. ⏳ `build_cst_statement` - Needs id_gen parameter (partially done)
4. ⏳ `build_cst_expression_from_pair` - Needs id_gen parameter
5. ⏳ `build_cst_expression_from_text` - Needs id_gen parameter (complex - uses PRATT_PARSER)
6. ⏳ `build_cst_atom` - Needs id_gen parameter
7. ⏳ All helper functions - Need id_gen parameter

## Special Cases

- `build_cst_expression_from_text` uses PRATT_PARSER which calls `build_cst_atom` in a closure
  - Need to thread id_gen through the closure
  - This is more complex but doable

- Many statement builders (let, const, if, match, etc.) create expressions
  - Need to pass id_gen to all of them

## Progress Tracking

- Functions with id_gen parameter: 2 (build_cst_program, build_cst_block)
- Functions needing id_gen: ~50+
- Spanned::new calls updated: ~5
- Spanned::new calls remaining: ~188

## Next Steps

1. Fix `build_cst_statement` to generate ID before creating Spanned
2. Update `build_cst_expression_from_text` signature and thread id_gen through PRATT_PARSER
3. Update `build_cst_atom` and all atom builders
4. Systematically update all statement builders
5. Update all remaining Spanned::new calls
