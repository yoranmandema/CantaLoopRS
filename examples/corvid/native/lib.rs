// Re-export modules.rs as lib.rs
#[macro_use]
extern crate lazy_static;

pub mod modules;
pub use modules::*;

// The registration function is now in modules.rs

