# CantaLoop Architecture

This document describes the architecture of the CantaLoop language implementation. It serves as a guide for understanding the codebase structure and component interactions.

## Overview

CantaLoop is a functional programming language with a complete implementation in Rust, including parser, type checker, bytecode compiler, and virtual machine.

## Pipeline

```
Source Code (.mln)
    ↓
[parser.rs] - Pest-based parsing
    ↓
AST (Abstract Syntax Tree)
    ↓
[semantic_analyser.rs] - Type checking & HIR generation
    ↓
HIR (High-level Intermediate Representation)
    ↓
[bytecode_emitter.rs] - Bytecode compilation
    ↓
Bytecode (OpCode instructions)
    ↓
[vm.rs] - Virtual machine execution
    ↓
Output
```

## Core Components

### parser.rs
- **Responsibility**: Converts source code text into an Abstract Syntax Tree (AST)
- **Technology**: Pest parser generator with Pratt parser for operator precedence
- **Output**: `ast::Program` containing statements and expressions
- **Key Function**: `parse_program()` - Entry point for parsing

### ast.rs / ast_enums.rs / ast_builder.rs
- **Responsibility**: AST representation and construction
- **ast_enums.rs**: Core AST data structures (Expression, Statement, Program, etc.)
- **ast_builder.rs**: Converts Pest parse tree pairs into typed AST nodes
- **Key Types**: `Expression`, `Statement`, `Program`, `Block`

### semantic_analyser.rs
- **Responsibility**: Type checking and High-level Intermediate Representation (HIR) generation
- **Process**:
  1. Traverses AST
  2. Resolves variable types and scope
  3. Validates type correctness
  4. Generates HIR with typed expressions and variable slots
- **Key Types**: `HirBuilder`, `HirAst`, `HirExpression`, `ValueKind`
- **No allocations during type checking** - Uses arena-style scope management

### bytecode.rs / bytecode_opcode.rs / bytecode_emitter.rs
- **Responsibility**: Compiles HIR to bytecode instructions
- **bytecode_opcode.rs**: Defines the instruction set (OpCode enum)
- **bytecode_emitter.rs**: Emits bytecode from HIR AST
- **Key Types**: `OpCode`, `ByteCodeEmitter`
- **Optimizations**: Static number operations (AddNum, MulNum, etc.)

### vm.rs
- **Responsibility**: Executes bytecode instructions
- **Technology**: Stack-based virtual machine with NaN-boxed values
- **Key Features**:
  - Value representation using NaN boxing for efficient type tagging
  - Stack-based execution model
  - Function calls with frame management
  - Thunk evaluation for lazy function application
- **Key Types**: `VM`, `Value`, `ValueHeap`
- **No heap allocations during execution** - Uses pre-allocated heap for strings/thunks

### engine.rs
- **Responsibility**: Orchestrates the entire compilation and execution pipeline
- **Process**:
  1. Parses source file
  2. Builds HIR with type checking
  3. Emits bytecode
  4. Executes in VM
- **Key Types**: `Engine`, `BytecodeFunction`
- **Built-in Functions**: Registers native functions (e.g., `print`)

### lsp.rs / lsp_server.rs
- **Responsibility**: Language Server Protocol implementation for IDE support
- **Features**:
  - Real-time diagnostics (parse errors, type errors)
  - Hover information (variable types, function signatures)
  - Code completion
- **Key Types**: `CantaLoopLSPServer`
- **Technology**: tower-lsp async framework

## Data Flow

### Compilation Phase

1. **Parse**: Source → AST
   - `parse_program()` in parser.rs
   - Uses Pest grammar from `src/grammar/grammar.pest`
   - Builds AST via `ast_builder.rs`

2. **Analyze**: AST → HIR
   - `HirBuilder::build()` in semantic_analyser.rs
   - Performs type inference and checking
   - Creates variable slots and function IDs
   - Generates HIR AST with typed expressions

3. **Compile**: HIR → Bytecode
   - `ByteCodeEmitter::emit_program()` in bytecode_emitter.rs
   - Traverses HIR and emits OpCode instructions
   - Handles control flow (loops, conditionals, jumps)

### Execution Phase

1. **Initialize**: VM setup
   - `VM::new()` creates VM with bytecode
   - Sets up call stack and value stack
   - Initializes value heap

2. **Execute**: Bytecode → Result
   - `VM::run()` executes instructions
   - Uses dispatch table for efficient opcode handling
   - Manages stack and frames

## Value Representation

Values use NaN boxing for efficient type tagging:
- **Numbers**: Valid f64 (not NaN)
- **Other types**: NaN with tag bits in payload (String, Boolean, Function, Thunk)
- Single 64-bit representation for all types
- Heap allocation only for String and Thunk data

## Type System

- **Primitive Types**: Number, String, Boolean
- **Function Types**: `(param_types) -> return_type`
- **Thunk Types**: `(param_types) ~> return_type` (lazy function application)
- Type inference with explicit annotations supported
- Type checking happens in semantic_analyser.rs

## Memory Management

- **Stack-based execution**: Value stack for operands
- **Frame stack**: Function call frames
- **Value heap**: Pre-allocated storage for strings and thunks
- **No GC**: Manual memory management with explicit heap

## Key Design Decisions

1. **Flat module structure**: Avoids deep `mod.rs` trees for better AI tooling
2. **HIR separation**: Explicit separation between AST and typed IR
3. **Bytecode caching**: Functions compiled to bytecode are cached in Engine
4. **NaN boxing**: Efficient value representation without heap allocation for primitives
5. **Dispatch table**: Fast opcode execution using function pointer table

## Testing

Tests are located in `tests/` directory:
- `test_parser.rs`: Parser correctness
- `test_ast.rs`: AST construction
- `test_semantic.rs`: Type checking
- `test_bytecode.rs`: Bytecode emission
- `test_vm.rs`: VM execution
- `test_integration.rs`: End-to-end tests

