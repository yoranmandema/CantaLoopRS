# Detailed VM Migration Guide

## Overview

This guide will help you move all VM execution logic from `src/core/vm.rs` to `cantaloop-core/src/vm.rs`, ensuring there is exactly one canonical VM implementation.

**Goal**: `cantaloop-core/src/vm.rs` becomes THE VM. Desktop VM becomes a thin wrapper.

## Prerequisites

- ✅ Host trait created (`cantaloop-core/src/host.rs`)
- ✅ VM structure updated to be generic over `Storage` and `Host`
- ✅ Core crate compiles

## Migration Strategy

We'll migrate in phases to keep things working:

1. **Phase 1**: Move execution loop and frame management
2. **Phase 2**: Move opcode handlers (in batches)
3. **Phase 3**: Move helper methods
4. **Phase 4**: Create DesktopHost implementation
5. **Phase 5**: Make desktop VM a thin wrapper
6. **Phase 6**: Delete old VM

---

## Phase 1: Execution Loop and Frame Management

### Step 1.1: Move CallFrame and StepResult

**Source**: `src/core/vm.rs` lines 548-553, 27-31

**Action**: These are already in `cantaloop-core/src/vm.rs`, but verify they match:

```rust
// In cantaloop-core/src/vm.rs - should already exist
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepResult {
    Normal,   // Normal execution, IP was incremented
    Continue, // Special case (e.g., Ret), needs to restart loop
    Halted,   // VM has halted
}

#[cfg(feature = "std")]
pub struct CallFrame {
    pub code: &'static [OpCode],
    pub ip: usize,
    pub locals: std::vec::Vec<Value>,
    pub stack_depth: usize,
}
```

### Step 1.2: Move execute_until_empty

**Source**: `src/core/vm.rs` lines 631-705

**Action**: Copy to `cantaloop-core/src/vm.rs`, adapt to use `self.storage` and `self.host`:

```rust
// In cantaloop-core/src/vm.rs
impl<S: VmStorage, H: Host> VM<S, H> {
    /// Execute frames until the call stack is empty or until a specific frame count is reached.
    fn execute_until_empty(&mut self, target_frame_count: Option<usize>) {
        while !self.call_stack.is_empty() {
            // Check if we've reached the target frame count
            if let Some(target) = target_frame_count {
                if self.call_stack.len() <= target {
                    return;
                }
            }

            let frame_idx = self.call_stack.len() - 1;

            // Check if frame is finished
            let frame_finished = {
                let frame = &self.call_stack[frame_idx];
                frame.ip >= frame.code.len()
            };

            if frame_finished {
                // Function reached end without explicit return - handle implicit return
                self.execute_return();
                continue;
            }

            // Execute one step
            match self.step(frame_idx) {
                StepResult::Normal => {} // Normal execution, IP already incremented
                StepResult::Continue => {
                    // Special case (e.g., Ret), needs to restart loop
                    continue;
                }
                StepResult::Halted => {
                    break;
                }
            }
        }
    }
}
```

**Key Changes**:
- Remove references to `self.stack` → use `self.storage.push/pop`
- Remove references to `self.heap` → use `self.storage` methods
- Remove references to `self.engine` → use `self.host.call_native_function`
- Remove references to `self.bytecode_functions` → use `self.host.get_bytecode_function`
- Remove references to `self.hir` → use `self.host.get_constant`

### Step 1.3: Move execute_return

**Source**: `src/core/vm.rs` lines 656-692

**Action**: Copy and adapt:

```rust
// In cantaloop-core/src/vm.rs
impl<S: VmStorage, H: Host> VM<S, H> {
    fn execute_return(&mut self) {
        if self.call_stack.is_empty() {
            return;
        }

        let frame = self.call_stack.pop().unwrap();
        
        // Restore stack to depth before this frame
        let target_depth = frame.stack_depth;
        let current_depth = self.storage.stack_depth();
        
        // Pop excess values (keeping return value if any)
        if current_depth > target_depth {
            // Keep the top value (return value) if stack grew
            let return_value = self.storage.pop();
            
            // Pop any remaining values
            while self.storage.stack_depth() > target_depth {
                self.storage.pop();
            }
            
            // Push return value back
            if let Some(val) = return_value {
                self.storage.push(val).ok();
            }
        }
    }
}
```

