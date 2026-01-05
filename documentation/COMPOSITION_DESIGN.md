# Function Composition Design for CantaLoop

## Executive Summary

This document outlines the design for safe function composition (`|>`) with partially applied multi-argument functions in CantaLoop. The design addresses thunk forcing, composition semantics, and ensures arithmetic operations always receive fully evaluated values.

## Problem Statement

Current issues:
1. **Thunk forcing failures**: Arithmetic operations panic when receiving unforced thunks
2. **Ambiguous composition**: `add10 |> add5` is unclear when both functions are partially applied
3. **Nested thunk handling**: Composed thunks with nested partial applications need proper evaluation order

## Design Decisions

### 1. Composition Semantics

**Decision: Left-to-right function application with argument accumulation**

For `f |> g` where both are partially applied:
- `f |> g` means: "apply `f` to its arguments, then pass the result to `g`"
- When `(f |> g)(x)` is called:
  1. Apply `x` to `f` (combining with `f`'s existing partial args)
  2. If `f` is fully applied, evaluate it to get result `r`
  3. Apply `r` to `g` (combining with `g`'s existing partial args)
  4. If `g` is fully applied, evaluate it to get final result

**Example: `add10 |> add5 |> mul2`**
- `add10` = thunk(add, [10]) - needs 1 more arg
- `add5` = thunk(add, [5]) - needs 1 more arg  
- `mul2` = thunk(mul, [2]) - needs 1 more arg
- `compose_fn(5)!`:
  1. `add10(5)` → `add(10, 5)` = 15 (fully applied, evaluate)
  2. `add5(15)` → `add(5, 15)` = 20 (fully applied, evaluate)
  3. `mul2(20)` → `mul(2, 20)` = 40 (fully applied, evaluate)
  4. Result: 40

### 2. Where Composition Happens

**Decision: Hybrid approach - HIR for structure, VM for execution**

- **Semantic Analyser (HIR)**: Creates `ComposeThunk` nodes to represent composition structure
- **Bytecode Emitter**: Emits `ComposeThunk` opcode to create composed thunk at runtime
- **VM**: Handles actual composition execution and thunk forcing

**Rationale**:
- HIR captures the intent and structure
- VM handles lazy evaluation and runtime thunk forcing
- This preserves lazy evaluation semantics while allowing static analysis

### 3. Thunk Forcing Strategy

**Decision: Eager forcing at composition boundaries and arithmetic operations**

**Forcing points**:
1. **Arithmetic operations**: Always force both operands before operation
2. **Function invocation**: Force arguments when binding to parameter slots
3. **Composition evaluation**: Force intermediate results before passing to next function
4. **Return values**: Force thunks returned from functions before use

**Implementation**:
- `force_value()` already exists and handles nested thunks iteratively
- Ensure all arithmetic opcodes (`AddNum`, `MulNum`, etc.) call `force_value()`
- Ensure `invoke_thunk_value_recursive()` properly forces composed thunks

### 4. Auto-Currying

**Decision: Keep current explicit partial application, no auto-currying**

**Rationale**:
- Current design is explicit: `add(10)` clearly creates a thunk
- Auto-currying would complicate type inference and error messages
- Explicit partial application is more predictable for users
- Multi-arg functions remain multi-arg; partial application is explicit

**Future consideration**: Could add syntax sugar like `add.curry(10)` if needed, but not required for composition to work.

## Implementation Plan

### Phase 1: Fix Thunk Forcing in Arithmetic Operations

**Problem**: `AddNum`, `MulNum`, `SubNum` don't force values before operations.

**Solution**: Ensure all optimized arithmetic opcodes force values.

**Files to modify**:
- `src/core/vm.rs`: `op_mul_num()`, `op_sub_num()` - add `force_value()` calls

**Current state**:
- `op_add_num()` already forces values (line 464-465)
- `op_mul_num()` and `op_sub_num()` don't force values

### Phase 2: Fix Composed Thunk Invocation

**Problem**: When invoking composed thunks, intermediate results might not be fully forced.

**Solution**: Ensure `invoke_thunk_value_recursive()` properly forces intermediate results in composition.

**Files to modify**:
- `src/core/vm.rs`: `invoke_thunk_value_recursive()` - ensure composed thunk evaluation forces intermediate results

**Current state**:
- `invoke_thunk_value_recursive()` handles composed thunks (line 1024-1049)
- Need to verify that `first_result` is fully forced before passing to second function

### Phase 3: Fix Composed Thunk Argument Application

**Problem**: When `(f |> g)(x)` is called, arguments need to be properly applied to the first function.

**Solution**: Ensure `execute_prepare_call()` correctly handles composed thunks with new arguments.

**Files to modify**:
- `src/core/vm.rs`: `execute_prepare_call()` - verify composed thunk argument handling

**Current state**:
- `execute_prepare_call()` handles composed thunks (line 1158-1192)
- Need to verify argument combination logic is correct

### Phase 4: Add Type Checking for Composition (Optional)

**Enhancement**: Add semantic analysis to verify composition compatibility.

**Files to modify**:
- `src/core/hir_lowering/mod.rs`: Add type checking for `ComposeThunk` nodes

## Detailed Execution Flow

### Example: `compose_fn(5)!` where `compose_fn = add10 |> add5 |> mul2`

**Initial state**:
- `add10` = thunk(add, [10]) - function ID 0, needs 1 arg
- `add5` = thunk(add, [5]) - function ID 0, needs 1 arg
- `mul2` = thunk(mul, [2]) - function ID 1, needs 1 arg
- `compose_fn` = composed_thunk(
    first: composed_thunk(
        first: thunk(add, [10]),
        second: thunk(add, [5])
    ),
    second: thunk(mul, [2])
  )

**Step-by-step execution**:

1. **Bytecode execution reaches `compose_fn(5)!`**:
   - Stack: `[5, compose_fn]`
   - Execute `Thunk(1)` → combines `[5]` with `compose_fn`'s args
   - Since `compose_fn` is a composed thunk, `execute_prepare_call()` handles it:
     - Pops `compose_fn` from stack
     - Pops `[5]` from stack
     - Extracts first composed thunk: `add10 |> add5`
     - Applies `[5]` to `add10 |> add5`:
       - Extracts `add10` (thunk(add, [10]))
       - Combines `[10]` + `[5]` = `[10, 5]`
       - Creates `thunk(add, [10, 5])` - fully applied!
       - Recomposes: `composed_thunk(first: thunk(add, [10, 5]), second: thunk(add, [5]))`
     - Then applies to outer composition:
       - Creates: `composed_thunk(first: composed_thunk(...), second: thunk(mul, [2]))`
   - Stack: `[prepared_composed_thunk]`

2. **Execute `Invoke`**:
   - Pops prepared composed thunk
   - `execute_invoke()` detects `ThunkData::Composed`
   - Extracts `first` = inner composed thunk, `second` = `thunk(mul, [2])`
   - Invokes `first` (inner composed thunk):
     - Extracts `first` = `thunk(add, [10, 5])`, `second` = `thunk(add, [5])`
     - Invokes `thunk(add, [10, 5])`:
       - `invoke_thunk_sync()` called with func_id=0, args=[10, 5]
       - Forces all args (already concrete values)
       - Creates call frame for `add` function
       - Executes: `10 + 5 = 15`
       - Returns: `Value::number(15)`
     - Result: `15`
   - Applies `15` to `second` (`thunk(add, [5])`):
     - `execute_prepare_call(1)` with `[15, thunk(add, [5])]`
     - Combines: `[5]` + `[15]` = `[5, 15]`
     - Creates: `thunk(add, [5, 15])` - fully applied!
     - Invokes: `5 + 15 = 20`
     - Returns: `Value::number(20)`
   - Result: `20`
   - Applies `20` to `thunk(mul, [2])`:
     - `execute_prepare_call(1)` with `[20, thunk(mul, [2])]`
     - Combines: `[2]` + `[20]` = `[2, 20]`
     - Creates: `thunk(mul, [2, 20])` - fully applied!
     - Invokes: `2 * 20 = 40`
     - Returns: `Value::number(40)`
   - Final result: `40`

**Key invariants**:
- Each function receives exactly the number of arguments it needs
- Intermediate results are fully evaluated (not thunks) before passing to next function
- All arithmetic operations receive concrete number values

## Code Changes Required

### 1. Fix `op_mul_num()` and `op_sub_num()`

```rust
fn op_mul_num(_vm: &mut VM, _frame_idx: usize, _opcode: &OpCode) -> StepResult {
    let rhs = _vm.stack.pop().expect("Stack underflow");
    let lhs = _vm.stack.pop().expect("Stack underflow");
    // Force values in case they're thunks
    let lhs_forced = _vm.force_value(lhs);
    let rhs_forced = _vm.force_value(rhs);
    if let (Some(a), Some(b)) = (lhs_forced.as_number(), rhs_forced.as_number()) {
        _vm.stack.push(Value::number(a * b));
    } else {
        let lhs_str = lhs_forced.value_to_string(&_vm.heap);
        let rhs_str = rhs_forced.value_to_string(&_vm.heap);
        panic!("MulNum: expected numbers but got non-number values (lhs: {:?} = {}, rhs: {:?} = {})", 
            lhs_forced, lhs_str, rhs_forced, rhs_str);
    }
    StepResult::Normal
}

fn op_sub_num(_vm: &mut VM, _frame_idx: usize, _opcode: &OpCode) -> StepResult {
    let rhs = _vm.stack.pop().expect("Stack underflow");
    let lhs = _vm.stack.pop().expect("Stack underflow");
    // Force values in case they're thunks
    let lhs_forced = _vm.force_value(lhs);
    let rhs_forced = _vm.force_value(rhs);
    if let (Some(a), Some(b)) = (lhs_forced.as_number(), rhs_forced.as_number()) {
        _vm.stack.push(Value::number(a - b));
    } else {
        let lhs_str = lhs_forced.value_to_string(&_vm.heap);
        let rhs_str = rhs_forced.value_to_string(&_vm.heap);
        panic!("SubNum: expected numbers but got non-number values (lhs: {:?} = {}, rhs: {:?} = {})", 
            lhs_forced, lhs_str, rhs_forced, rhs_str);
    }
    StepResult::Normal
}
```

### 2. Verify `invoke_thunk_value_recursive()` forces intermediate results

The current implementation should already force results, but verify:
- Line 1030: `first_result` is the result of `invoke_thunk_value_recursive()`, which should return a concrete value
- Line 1034: `first_result` is pushed to stack, then passed to `execute_prepare_call()`
- `execute_prepare_call()` should handle concrete values correctly

### 3. Verify `execute_prepare_call()` handles composed thunks correctly

Current implementation (lines 1158-1192) looks correct, but verify:
- Arguments are properly combined with existing thunk args
- Recomposed thunks maintain correct structure

## Testing Strategy

### Test Cases

1. **Basic composition**: `add10 |> mul2` with `(5)!`
   - Expected: `add(10, 5) = 15`, then `mul(2, 15) = 30`

2. **Nested composition**: `add10 |> add5 |> mul2` with `(5)!`
   - Expected: `add(10, 5) = 15`, `add(5, 15) = 20`, `mul(2, 20) = 40`

3. **Composition with single-arg functions**: `square |> double` with `(5)!`
   - Expected: `square(5) = 25`, `double(25) = 50`

4. **Arithmetic in function bodies**: Functions that perform arithmetic on thunk arguments
   - Ensure arguments are forced before arithmetic operations

5. **Deep nesting**: `f1 |> f2 |> f3 |> f4` with multiple partial applications

## Summary

**Key decisions**:
1. Composition semantics: left-to-right with argument accumulation
2. Hybrid approach: HIR for structure, VM for execution
3. Eager forcing at composition boundaries and arithmetic operations
4. No auto-currying (keep explicit partial application)

**Implementation priority**:
1. Fix arithmetic opcodes to force values (critical)
2. Verify composed thunk invocation forces intermediate results
3. Test with example: `add10 |> add5 |> mul2`
4. Add comprehensive test cases

**Expected outcome**:
- `compose_fn(5)!` where `compose_fn = add10 |> add5 |> mul2` evaluates to `40`
- No panics from unforced thunks in arithmetic operations
- Predictable composition semantics for users

