// Library entry point for integration tests
pub mod ast;
pub mod parser;
pub mod bytecode;
pub mod engine;
pub mod vm;
pub mod semantic_analyser;

#[macro_use]
extern crate lazy_static;

// Re-export commonly used types for easier testing
pub use engine::Engine;
pub use parser::parse_program;
pub use vm::{VM, Value};
pub use bytecode::{emitter::ByteCodeEmitter, opcode::OpCode};
pub use semantic_analyser::{FunctionSignature, ValueKind, HirBuilder};