**Checkpoint**: Compile `cantaloop-core` to verify structure is correct.

---

## Phase 2: Move Opcode Handlers

We'll move handlers in logical groups. Each handler needs these changes:

### Pattern for Converting Handlers

**Old Pattern**:
```rust
fn op_ld_num(_vm: &mut VM, _frame_idx: usize, opcode: &OpCode) -> StepResult {
    if let OpCode::LdNum(n) = opcode {
        _vm.stack.push(Value::number(*n));
    }
    StepResult::Normal
}
```

**New Pattern**:
```rust
// In cantaloop-core/src/vm.rs
impl<S: VmStorage, H: Host> VM<S, H> {
    fn op_ld_num(&mut self, _frame_idx: usize, opcode: &OpCode) -> Result<StepResult, VmError> {
        if let OpCode::LdNum(n) = opcode {
            self.storage.push(Value::number(*n))?;
        }
        Ok(StepResult::Normal)
    }
}
```

**Key Changes**:
- `_vm.stack` → `self.storage`
- `_vm.heap` → `self.storage` (use storage methods)
- `_vm.engine` → `self.host`
- Return `Result<StepResult, VmError>` instead of `StepResult`
- Use `?` for error propagation

### Step 2.1: Move Simple Load Handlers

**Handlers to move**:
- `op_ld_num` (line 710)
- `op_ld_bool` (line 723)
- `op_ld_func` (line 787)

**Example - op_ld_num**:
```rust
// In cantaloop-core/src/vm.rs
fn op_ld_num(&mut self, _frame_idx: usize, opcode: &OpCode) -> Result<StepResult, VmError> {
    if let OpCode::LdNum(n) = opcode {
        self.storage.push(Value::number(*n))?;
    }
    Ok(StepResult::Normal)
}
```

**Example - op_ld_func**:
```rust
fn op_ld_func(&mut self, _frame_idx: usize, opcode: &OpCode) -> Result<StepResult, VmError> {
    if let OpCode::LdFunc(id) = opcode {
        // Functions are just IDs, no host call needed
        self.storage.push(Value::function(*id))?;
    }
    Ok(StepResult::Normal)
}
```

### Step 2.2: Move String Handler

**Source**: `src/core/vm.rs` line 718

**Action**: Adapt to use storage:

```rust
fn op_ld_str(&mut self, _frame_idx: usize, opcode: &OpCode) -> Result<StepResult, VmError> {
    #[cfg(feature = "std")]
    {
        if let OpCode::LdStr(s) = opcode {
            let val = Value::string_with_storage(s.clone(), &mut self.storage)?;
            self.storage.push(val)?;
        }
    }
    #[cfg(not(feature = "std"))]
    {
        // For no_std, strings might use indices
        return Err(VmError::InvalidOperation);
    }
    Ok(StepResult::Normal)
}
```

### Step 2.3: Move Constant Handler

**Source**: `src/core/vm.rs` line 781

**Action**: Use host:

```rust
fn op_ld_const(&mut self, _frame_idx: usize, opcode: &OpCode) -> Result<StepResult, VmError> {
    if let OpCode::LdConst(id) = opcode {
        let const_val = self.host.get_constant(*id)
            .ok_or(VmError::InvalidOperation)?;
        self.storage.push(const_val)?;
    }
    Ok(StepResult::Normal)
}
```

### Step 2.4: Move Variable Handler

**Source**: `src/core/vm.rs` line 735

**Action**: Adapt frame access:

