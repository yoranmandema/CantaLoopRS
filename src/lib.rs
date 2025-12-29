//! CantaLoop language implementation library.
//! 
//! This crate provides a complete implementation of the CantaLoop functional
//! programming language, including parsing, type checking, bytecode compilation,
//! and virtual machine execution.
//!
//! # Example
//!
//! ```rust,no_run
//! use CantaLoopRS::{Engine, FunctionSignature, ValueKind};
//! use CantaLoopRS::core::engine::Arity;
//!
//! let mut engine = Engine::new();
//! engine.add_string_function("print", FunctionSignature {
//!     params: vec![ValueKind::String],
//!     return_type: Box::new(ValueKind::String),
//! }, Arity::Fixed(1), |args| {
//!     println!("{}", args[0]);
//!     "".to_string()
//! });
//! engine.run("examples/helloworld.mln");
//! ```

#![allow(non_snake_case)]

/// Core CantaLoop language implementation.
pub mod core;
/// Language Server Protocol implementation for IDE support.
pub mod lsp;
/// Standard library modules.
pub mod stdlib;

#[macro_use]
extern crate lazy_static;

// Re-export commonly used types for easier access
pub use core::{
    Engine, parse_program, VM, Value, ByteCodeEmitter, OpCode,
    CompilerState, FunctionSignature, ValueKind, HirBuilder, Symbol, SymbolKind, SymbolTable
};

