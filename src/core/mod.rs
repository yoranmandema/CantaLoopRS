/// Abstract Syntax Tree representation and builder.
pub mod ast;
pub mod ast_builder;
pub mod ast_enums;
/// Bytecode compilation and instruction set.
pub mod bytecode;
pub mod bytecode_emitter;
pub mod bytecode_opcode;
/// Main engine orchestrating compilation and execution.
pub mod engine;
/// Pest-based parser for CantaLoop source code.
pub mod parser;
/// Type checking and High-level Intermediate Representation (HIR) generation.
pub mod semantic_analyser;
/// Stack-based virtual machine for bytecode execution.
pub mod vm;

// Re-export commonly used types
pub use engine::Engine;
pub use parser::parse_program;
pub use vm::{VM, Value};
pub use bytecode::{ByteCodeEmitter, OpCode};
pub use semantic_analyser::{CompilerState, FunctionSignature, ValueKind, HirBuilder, Symbol, SymbolKind, SymbolTable};