```rust
fn op_ld_var(&mut self, frame_idx: usize, opcode: &OpCode) -> Result<StepResult, VmError> {
    if let OpCode::LdVar(id) = opcode {
        let idx = *id as usize;
        
        // Walk the call stack backwards to find the variable
        let mut val = Value::none();
        let mut found = false;
        
        // Check frames from current to oldest
        for i in (0..=frame_idx).rev() {
            if i >= self.call_stack.len() {
                continue;
            }
            let frame = &self.call_stack[i];
            if idx < frame.locals.len() {
                let frame_val = frame.locals[idx];
                if !frame_val.is_none() {
                    val = frame_val;
                    found = true;
                    break;
                } else if i == frame_idx {
                    found = true;
                }
            }
        }
        
        if !found && frame_idx < self.call_stack.len() {
            let frame = &self.call_stack[frame_idx];
            if idx < frame.locals.len() {
                val = frame.locals[idx];
            }
        }
        
        self.storage.push(val)?;
    }
    Ok(StepResult::Normal)
}
```

### Step 2.5: Move Arithmetic Handlers

**Handlers**: `op_add`, `op_sub`, `op_mul`, `op_div`, `op_mod`, `op_pow`

**Source**: `src/core/vm.rs` lines 800-880

**Pattern**:
```rust
fn op_add(&mut self, _frame_idx: usize, _opcode: &OpCode) -> Result<StepResult, VmError> {
    let b = self.storage.pop().ok_or(VmError::StackUnderflow)?;
    let a = self.storage.pop().ok_or(VmError::StackUnderflow)?;
    
    // Force values in case they're thunks
    let a_forced = self.force_value(a)?;
    let b_forced = self.force_value(b)?;
    
    if let (Some(na), Some(nb)) = (a_forced.as_number(), b_forced.as_number()) {
        self.storage.push(Value::number(na + nb))?;
    } else {
        return Err(VmError::InvalidOperation);
    }
    
    Ok(StepResult::Normal)
}
```

**Note**: You'll need to implement `force_value` helper (see Phase 3).

### Step 2.6: Move Comparison Handlers

**Handlers**: `op_eq`, `op_ne`, `op_gt`, `op_lt`, `op_ge`, `op_le`

**Source**: `src/core/vm.rs` lines 886-916

**Pattern**:
```rust
fn op_eq(&mut self, _frame_idx: usize, _opcode: &OpCode) -> Result<StepResult, VmError> {
    let b = self.storage.pop().ok_or(VmError::StackUnderflow)?;
    let a = self.storage.pop().ok_or(VmError::StackUnderflow)?;
    
    let a_forced = self.force_value(a)?;
    let b_forced = self.force_value(b)?;
    
    let eq = match (a_forced.as_number(), b_forced.as_number()) {
        (Some(na), Some(nb)) => na == nb,
        (Some(_), None) | (None, Some(_)) => false,
        (None, None) => {
            // Compare booleans, functions, or use raw comparison
            a_forced.as_boolean() == b_forced.as_boolean() ||
            a_forced.as_function() == b_forced.as_function()
        }
    };
    
    self.storage.push(Value::boolean(eq))?;
    Ok(StepResult::Normal)
}
```

### Step 2.7: Move Logic Handlers

**Handlers**: `op_and`, `op_or`, `op_not`

**Source**: `src/core/vm.rs` lines 922-960

### Step 2.8: Move Array Handlers

**Handlers**: `op_make_array`, `op_array_iter`, `op_index`, `op_array_slice`, `op_array_next`

**Source**: `src/core/vm.rs` lines 1121-1301

**Key Changes**:
- `Value::array_with_heap` → `Value::array_with_storage`
- `heap.arrays` → `storage.get_array/get_array_mut`
- `heap.array_iters` → `storage.get_array_iter_mut`

### Step 2.9: Move Struct Handlers

**Handlers**: `op_make_struct`, `op_get_field`

**Source**: `src/core/vm.rs` lines 1326-1439

### Step 2.10: Move Function Call Handlers

