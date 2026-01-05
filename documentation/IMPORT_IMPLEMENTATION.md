# Dot-Path Import Implementation Summary

This document summarizes the implementation of dot-path imports in CantaLoop.

## Syntax

CantaLoop supports three forms of import statements:

1. **Single import**: `use math.utils.square;`
2. **Multiple imports**: `use math.utils.{cube, pow};`
3. **Wildcard import**: `use math.utils.*;`

## Grammar Changes

### `src/grammar/grammar.pest`

Added the following rules:

```pest
// Import/Use statements
use_statement = ${ "use" ~ WHITESPACE+ ~ import_path ~ (WHITESPACE* ~ import_selector)? }
import_path = { identifier ~ ("." ~ identifier)* }
import_selector = { "." ~ identifier | "{" ~ WHITESPACE* ~ import_list? ~ WHITESPACE* ~ "}" | "*" }
import_list = { identifier ~ (WHITESPACE* ~ "," ~ WHITESPACE* ~ identifier)* }
```

The `use` keyword was also added to the keyword list to prevent it from being used as an identifier.

## AST Changes

### `src/core/ast/enums.rs`

Added `Statement::Use` variant:

```rust
Statement::Use {
    path: Vec<String>, // Dot-separated path like ["math", "utils"]
    selector: ImportSelector,
}
```

Added `ImportSelector` enum:

```rust
pub enum ImportSelector {
    /// Import a single name: `use math.utils.square;`
    Single(String),
    /// Import multiple names: `use math.utils.{cube, pow};`
    Multiple(Vec<String>),
    /// Import all: `use math.utils.*;`
    Wildcard,
}
```

## AST Builder Changes

### `src/core/ast/builder.rs`

Added `build_use_statement()` function that:
1. Parses the import path (dot-separated identifiers)
2. Parses the optional import selector
3. If no selector is present and the path has multiple components, treats the last component as the selector

This handles the syntax `use math.utils.square;` correctly by splitting it into:
- path: `["math", "utils"]`
- selector: `Single("square")`

## Semantic Analysis Changes

### `src/core/hir_lowering/mod.rs` and related files

#### ImportTable Type

```rust
/// Maps imported symbol names to their function IDs.
/// Used for compile-time resolution of imports.
pub type ImportTable = HashMap<String, u32>; // symbol_name -> function_id
```

#### HirBuilder Fields

Added to `HirBuilder`:
- `modules: HashMap<String, Module>` - Maps module paths to their modules
- `import_table: ImportTable` - Maps imported symbol names to function IDs

#### Module Registration

```rust
pub fn register_module(&mut self, path: &str, functions: HashMap<String, u32>) {
    self.modules.insert(path.to_string(), Module { functions });
}
```

#### Import Resolution

```rust
fn resolve_import(&self, path: &[String], selector: &ImportSelector) -> Result<ImportTable, HirError> {
    let module_path = path.join(".");
    let module = self.modules.get(&module_path)
        .ok_or_else(|| HirError::TypeError(format!("Module '{}' not found", module_path)))?;

    let mut imports = ImportTable::new();

    match selector {
        ImportSelector::Single(name) => {
            let func_id = module.functions.get(name)
                .ok_or_else(|| HirError::TypeError(format!("Function '{}' not found in module '{}'", name, module_path)))?;
            imports.insert(name.clone(), *func_id);
        }
        ImportSelector::Multiple(names) => {
            for name in names {
                let func_id = module.functions.get(name)
                    .ok_or_else(|| HirError::TypeError(format!("Function '{}' not found in module '{}'", name, module_path)))?;
                imports.insert(name.clone(), *func_id);
            }
        }
        ImportSelector::Wildcard => {
            // Import all functions from the module
            for (name, func_id) in &module.functions {
                imports.insert(name.clone(), *func_id);
            }
        }
    }

    Ok(imports)
}
```

#### Use Statement Processing

In `process_statement()`, `Statement::Use` is handled:

```rust
Statement::Use { path, selector } => {
    // Process imports at compile-time: resolve symbols and add to import table
    let imports = self.resolve_import(&path, &selector)?;
    // Merge imports into the import table
    for (name, func_id) in imports {
        if self.import_table.contains_key(&name) {
            return Err(HirError::TypeError(format!("Symbol '{}' already imported", name)));
        }
        self.import_table.insert(name, func_id);
    }
    // Use statements don't generate any HIR statements (compile-time only)
    Ok(HirStmt::Nop)
}
```

#### Name Resolution

Imported symbols are checked first in identifier resolution:

1. In `process_identifier()`:
```rust
// First check imported symbols (compile-time resolved)
if let Some(function_id) = self.import_table.get(&identifier) {
    // Imported function - convert to thunk by calling with no args
    Ok(HirExpression::FunctionCall {
        function_id: *function_id,
        args: Vec::new(),
        invoke: false,
    })
} else if let Some(slot) = self.resolve_var(&identifier) {
    // ... variable resolution
}
```

2. In `process_function_call()`:
```rust
// Check imported symbols first (compile-time resolved)
let imported_func_id = self.import_table.get(identifier_name).copied();
let regular_func_id = self.resolve_function(identifier_name);

if let Some(function_id) = imported_func_id.or(regular_func_id) {
    // It's a function (imported or regular) - create FunctionCall
    // ...
}
```

## Engine Changes

### `src/core/engine.rs`

Added `register_module()` method:

```rust
/// Register a module that can be imported.
/// 
/// # Arguments
/// * `path` - Dot-separated module path (e.g., "math.utils")
/// * `functions` - Map of function names to their function IDs
pub fn register_module(&mut self, path: &str, functions: HashMap<String, u32>) {
    self.hir_builder.register_module(path, functions);
}
```

Added `get_function_id_by_name()` helper method:

```rust
/// Get the function ID for a registered function by name.
/// 
/// Returns None if the function is not found.
pub fn get_function_id_by_name(&self, name: &str) -> Option<u32> {
    self.hir_builder.resolve_function(name)
}
```

This makes it easier to register modules by looking up function IDs after registering functions.

## Compile-Time Only

**Important**: Imports are resolved at compile-time only. The bytecode and VM never see import statements - they only see `LdFunc(id)` instructions. The import resolution happens during semantic analysis, and imported symbols are mapped directly to function IDs in the HIR.

## Example Usage

```rust
use std::collections::HashMap;
use cantaloop::Engine;

let mut engine = Engine::new();

// Register functions
add_number_fn!(engine, "square", 1, |args: &[f64]| args[0] * args[0]);
add_number_fn!(engine, "cube", 1, |args: &[f64]| args[0] * args[0] * args[0]);
add_number_fn!(engine, "pow", 2, |args: &[f64]| args[0].powf(args[1]));

// Get function IDs by name
let square_id = engine.get_function_id_by_name("square").unwrap();
let cube_id = engine.get_function_id_by_name("cube").unwrap();
let pow_id = engine.get_function_id_by_name("pow").unwrap();

// Register module
let mut math_utils = HashMap::new();
math_utils.insert("square".to_string(), square_id);
math_utils.insert("cube".to_string(), cube_id);
math_utils.insert("pow".to_string(), pow_id);
engine.register_module("math.utils", math_utils);

// Now programs can use:
// use math.utils.square;
// use math.utils.{cube, pow};
// use math.utils.*;
```

## Summary

The import system is fully compile-time: imports are resolved during semantic analysis, mapped to function IDs, and stored in the `ImportTable`. The bytecode compiler and VM only see function IDs, never import statements. This keeps the runtime simple while providing a clean module system at the language level.

