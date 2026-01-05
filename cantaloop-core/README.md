# cantaloop-core

Core VM types and storage abstractions for CantaLoop, designed for both desktop and embedded systems.

## Overview

This crate provides the foundational types for the CantaLoop VM, with a storage abstraction that allows the same bytecode to run on both:
- **Desktop systems**: Uses `DynamicStorage` (Vec/HashMap) for dynamic memory allocation
- **Embedded systems**: Uses `FixedStorage` (fixed-size arrays) for deterministic, no-heap execution

## Key Components

### Value
The NaN-boxed value type that represents all CantaLoop values (numbers, strings, arrays, structs, thunks, etc.).

### OpCode
The bytecode instruction set. Same instructions work on both desktop and embedded.

### VmStorage Trait
The storage abstraction that provides:
- Stack operations (push/pop/peek)
- Heap allocation (strings, arrays, structs, thunks, iterators)

### Storage Implementations

#### DynamicStorage (std only)
```rust
use cantaloop_core::DynamicStorage;

let mut storage = DynamicStorage::new();
storage.push(Value::number(42.0))?;
let value = storage.pop();
```

#### FixedStorage (no_std compatible)
```rust
use cantaloop_core::FixedStorage;

// Fixed-size storage: 256 stack slots, 64 heap slots
type EmbeddedStorage = FixedStorage<256, 64>;
let mut storage = EmbeddedStorage::new();
storage.push(Value::number(42.0))?;
```

## Usage

### Desktop VM
```rust
use cantaloop_core::{DynamicStorage, DesktopVM};

let storage = DynamicStorage::new();
let mut vm = DesktopVM::new(storage);
// ... execute bytecode
```

### Embedded VM (Future)
```rust
use cantaloop_core::{FixedStorage, VM};

type EmbeddedVM = VM<FixedStorage<256, 64>>;
let storage = FixedStorage::<256, 64>::new();
let mut vm = EmbeddedVM::new(storage);
// ... execute bytecode
```

## Current Status

✅ Core types extracted (Value, OpCode, VmStorage)  
✅ DynamicStorage implementation  
✅ FixedStorage implementation (basic)  
⏳ Full VM implementation generic over storage  
⏳ Complete no_std support (some types still need std::Vec alternatives)

## Design Philosophy

The storage abstraction separates **what** the VM does (execute bytecode) from **how** it stores data (heap vs fixed arrays). This allows:

1. **Same language, different memory strategy**: Desktop uses dynamic allocation, embedded uses fixed-size arrays
2. **Deterministic execution**: Embedded VM has predictable memory usage
3. **Portable bytecode**: Same bytecode runs on both platforms
4. **Gradual migration**: Existing desktop VM can continue working while embedded VM is developed

## Limitations

- `StructData` and `ThunkData` currently require `std::Vec` - need fixed-size alternatives for full no_std
- String handling in `FixedStorage` needs implementation
- Call stack in VM needs fixed-size version for no_std

These can be addressed incrementally as needed for specific embedded targets.