**Handlers**: `op_thunk`, `op_invoke`, `op_ret`, `op_call_stack`, `op_make_partial`, `op_compose_thunk`, `op_ret_invoke`

**Source**: `src/core/vm.rs` lines 1000-1119

**Key Changes**:
- `call_function` → use `self.host.call_native_function` or `self.host.get_bytecode_function`
- Frame creation uses `self.host.get_bytecode_function`

### Step 2.11: Move Control Flow Handlers

**Handlers**: `op_jmp`, `op_jmp_if_false`, `op_jmp_if_true`

**Source**: `src/core/vm.rs` lines 1445-1500 (approximately)

### Step 2.12: Move Functional Handlers

**Handlers**: `op_map`, `op_filter`, `op_fold`

**Source**: `src/core/vm.rs` lines 1339-1439

### Step 2.13: Update step() Method

**Source**: `src/core/vm.rs` line 608

**Action**: Create dispatch table and step method:

```rust
// In cantaloop-core/src/vm.rs
type OpHandler<S, H> = fn(&mut VM<S, H>, usize, &OpCode) -> Result<StepResult, VmError>;

// Dispatch table - indices must match opcode discriminant values
const OPCODE_COUNT: usize = 48;

impl<S: VmStorage, H: Host> VM<S, H> {
    fn step(&mut self, frame_idx: usize) -> Result<StepResult, VmError> {
        let frame = &self.call_stack[frame_idx];
        let ip = frame.ip;
        
        if ip >= frame.code.len() {
            return Ok(StepResult::Halted);
        }
        
        let opcode = &frame.code[ip];
        let disc = opcode.discriminant() as usize;
        
        // Increment IP
        self.call_stack[frame_idx].ip += 1;
        
        // Dispatch to handler
        match disc {
            0 => self.op_ld_num(frame_idx, opcode),
            1 => self.op_ld_str(frame_idx, opcode),
            2 => self.op_ld_bool(frame_idx, opcode),
            3 => self.op_ld_var(frame_idx, opcode),
            4 => self.op_ld_const(frame_idx, opcode),
            5 => self.op_ld_func(frame_idx, opcode),
            6 => self.op_add(frame_idx, opcode),
            7 => self.op_sub(frame_idx, opcode),
            8 => self.op_mul(frame_idx, opcode),
            9 => self.op_div(frame_idx, opcode),
            10 => self.op_mod(frame_idx, opcode),
            11 => self.op_pow(frame_idx, opcode),
            12 => self.op_add_num(frame_idx, opcode),
            13 => self.op_mul_num(frame_idx, opcode),
            14 => self.op_sub_num(frame_idx, opcode),
            15 => self.op_eq(frame_idx, opcode),
            16 => self.op_ne(frame_idx, opcode),
            17 => self.op_gt(frame_idx, opcode),
            18 => self.op_lt(frame_idx, opcode),
            19 => self.op_ge(frame_idx, opcode),
            20 => self.op_le(frame_idx, opcode),
            21 => self.op_and(frame_idx, opcode),
            22 => self.op_or(frame_idx, opcode),
            23 => self.op_neg(frame_idx, opcode),
            24 => self.op_not(frame_idx, opcode),
            25 => self.op_st_var(frame_idx, opcode),
            26 => self.op_pop(frame_idx, opcode),
            27 => self.op_print(frame_idx, opcode),
            28 => self.op_call_stack(frame_idx, opcode),
            29 => self.op_thunk(frame_idx, opcode),
            30 => self.op_make_partial(frame_idx, opcode),
            31 => self.op_compose_thunk(frame_idx, opcode),
            32 => self.op_invoke(frame_idx, opcode),
            33 => self.op_ret(frame_idx, opcode),
            34 => self.op_ret_invoke(frame_idx, opcode),
            35 => self.op_jmp_if_false(frame_idx, opcode),
            36 => self.op_jmp_if_true(frame_idx, opcode),
            37 => self.op_jmp(frame_idx, opcode),
            38 => self.op_make_array(frame_idx, opcode),
            39 => self.op_array_iter(frame_idx, opcode),
            40 => self.op_array_next(frame_idx, opcode),
            41 => self.op_index(frame_idx, opcode),
            42 => self.op_array_slice(frame_idx, opcode),
            43 => self.op_make_struct(frame_idx, opcode),
            44 => self.op_get_field(frame_idx, opcode),
            45 => self.op_map(frame_idx, opcode),
            46 => self.op_filter(frame_idx, opcode),
            47 => self.op_fold(frame_idx, opcode),
            _ => Err(VmError::InvalidOperation),
        }
    }
}
```

