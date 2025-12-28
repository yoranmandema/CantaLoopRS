# Import System Examples

This directory contains examples demonstrating CantaLoop's dot-path import system.

## Syntax

CantaLoop supports three forms of import statements:

1. **Single import**: `use math.utils.square;`
2. **Multiple imports**: `use math.utils.{cube, pow};`
3. **Wildcard import**: `use math.utils.*;`

## Example Usage

See `module_example.mln` for a complete example using all three import forms.

## Registering Modules

To make functions available for import, you need to:

1. Register the functions with the Engine
2. Get their function IDs
3. Register them as a module

Example Rust code:

```rust
use std::collections::HashMap;
use CantaLoopRS::Engine;

let mut engine = Engine::new();

// Register functions
add_number_fn!(engine, "square", 1, |args: &[f64]| args[0] * args[0]);
add_number_fn!(engine, "cube", 1, |args: &[f64]| args[0] * args[0] * args[0]);
add_number_fn!(engine, "pow", 2, |args: &[f64]| args[0].powf(args[1]));

// Get function IDs by name
let square_id = engine.get_function_id_by_name("square").unwrap();
let cube_id = engine.get_function_id_by_name("cube").unwrap();
let pow_id = engine.get_function_id_by_name("pow").unwrap();

// Register as a module
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

## How It Works

Imports are resolved at **compile-time only**. The semantic analyser:

1. Parses `use` statements during AST building
2. Resolves import paths to modules registered with the Engine
3. Maps imported symbol names to function IDs
4. Stores them in an `ImportTable` for name resolution

When the code references an imported function, the semantic analyser looks it up in the `ImportTable` and generates a `FunctionCall` with the resolved function ID. The bytecode compiler and VM never see import statements - they only see `LdFunc(id)` instructions.

