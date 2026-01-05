// Native Rust modules for your Melon project
//
// This file contains native Rust functions that can be used in your Melon code.
// Use the melon_module! macro to define modules declaratively.
//
// Example:
//   use mymath.hypot;
//   let result = hypot(3, 4);  // 5.0

use cantaloop::core::engine::StdModule;
use cantaloop::core::vm::Value;
use cantaloop::melon_module;

#[macro_use]
extern crate lazy_static;

// Define your native modules here
lazy_static::lazy_static! {
    pub static ref MYMATH_MODULE: StdModule = melon_module! {
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
}

// Export all native modules for discovery
// The build system will use this to automatically register modules
pub const NATIVE_MODULES: &[&StdModule] = &[&*MYMATH_MODULE];