**Checkpoint**: After each batch of handlers, compile and fix errors.

---

## Phase 3: Move Helper Methods

### Step 3.1: Move force_value

**Source**: `src/core/vm.rs` line 1441 (approximately)

**Action**: Adapt to use storage:

```rust
impl<S: VmStorage, H: Host> VM<S, H> {
    /// Force evaluation of a value (handle thunks)
    fn force_value(&mut self, val: Value) -> Result<Value, VmError> {
        if val.is_thunk() {
            // Invoke thunk recursively
            self.invoke_thunk_value_recursive(val)
        } else {
            Ok(val)
        }
    }
    
    fn invoke_thunk_value_recursive(&mut self, thunk: Value) -> Result<Value, VmError> {
        // Implementation from old VM, adapted to use storage and host
        // ...
    }
}
```

### Step 3.2: Move binary operations

**Source**: `src/core/vm.rs` lines 1500-1600 (approximately)

**Methods**: `binary_add`, `binary_sub`, `binary_mul`, etc.

### Step 3.3: Move call_function

**Source**: `src/core/vm.rs` line 2293

**Action**: Adapt to use host:

```rust
fn call_function(&mut self, func_id: u32, args: &[Value]) -> Result<Value, VmError> {
    // Try native function first
    match self.host.call_native_function(func_id, args) {
        Ok(result) => Ok(result),
        Err(_) => {
            // Try bytecode function
            if let Some(bytecode_func) = self.host.get_bytecode_function(func_id) {
                self.call_bytecode_function(&bytecode_func, args)
            } else {
                Err(VmError::InvalidOperation)
            }
        }
    }
}
```

### Step 3.4: Move frame creation methods

**Source**: `src/core/vm.rs` lines 1750-1850 (approximately)

**Methods**: `create_bytecode_frame`, etc.

---

## Phase 4: Create DesktopHost

### Step 4.1: Create DesktopHost struct

**File**: `src/core/desktop_host.rs` (new file)

```rust
use std::collections::HashMap;
use std::sync::Arc;
use cantaloop_core::{Host, Value, VmError, BytecodeFunction};
use crate::core::engine::{Engine, BytecodeFunction as EngineBytecodeFunction};
use crate::core::hir_lowering::HirAst;
use crate::core::compileSession::CompileSession;

pub struct DesktopHost {
    engine: Arc<Engine>,
    bytecode_functions: HashMap<u32, EngineBytecodeFunction>,
    hir: HirAst,
    type_registry: HashMap<u32, (String, Vec<String>)>,
}

impl DesktopHost {
    pub fn new(
        engine: Arc<Engine>,
        bytecode_functions: HashMap<u32, EngineBytecodeFunction>,
        hir: HirAst,
        type_registry: HashMap<u32, (String, Vec<String>)>,
    ) -> Self {
        Self {
            engine,
            bytecode_functions,
            hir,
            type_registry,
        }
    }
}

impl Host for DesktopHost {
    fn call_native_function(&mut self, func_id: u32, args: &[Value]) -> Result<Value, VmError> {
        // Convert args to Vec<Value> for engine
        let args_vec: Vec<Value> = args.to_vec();
        
        // Get function from engine
        if let Some(native_func) = self.engine.functions.get(&func_id) {
            // Create a temporary ValueHeap for the call
            // Note: This is a temporary solution - native functions still expect ValueHeap
            // You may need to adapt native function signatures
            let mut heap = crate::core::vm::ValueHeap::new();
            let result = (native_func.func)(args_vec, &mut heap);
            Ok(result)
        } else {
            Err(VmError::InvalidOperation)
        }
    }
    
    fn get_constant(&self, const_id: u32) -> Option<Value> {
        // Use CompileSession to get constant from HIR
        // This requires access to a ValueHeap, which is a challenge
        // You may need to pass storage to get_constant or refactor constants
        None // TODO: Implement
    }
    
    #[cfg(feature = "std")]
    fn get_type_info(&self, type_id: u32) -> Option<(&str, &[String])> {
        self.type_registry.get(&type_id).map(|(name, fields)| {
            (name.as_str(), fields.as_slice())
        })
    }
    
    fn get_bytecode_function(&self, func_id: u32) -> Option<BytecodeFunction> {
        self.bytecode_functions.get(&func_id).map(|bf| {
            BytecodeFunction {
                code: bf.code,
                param_var_ids: Box::leak(Box::new(bf.param_var_ids.clone())),
            }
        })
    }
}
```

