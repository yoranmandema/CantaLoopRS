# Native Modules

This directory contains native Rust modules for your Melon project.

## Quick Start

1. **Configure the dependency**: Edit `Cargo.toml` and uncomment/configure the `cantaloop` dependency
   - If cantaloop is published: `cantaloop = "0.1.0"`
   - If using local path: `cantaloop = { path = "../../cantaloop" }`
   - If using git: `cantaloop = { git = "https://github.com/..." }`

2. **Define your modules**: Edit `modules.rs` to add your native functions using the `melon_module!` macro

3. **Run your project**: `melon run` will automatically compile and load your native modules

## Example Usage in Melon

```melon
use hypot from mymath;

let result = hypot(3, 4);  // 5.0
```

## How It Works

1. Configure `cantaloop` dependency in `Cargo.toml`
2. Define modules in `modules.rs` using the `melon_module!` macro
3. The `register_native_modules` function exports them for the melon runtime
4. Run `melon run` - it automatically compiles and loads your native modules
5. Import and use them in your Melon code

## Adding More Modules

Just add more module definitions to `modules.rs` and register them in `register_native_modules`:

```rust
lazy_static::lazy_static! {
    pub static ref MYUTILS_MODULE: StdModule = melon_module! {
        module myutils {
            // ... functions ...
        }
    };
}

#[no_mangle]
pub extern "C" fn register_native_modules(engine: *mut cantaloop::core::engine::Engine) {
    unsafe {
        if let Some(engine_ref) = engine.as_mut() {
            engine_ref.register_module(&*MYMATH_MODULE, "");
            engine_ref.register_module(&*MYUTILS_MODULE, "");  // Add more here
        }
    }
}
```

See NATIVE_MODULES.md for detailed documentation.
