/// Bytecode compilation and instruction set for the CantaLoop VM.
/// 
/// This module handles:
/// - Bytecode instruction definitions (OpCode)
/// - Compilation from HIR AST to bytecode instructions

#[path = "bytecode_opcode.rs"]
pub mod opcode;

#[path = "bytecode_emitter.rs"]
pub mod emitter;

pub use opcode::*;
pub use emitter::ByteCodeEmitter;

