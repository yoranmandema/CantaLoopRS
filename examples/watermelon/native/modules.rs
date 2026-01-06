// // Native Rust modules for your Melon project
// //
// // This file contains native Rust functions that can be used in your Melon code.
// // Use the melon_module! macro to define modules declaratively.
// //
// // Example:
// //   use mymath.hypot;
// //   let result = hypot(3, 4);  // 5.0

// use cantaloop::core::engine::StdModule;
// use cantaloop::core::vm::Value;
// use cantaloop::melon_module;

// // Define your native modules here
// lazy_static::lazy_static! {
//     pub static ref MYMATH_MODULE: StdModule = melon_module! {
//         module mymath {
//             fn hypot(a: num, b: num) -> num {
//                 |args, _heap| {
//                     let a = args[0].as_number().expect("expected number");
//                     let b = args[1].as_number().expect("expected number");
//                     Value::number((a*a + b*b).sqrt())
//                 }
//             }
//         }
//     };
// }

// // Export a registration function that can be called from the melon runtime
// #[no_mangle]
// pub extern "C" fn register_native_modules(engine: *mut cantaloop::core::engine::Engine) {
//     unsafe {
//         if let Some(engine_ref) = engine.as_mut() {
//             // Register modules directly (can't use const array with lazy_static)
//             engine_ref.register_module(&*MYMATH_MODULE, "");
//         }
//     }
// }
