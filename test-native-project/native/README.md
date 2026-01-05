# Native Modules

This directory contains native Rust modules for your Melon project.

## Quick Start

1. Edit `modules.rs` to define your native modules using the `melon_module!` macro
2. The modules will be automatically discovered and loaded when you run `melon run`

## Example Usage in Melon

```melon
use hypot from mymath;

let result = hypot(3, 4);  // 5.0
```

## How It Works

1. Define modules in `modules.rs` using the `melon_module!` macro
2. Export them in the `NATIVE_MODULES` constant
3. The melon runtime automatically discovers and registers them
4. Import and use them in your Melon code

## Adding More Modules

Just add more module definitions to `modules.rs` and include them in `NATIVE_MODULES`:

```rust
pub const NATIVE_MODULES: &[&StdModule] = &[
    &*MYMATH_MODULE,
    &*MYUTILS_MODULE,  // Add more modules here
];
```

See NATIVE_MODULES.md for detailed documentation.
