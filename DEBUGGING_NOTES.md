# Debugging Notes: Function Composition Issue

## Current Status

The implementation has been updated with:
1. ✅ Fixed `op_mul_num()` and `op_sub_num()` to force thunks
2. ✅ Added handling for nested composed thunks in `execute_prepare_call()`
3. ❌ Still encountering error: `AddNum: expected numbers but got non-number values (lhs: Value::Number(5) = 5, rhs: Value::None = None)`

## Error Analysis

The error occurs when executing `add10 |> add5 |> mul2` with `(5)!`.

**Error details**:
- `lhs: Value::Number(5) = 5` - first argument to `add` function
- `rhs: Value::None = None` - second argument to `add` function (should be a number)

**Interpretation**:
- The `add` function is being invoked with only one argument (`5`) instead of two (`10, 5`)
- This suggests that when combining arguments from `add10` (which has `[10]`) with the new argument `[5]`, the combination is not working correctly
- OR the arguments are being passed in the wrong order

## Potential Issues

### 1. Argument Combination Order

When `add10(5)` is called:
- `add10` = `thunk(add, [10])` - has existing arg `[10]`
- New arg = `[5]`
- Should combine to: `[10, 5]` (existing first, then new)
- But error suggests we're getting `[5]` instead

**Check**: Verify that `execute_prepare_call()` correctly combines existing args with new args for regular thunks (line 1231-1236).

### 2. Nested Composition Argument Handling

When `compose_fn(5)!` is called where `compose_fn = add10 |> add5 |> mul2`:
- The composed thunk structure is: `composed_thunk(
    first: composed_thunk(first: thunk(add, [10]), second: thunk(add, [5])),
    second: thunk(mul, [2])
  )`
- `execute_prepare_call(1)` should apply `[5]` to the nested composed thunk
- The fix at line 1175-1185 handles nested composition, but might have issues

**Check**: Verify that when recursively calling `execute_prepare_call()` on a nested composed thunk, the arguments are correctly propagated.

### 3. Argument Binding in `invoke_thunk_sync`

When `invoke_thunk_sync()` is called with `args=[10, 5]`:
- It should bind `args[0]=10` to `param_var_ids[0]` (first parameter)
- It should bind `args[1]=5` to `param_var_ids[1]` (second parameter)
- But error shows first parameter is `5`, not `10`

**Check**: Verify that `args` vector has the correct values when `invoke_thunk_sync()` is called.

## Debugging Steps

1. **Add debug output** to trace argument values:
   - In `execute_prepare_call()`: Log existing args and new args before combining
   - In `invoke_thunk_sync()`: Log the `args` vector before binding to locals
   - In `invoke_thunk_value_recursive()`: Log thunk structure and args

2. **Verify argument order**:
   - Check that `pop_n()` correctly reverses the order (it does at line 286)
   - Verify that args are pushed in the correct order before `execute_prepare_call()`

3. **Test with simpler composition**:
   - Test `add10 |> mul2` with `(5)!` first
   - Then test `add10 |> add5` with `(5)!`
   - Finally test the full `add10 |> add5 |> mul2`

## Next Steps

1. Add comprehensive debug logging to trace argument flow
2. Verify that `pop_n()` and argument pushing maintain correct order
3. Test each composition level separately
4. Consider adding a test case that prints intermediate values

## Files to Check

- `src/vm.rs`:
  - `execute_prepare_call()` (line 1156) - argument combination
  - `invoke_thunk_sync()` (line 1066) - argument binding
  - `invoke_thunk_value_recursive()` (line 1029) - composed thunk invocation
  - `pop_n()` (line 281) - argument popping order

