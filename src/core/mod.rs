/// Abstract Syntax Tree representation and builder.
pub mod ast;
/// Bytecode compilation and instruction set.
pub mod bytecode;
/// Main engine orchestrating compilation and execution.
pub mod engine;
/// Pest-based parser for CantaLoop source code.
pub mod parser;
/// HIR lowering pass: AST → HIR → CompilerState
/// 
/// This module performs the lowering transformation from AST to High-level Intermediate Representation (HIR),
/// and builds CompilerState as the single source of truth for semantic information.
/// It is no longer just "semantic analysis" - it defines symbol identity, scoping, binding, and semantic meaning.
pub mod hir_lowering;
/// Stack-based virtual machine for bytecode execution.
pub mod vm;

pub mod compileSession;
pub mod projectLoader;
pub mod native_module_loader;

// Re-export commonly used types
pub use engine::Engine;
pub use parser::parse_program;
pub use vm::{VM, Value};
pub use bytecode::{ByteCodeEmitter, OpCode};
pub use hir_lowering::{CompilerState, FunctionSignature, ValueKind, HirBuilder, Symbol, SymbolKind, SymbolTable};

