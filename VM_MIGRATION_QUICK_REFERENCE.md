# VM Migration Quick Reference

## Common Conversion Patterns

### Stack Operations

| Old | New |
|-----|-----|
| `_vm.stack.push(val)` | `self.storage.push(val)?` |
| `_vm.stack.pop()` | `self.storage.pop().ok_or(VmError::StackUnderflow)?` |
| `_vm.stack.len()` | `self.storage.stack_depth()` |
| `_vm.stack.last()` | `self.storage.peek()` |

### Heap Access

| Old | New |
|-----|-----|
| `_vm.heap.arrays[idx]` | `self.storage.get_array(idx)?` |
| `_vm.heap.arrays[idx] = ...` | `self.storage.get_array_mut(idx)?` |
| `_vm.heap.strings[idx]` | `self.storage.get_string(idx)?` |
| `_vm.heap.structs[idx]` | `self.storage.get_struct(idx)?` |
| `_vm.heap.thunks[idx]` | `self.storage.get_thunk(idx)?` |
| `_vm.heap.array_iters[idx]` | `self.storage.get_array_iter_mut(idx)?` |

### Value Creation

| Old | New |
|-----|-----|
| `Value::string_with_heap(s, &mut _vm.heap)` | `Value::string_with_storage(s, &mut self.storage)?` |
| `Value::array_with_heap(elements, &mut _vm.heap)` | `Value::array_with_storage(&elements, &mut self.storage)?` |
| `Value::struct_with_heap(type_id, fields, &mut _vm.heap)` | `Value::struct_with_storage(type_id, &fields, &mut self.storage)?` |
| `Value::thunk_with_heap(func_id, bound, &mut _vm.heap)` | `Value::thunk_with_storage(ThunkData::regular(func_id, &bound)?, &mut self.storage)?` |

### Host Operations

| Old | New |
|-----|-----|
| `_vm.engine.functions.get(&func_id)` | `self.host.call_native_function(func_id, args)?` |
| `_vm.bytecode_functions.get(&func_id)` | `self.host.get_bytecode_function(func_id)?` |
| `CompileSession::get_constant_from_hir(&_vm.hir, id, &mut _vm.heap)` | `self.host.get_constant(id)?` |
| `_vm.type_registry.get(&type_id)` | `self.host.get_type_info(type_id)?` |

### Error Handling

| Old | New |
|-----|-----|
| `panic!("message")` | `return Err(VmError::InvalidOperation)` |
| `.expect("message")` | `.ok_or(VmError::StackUnderflow)?` |
| Return `StepResult` | Return `Result<StepResult, VmError>` |

## Handler Template

```rust
fn op_<name>(&mut self, frame_idx: usize, opcode: &OpCode) -> Result<StepResult, VmError> {
    if let OpCode::<Variant>(data) = opcode {
        // 1. Pop operands from stack
        let b = self.storage.pop().ok_or(VmError::StackUnderflow)?;
        let a = self.storage.pop().ok_or(VmError::StackUnderflow)?;
        
        // 2. Force values if needed (for thunks)
        let a_forced = self.force_value(a)?;
        let b_forced = self.force_value(b)?;
        
        // 3. Perform operation
        let result = /* operation */;
        
        // 4. Push result
        self.storage.push(result)?;
    }
    Ok(StepResult::Normal)
}
```

## Handler Checklist

For each handler, verify:
- [ ] Uses `self.storage` instead of `_vm.stack`/`_vm.heap`
- [ ] Uses `self.host` for platform operations
- [ ] Returns `Result<StepResult, VmError>`
- [ ] Uses `?` for error propagation
- [ ] Handles `StackUnderflow` errors
- [ ] Forces thunks if needed
- [ ] Compiles without errors

## Opcode Handler Order

Move handlers in this order (easiest to hardest):

1. **Simple loads** (LdNum, LdBool, LdFunc) - No dependencies
2. **String/Constant loads** (LdStr, LdConst) - Need host
3. **Arithmetic** (Add, Sub, Mul, Div) - Need force_value
4. **Comparisons** (Eq, Ne, Gt, Lt) - Need force_value
5. **Logic** (And, Or, Not) - Need force_value
6. **Arrays** (MakeArray, ArrayIter, etc.) - Need storage methods
7. **Structs** (MakeStruct, GetField) - Need storage methods
8. **Variables** (LdVar, StVar) - Need frame access
9. **Functions** (Thunk, Invoke, Ret) - Need host + frames
10. **Control flow** (Jmp, JmpIfFalse) - Need IP manipulation
11. **Functional** (Map, Filter, Fold) - Complex, depends on others

## Testing After Each Handler

