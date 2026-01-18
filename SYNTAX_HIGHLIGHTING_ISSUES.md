# Syntax Highlighting Issues - Diagnosis

## Summary
Current syntax highlighting coverage: **60%** (expected: 80%+)

## Test Results

### 1. Use Statement Highlighting ❌
**Status**: BROKEN (0% coverage for use statements)

**Expected**: 5 identifiers (`print`, `std`, `map`, `functional`, `test`)
**Actual**: Only 1 identifier highlighted (`test` function)

**Missing**:
- `print` (imported function name)
- `std` (module name)
- `map` (imported function name)
- `functional` (module name)

**Root Cause**: Use statement identifiers are not recorded in SymbolResolver during HIR lowering.

---

### 2. Function Body Highlighting ❌
**Status**: BROKEN (27% coverage for function bodies)

**Expected**: 15 identifiers
**Actual**: Only 4 highlighted correctly

**Issues Found**:

#### A. Parameter Name Swap Bug 🐛
Parameters `a` and `b` are recorded with **swapped names**:
- Position 8..9 should be `a` but shows as `b`
- Position 16..17 should be `b` but shows as `a`

#### B. Local Variables Not Recorded ❌
Variables declared inside function bodies are NOT in symbol table:
- `sum` (in `add` function)
- `x`, `y`, `result` (in `main` function)

#### C. References Not Recorded ❌
Variable/function references inside expressions are NOT in symbol table:
- `a + b` - neither `a` nor `b` highlighted
- `add(x, y)` - neither `add`, `x`, nor `y` highlighted
- `return sum` - `sum` not highlighted

---

### 3. Expression Highlighting ❌
**Status**: BROKEN

All variable references in expressions are missing from symbol table:
- Binary operations: `a + b`, `a * 2`, `b - x`, `c / y`
- Function calls: `add(x, y)`
- Return statements: `return sum`

---

## Root Causes

### 1. Use Statement Processing
**File**: `src/core/hir_lowering/lower_stmt.rs`

Use statements are processed but symbols are NOT recorded:
- Module names not recorded (`std`, `functional`)
- Imported function names not recorded (`print`, `map`)

**Fix Needed**: Call `symbol_resolver.define()` when processing use statements.

---

### 2. Function Body Symbol Recording
**File**: `src/core/hir_lowering/lower_stmt.rs`

Local variables inside function bodies are NOT recorded in SymbolResolver:
- `init_var()` is called but without CST ID
- Should use `init_var_with_cst_id()` with proper CST ID

**Fix Needed**:
1. Ensure all `init_var()` calls pass CST ID
2. Record variable definitions when processing Let statements inside functions

---

### 3. Expression Reference Recording
**File**: `src/core/hir_lowering/lower_stmt.rs` (process_expression)

Variable references in expressions are NOT calling `record_symbol_reference`:
- Binary expressions process LHS and RHS but don't record references
- Function call arguments are not recorded
- Return expression identifiers are not recorded

**Fix Needed**: Ensure `record_symbol_reference()` is called for ALL identifier resolutions.

---

### 4. Parameter Name Bug 🐛
**File**: `src/core/hir_lowering/lower_stmt.rs` (around line 2394-2401)

Parameters are being recorded with the wrong entity IDs or in the wrong order.

Current code:
```rust
for arg in &arguments {
    let param_kind = self.parse_type_string(&arg.kind);
    let var_id = self.init_var_with_cst_id(&arg.identifier.name, param_kind, Some(arg.identifier.cst_id));
    param_var_ids.push(var_id);
}
```

**Investigation Needed**: Check if `init_var_with_cst_id` is recording the correct name-to-entity mapping.

---

## Priority Fixes

### High Priority (Breaks Basic Highlighting)
1. ✅ Function parameters recorded (but with wrong names - needs fix)
2. ❌ **Local variables not recorded** - CRITICAL
3. ❌ **Expression references not recorded** - CRITICAL

### Medium Priority (Breaks Advanced Features)
4. ❌ Use statement imports not recorded
5. 🐛 Parameter name swap bug

### Low Priority (Edge Cases)
6. Module names in use statements

---

## Expected vs Actual Symbol Coverage

### Test: `test_use_statement_highlighting`
- **Expected**: 5 symbols (print, std, map, functional, test)
- **Actual**: 1 symbol (test)
- **Coverage**: 20%

### Test: `test_function_body_highlighting`
- **Expected**: 15 symbols
- **Actual**: 4 symbols (with 2 having wrong names)
- **Coverage**: 27% (worse if you count name bugs)

### Test: `test_coverage_percentage`
- **Expected**: ≥80% coverage
- **Actual**: 60% coverage
- **Gap**: 20 percentage points

---

## Verification Commands

Run these tests to verify fixes:
```bash
# All highlighting tests
cargo test --test test_syntax_highlighting -- --nocapture

# Specific tests
cargo test --test test_syntax_highlighting test_use_statement_highlighting -- --nocapture
cargo test --test test_syntax_highlighting test_function_body_highlighting -- --nocapture
cargo test --test test_syntax_highlighting test_coverage_percentage -- --nocapture
```

---

## Success Criteria

✅ **Fixed when**:
1. Use statement identifiers appear in symbol table
2. Local variables inside functions appear in symbol table
3. All variable/function references in expressions appear in symbol table
4. Parameter names match correctly (no swap bug)
5. Overall coverage ≥ 80%
6. All 5 tests in `test_syntax_highlighting` pass
