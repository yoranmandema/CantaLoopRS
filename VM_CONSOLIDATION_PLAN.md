# VM Consolidation Plan

## Goal
Move all VM execution logic to `cantaloop-core` so there is exactly one canonical VM implementation.

## Current State
- ❌ `src/core/vm.rs` - Desktop VM with full execution logic (~3200 lines)
- ❌ `cantaloop-core/src/vm.rs` - Basic VM skeleton (~240 lines)
- ❌ Two separate Value definitions (one in each)

## Target State
- ✅ `cantaloop-core/src/vm.rs` - THE VM (single source of truth)
- ✅ `cantaloop-core/src/value.rs` - THE Value (single definition)
- ✅ Desktop VM becomes thin wrapper: `pub type DesktopVM = cantaloop_core::VM<DynamicStorage, DesktopHost>;`

## Migration Steps

### Phase 1: Structure (Current)
- ✅ Create Host trait
- ✅ Make VM generic over Host and Storage
- ✅ Update exports

### Phase 2: Move Execution Logic
- [ ] Move StepResult enum
- [ ] Move CallFrame structure
- [ ] Move execution loop (execute_until_empty)
- [ ] Move all opcode handlers (48 handlers)
- [ ] Move helper methods (binary_add, force_value, etc.)

### Phase 3: Remove Duplicates
- [ ] Delete old VM implementation
- [ ] Update all references
- [ ] Make desktop VM a thin wrapper

## Key Changes Needed

### VM Structure
```rust
// cantaloop-core/src/vm.rs
pub struct VM<S: VmStorage, H: Host> {
    storage: S,
    host: H,
    code: &'static [OpCode],
    ip: usize,
    call_stack: Vec<CallFrame>, // Will need fixed-size for no_std
}
```

### Opcode Handlers
All handlers need to:
- Use `&mut self.storage` instead of `&mut self.stack` and `&mut self.heap`
- Use `&mut self.host` for native functions, constants, etc.
- Be generic over Storage and Host

### Desktop Host Implementation
```rust
// src/core/desktop_host.rs (new file)
pub struct DesktopHost {
    engine: Arc<Engine>,
    bytecode_functions: HashMap<u32, BytecodeFunction>,
    hir: HirAst,
    type_registry: HashMap<u32, (String, Vec<String>)>,
}

impl Host for DesktopHost {
    fn call_native_function(&mut self, func_id: u32, args: &[Value]) -> Result<Value, VmError> {
        // Use engine to call native function
    }
    // ... other methods
}
```

## Files to Create/Modify

### Create
- `src/core/desktop_host.rs` - Desktop Host implementation
- `src/core/desktop_vm.rs` - Thin wrapper around cantaloop-core::VM

### Modify
- `cantaloop-core/src/vm.rs` - Move all execution logic here
- `cantaloop-core/src/host.rs` - Host trait (already created)
- `src/core/vm.rs` - Delete or make thin wrapper
- `src/core/mod.rs` - Update exports

### Delete (eventually)
- Old VM implementation in `src/core/vm.rs` (after migration complete)

## Progress Tracking

- [x] Phase 1: Structure
- [ ] Phase 2: Move Execution Logic (see VM_MIGRATION_GUIDE.md for detailed steps)
- [ ] Phase 3: Remove Duplicates

## Detailed Guide

See `VM_MIGRATION_GUIDE.md` for a comprehensive, step-by-step migration guide with:
- Exact code patterns for each handler
- Common pitfalls and solutions
- Testing strategy
- Success criteria