```rust
// Quick test pattern
#[test]
fn test_op_<name>() -> Result<(), VmError> {
    let storage = DynamicStorage::new();
    let host = NullHost;
    let bytecode: &'static [OpCode] = Box::leak(Box::new([
        OpCode::LdNum(5.0),
        OpCode::LdNum(3.0),
        OpCode::<Name>, // Your handler
    ]));
    
    let mut vm = VM::new(storage, host, bytecode);
    vm.run()?;
    
    let result = vm.storage.pop().ok_or(VmError::StackUnderflow)?;
    // Assert result
    Ok(())
}
```

## Common Issues and Fixes

### Issue: "cannot borrow `self` as mutable"

**Cause**: Trying to borrow storage and host simultaneously

**Fix**: 
```rust
// Bad
let array = self.storage.get_array(idx)?;
self.storage.push(val)?; // Error: storage already borrowed

// Good
let array_idx = /* get index */;
// ... do other operations ...
let array = self.storage.get_array(array_idx)?; // Borrow after other ops
```

### Issue: "expected `&mut VmStorage`, found `&mut DynamicStorage`"

**Cause**: Type mismatch

**Fix**: Ensure you're using trait methods, not concrete type methods

### Issue: "function cannot return value referencing local variable"

**Cause**: Returning reference to temporary

**Fix**: Return owned value or clone:
```rust
// Bad
fn get_something(&self) -> &str {
    self.storage.get_string(0)? // Returns &str from temporary
}

// Good
fn get_something(&self) -> String {
    self.storage.get_string(0)?.to_string()
}
```

## Progress Tracking Template

```markdown
## Handler Migration Status

### Phase 2.1: Simple Loads
- [x] op_ld_num
- [x] op_ld_bool
- [ ] op_ld_func

### Phase 2.2: String/Constants
- [ ] op_ld_str
- [ ] op_ld_const

### Phase 2.3: Arithmetic
- [ ] op_add
- [ ] op_sub
- [ ] op_mul
- [ ] op_div
- [ ] op_mod
- [ ] op_pow

### Phase 2.4: Comparisons
- [ ] op_eq
- [ ] op_ne
- [ ] op_gt
- [ ] op_lt
- [ ] op_ge
- [ ] op_le

### Phase 2.5: Logic
- [ ] op_and
- [ ] op_or
- [ ] op_not
- [ ] op_neg

### Phase 2.6: Arrays
- [ ] op_make_array
- [ ] op_array_iter
- [ ] op_array_next
- [ ] op_index
- [ ] op_array_slice

### Phase 2.7: Structs
- [ ] op_make_struct
- [ ] op_get_field

### Phase 2.8: Variables
- [ ] op_ld_var
- [ ] op_st_var

### Phase 2.9: Functions
- [ ] op_thunk
- [ ] op_invoke
- [ ] op_ret
- [ ] op_call_stack
- [ ] op_make_partial
- [ ] op_compose_thunk
- [ ] op_ret_invoke

### Phase 2.10: Control Flow
- [ ] op_jmp
- [ ] op_jmp_if_false
- [ ] op_jmp_if_true

### Phase 2.11: Functional
- [ ] op_map
- [ ] op_filter
- [ ] op_fold

### Phase 2.12: Other
- [ ] op_pop
- [ ] op_print
- [ ] op_add_num
- [ ] op_mul_num
- [ ] op_sub_num
```

## File Locations Reference

| What | Old Location | New Location |
|------|-------------|--------------|
| VM struct | `src/core/vm.rs:559` | `cantaloop-core/src/vm.rs` |
| StepResult | `src/core/vm.rs:28` | `cantaloop-core/src/vm.rs` |
| CallFrame | `src/core/vm.rs:548` | `cantaloop-core/src/vm.rs` |
| Opcode handlers | `src/core/vm.rs:710+` | `cantaloop-core/src/vm.rs` |
| Execution loop | `src/core/vm.rs:634` | `cantaloop-core/src/vm.rs` |
| Value | `src/core/vm.rs:91` | `cantaloop-core/src/value.rs` |
| Host trait | N/A (new) | `cantaloop-core/src/host.rs` |

## Compilation Commands

```bash
# Check core crate
cargo check --package cantaloop-core

# Check main crate
cargo check

# Run tests
cargo test

# Run example
cargo run --example embedded_vm --package cantaloop-core
```

## Git Workflow

```bash
# After each working batch of handlers
git add cantaloop-core/src/vm.rs
git commit -m "Migrate opcode handlers: op_add, op_sub, op_mul"

# If something breaks
git stash
# Fix issue
git stash pop
```

---

**Remember**: Work incrementally, test frequently, commit often! 🚀