**Note**: There are some challenges here:
- Native functions expect `ValueHeap`, but we're using `VmStorage`
- Constants need HIR access, which may need refactoring
- BytecodeFunction needs static lifetime

### Step 4.2: Adapt Native Function Interface

**Option A**: Create adapter that converts `VmStorage` to `ValueHeap` interface
**Option B**: Refactor native functions to use `VmStorage` trait
**Option C**: Keep temporary `ValueHeap` for native function calls (transitional)

For now, Option C is simplest for migration.

---

## Phase 5: Make Desktop VM a Thin Wrapper

### Step 5.1: Create wrapper file

**File**: `src/core/desktop_vm.rs` (new file)

```rust
use cantaloop_core::{VM, DynamicStorage, Host};
use crate::core::desktop_host::DesktopHost;

pub type DesktopVM = VM<DynamicStorage, DesktopHost>;

impl DesktopVM {
    pub fn new(
        engine: std::sync::Arc<crate::core::engine::Engine>,
        bytecode_functions: std::collections::HashMap<u32, crate::core::engine::BytecodeFunction>,
        hir: crate::core::hir_lowering::HirAst,
        ops: Vec<cantaloop_core::OpCode>,
    ) -> Self {
        // Build type registry
        let mut type_registry = std::collections::HashMap::new();
        for (struct_name, struct_def) in &hir.structs {
            let type_id = crate::core::vm::compute_struct_type_id(struct_name);
            let field_names: Vec<String> = struct_def.fields.iter()
                .map(|(name, _)| name.clone())
                .collect();
            type_registry.insert(type_id, (struct_name.clone(), field_names));
        }
        
        // Leak bytecode for static lifetime
        let ops_box = Box::new(ops);
        let ops_slice: &'static [cantaloop_core::OpCode] = Box::leak(ops_box);
        
        // Create storage and host
        let storage = DynamicStorage::new();
        let host = DesktopHost::new(engine, bytecode_functions, hir, type_registry);
        
        // Create VM
        VM::new(storage, host, ops_slice)
    }
}
```

### Step 5.2: Update exports

**File**: `src/core/mod.rs`

```rust
// Remove old VM export
// pub use vm::{VM, Value};

// Add new exports
pub use desktop_vm::DesktopVM;
pub use cantaloop_core::{Value, OpCode, VmError};
```

### Step 5.3: Update all references

**Action**: Find all uses of `VM` and update to `DesktopVM` or `cantaloop_core::VM`.

**Search for**:
- `use crate::core::vm::VM`
- `VM::new(`
- `let mut vm = VM`

---

## Phase 6: Delete Old VM

### Step 6.1: Verify everything works

