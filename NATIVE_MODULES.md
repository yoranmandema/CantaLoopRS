# Native Module System

This document describes the unified native module system for CantaLoop, which provides a declarative way to define native Rust functions that can be used in Melon code.

## Overview

The native module system unifies:
- **Stdlib modules** - Built-in standard library
- **Project-local bindings** - Custom native functions in your project
- **Future extensions** - Plugin system (planned)

All use the same `StdModule` interface and `melon_module!` macro.

## Architecture

### Core Types

```rust
pub struct StdModule {
    pub name: &'static str,
    pub functions: Vec<StdFunction>,
    pub structs: Vec<StdStruct>,
    pub submodules: Vec<StdModule>,
}

pub struct StdFunction {
    pub name: &'static str,
    pub signature: FunctionSignature,
    pub arity: Arity,
    pub impl_fn: Arc<dyn Fn(&[Value], &mut ValueHeap) -> Value + Send + Sync>,
}
```

### Registration API

```rust
impl Engine {
    pub fn register_module(&mut self, module: &StdModule, base_path: &str);
    pub fn load_project_native_modules(&mut self, project_root: &Path) -> Result<(), ...>;
}
```

## Using the Macro

### Basic Example

```rust
use cantaloop::core::engine::StdModule;
use cantaloop::core::vm::Value;
use cantaloop::melon_module;

pub static NUMBER_MODULE: StdModule = melon_module! {
    module number {
        fn add(a: num, b: num) -> num {
            |args, _heap| {
                let a = args[0].as_number().expect("expected number");
                let b = args[1].as_number().expect("expected number");
                Value::number(a + b)
            }
        }
        fn mul(a: num, b: num) -> num {
            |args, _heap| {
                let a = args[0].as_number().expect("expected number");
                let b = args[1].as_number().expect("expected number");
                Value::number(a * b)
            }
        }
    }
};
```

### Supported Types

- `num` - Number (f64)
- `str` - String
- `bool` - Boolean
- `any` - Any type
- `[num]`, `[str]`, `[bool]`, `[any]` - Array types

### Complex Modules

For modules with variadic functions, complex types, or structs, use a hybrid approach:

```rust
use crate::core::engine::{Arity, StdFunction, StdModule};
use crate::core::hir_lowering::{FunctionSignature, ValueKind};
use std::sync::Arc;

pub static MATH_MODULE: StdModule = {
    // Use macro for simple functions
    let mut module = melon_module! {
        module math {
            fn floor(x: num) -> num { /* ... */ }
            fn ceil(x: num) -> num { /* ... */ }
        }
    };
    
    // Manually add variadic functions
    module.functions.extend(vec![
        StdFunction {
            name: "sum",
            signature: FunctionSignature { /* ... */ },
            arity: Arity::Variadic { min: 2 },
            impl_fn: Arc::new(|args, _heap| { /* ... */ }),
        },
    ]);
    
    module
};
```

## Native Function Rules (Guardrails)

Native functions must follow these rules to ensure determinism and safety:

1. **No State Storage**: Do not store `Value` outside the call scope
2. **No Heap References**: Do not keep references to `ValueHeap` after the call
3. **No VM Mutation**: Do not mutate VM state (only use the heap for temporary allocations)
4. **Pure Effects**: All effects must happen via return values

These rules ensure:
- **Determinism** - Same inputs = same outputs
- **Replayability** - Functions can be re-executed safely
- **Debuggability** - No hidden state

## Project-Local Native Modules

### Project Structure

```
my_project/
├── src/
│   └── main.mln
├── native/
│   └── modules.rs
└── melon.json
```

### Example native/modules.rs

```rust
use cantaloop::core::engine::StdModule;
use cantaloop::core::vm::Value;
use cantaloop::melon_module;

pub static MYMATH_MODULE: StdModule = melon_module! {
    module mymath {
        fn hypot(a: num, b: num) -> num {
            |args, _heap| {
                let a = args[0].as_number().expect("expected number");
                let b = args[1].as_number().expect("expected number");
                Value::number((a*a + b*b).sqrt())
            }
        }
    }
};

// Export all modules for discovery
pub const NATIVE_MODULES: &[&StdModule] = &[&MYMATH_MODULE];
```

### Using in Melon Code

```melon
use hypot from mymath;

let result = hypot(3, 4);  // 5.0
```

## Import System

Each module has its own import scope, preventing collisions:

```melon
// file1.mln
use add from number;

// file2.mln  
use add from math;  // No conflict! Each file has its own import scope
```

The `Module` struct includes an `imports` field that tracks imported symbols per module:

```rust
pub struct Module {
    pub functions: HashMap<String, u32>,
    pub constants: HashMap<String, u32>,
    pub structs: HashMap<String, StructDef>,
    pub imports: HashMap<String, u32>,  // Per-module import scope
}
```

## Future Work

1. **Build System Integration**: Automatically compile and link `native/modules.rs`
2. **Struct Methods**: Support for methods on native structs
3. **Extension System**: Plugin architecture for third-party native modules
4. **Runtime Validation**: Compile-time checks for guardrail violations

## Status

✅ **Completed**:
- Unified `StdModule` interface
- `melon_module!` macro for declarative definitions
- `Engine::register_module()` unified API
- Per-module import scopes
- Stdlib refactored to use macro
- Project-local infrastructure

🚧 **In Progress**:
- Array type support in macro (added, needs testing)
- Documentation and examples

📋 **Planned**:
- Build system integration for native modules
- Struct method support
- Extension/plugin system

