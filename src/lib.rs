//! CantaLoop language implementation library.
//! 
//! This crate provides a complete implementation of the CantaLoop functional
//! programming language, including parsing, type checking, bytecode compilation,
//! and virtual machine execution.
//!
//! # Example
//!
//! ```rust
//! use CantaLoopRS::{Engine, FunctionSignature, ValueKind};
//!
//! let mut engine = Engine::new();
//! engine.add_function("print", FunctionSignature {
//!     params: vec![ValueKind::String],
//!     return_type: Box::new(ValueKind::String),
//! }, |args| {
//!     println!("{}", args[0]);
//!     "".to_string()
//! });
//! engine.run("examples/helloworld.mln");
//! ```

#![allow(non_snake_case)]
/// Abstract Syntax Tree representation and builder.
pub mod ast;
/// Pest-based parser for CantaLoop source code.
pub mod parser;
/// Bytecode compilation and instruction set.
pub mod bytecode;
/// Main engine orchestrating compilation and execution.
pub mod engine;
/// Stack-based virtual machine for bytecode execution.
pub mod vm;
/// Type checking and High-level Intermediate Representation (HIR) generation.
pub mod semantic_analyser;
/// Language Server Protocol implementation for IDE support.
pub mod lsp;

#[macro_use]
extern crate lazy_static;

// Re-export commonly used types for easier testing
pub use engine::Engine;
pub use parser::parse_program;
pub use vm::{VM, Value};
pub use bytecode::{ByteCodeEmitter, OpCode};
pub use semantic_analyser::{FunctionSignature, ValueKind, HirBuilder};