**Checklist**:
- [ ] All tests pass
- [ ] Desktop VM runs programs correctly
- [ ] No references to old `VM` struct
- [ ] `src/core/vm.rs` only contains:
  - `ValueHeap` (temporary, for native function compatibility)
  - `compute_struct_type_id` helper
  - Type definitions needed by compiler

### Step 6.2: Delete or minimize old VM

**Option A**: Delete `src/core/vm.rs` entirely (if nothing left)
**Option B**: Keep minimal parts (ValueHeap for compatibility, helpers)

### Step 6.3: Update documentation

Update all docs to reference `cantaloop-core::VM` as the canonical VM.

---

## Testing Strategy

### After Each Phase

1. **Compile check**: `cargo check --package cantaloop-core`
2. **Compile main**: `cargo check`
3. **Run tests**: `cargo test`
4. **Run examples**: `cargo run --example embedded_vm`

### Migration Testing

Create a test that runs the same bytecode on both:
- Old VM (during migration)
- New VM (in cantaloop-core)

Compare results to ensure correctness.

---

## Common Patterns and Solutions

### Pattern 1: Stack Operations

**Old**: `_vm.stack.push(val)`
**New**: `self.storage.push(val)?`

### Pattern 2: Heap Access

**Old**: `_vm.heap.arrays[idx]`
**New**: `self.storage.get_array(idx)?.unwrap()`

### Pattern 3: Native Function Calls

**Old**: `_vm.engine.functions.get(&func_id)`
**New**: `self.host.call_native_function(func_id, args)?`

### Pattern 4: Bytecode Functions

**Old**: `_vm.bytecode_functions.get(&func_id)`
**New**: `self.host.get_bytecode_function(func_id)`

### Pattern 5: Constants

**Old**: `CompileSession::get_constant_from_hir(&_vm.hir, id, &mut _vm.heap)`
**New**: `self.host.get_constant(id)?`

### Pattern 6: Error Handling

**Old**: `panic!` or `expect()`
**New**: Return `Result<StepResult, VmError>` and use `?`

---

## Troubleshooting

### Issue: Native functions expect ValueHeap

**Solution**: Create adapter or refactor native function interface gradually.

### Issue: Constants need HIR access

**Solution**: Pass HIR to Host, or pre-compute constants into a table.

### Issue: BytecodeFunction needs static lifetime

**Solution**: Use `Box::leak` to create static references (already done in old VM).

### Issue: Call stack needs fixed-size for no_std

**Solution**: For now, use `Vec` with `std` feature. Add fixed-size version later.

---

## Success Criteria

You're done when:

1. ✅ `cantaloop-core/src/vm.rs` contains all execution logic
2. ✅ `src/core/vm.rs` is deleted or minimal (only compatibility code)
3. ✅ Desktop VM is a thin wrapper: `pub type DesktopVM = VM<DynamicStorage, DesktopHost>;`
4. ✅ All tests pass
5. ✅ Same bytecode runs on desktop and (future) embedded
6. ✅ There is exactly one `VM::step()` implementation
7. ✅ There is exactly one `Value` definition

---

## Next Steps After Migration

1. **Refactor native functions**: Make them use `VmStorage` instead of `ValueHeap`
2. **Add fixed-size call stack**: For full no_std support
3. **Create ESP32 host**: Implement `Host` for embedded platform
4. **Optimize**: Profile and optimize the canonical VM

---

## Estimated Time

- Phase 1: 1-2 hours
- Phase 2: 8-12 hours (48 handlers)
- Phase 3: 2-3 hours
- Phase 4: 2-3 hours
- Phase 5: 1-2 hours
- Phase 6: 1 hour

**Total**: ~15-23 hours of focused work

---

## Tips

1. **Work incrementally**: Move a few handlers, test, repeat
2. **Use git commits**: Commit after each working batch
3. **Keep old VM working**: Don't delete until new one is complete
4. **Test frequently**: Run tests after each change
5. **Ask for help**: If stuck on a specific handler, document the issue

Good luck with the migration! 🚀

