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
[hir_lowering/] - Type checking & HIR generation
    ↓
HIR (High-level Intermediate Representation)
    ↓
[bytecode/emitter.rs] - Bytecode compilation
    ↓
Bytecode (OpCode instructions)
    ↓
[vm.rs] - Virtual machine execution
    ↓
Output
```

## Core Components

### parser.rs
- **Location**: `src/core/parser.rs`
- **Responsibility**: Converts source code text into an Abstract Syntax Tree (AST)
- **Technology**: Pest parser generator with Pratt parser for operator precedence
- **Grammar**: Defined in `src/grammar/grammar.pest`
- **Output**: `ast::Program` containing statements and expressions
- **Key Function**: `parse_program()` - Entry point for parsing

### ast/ (enums.rs, builder.rs, mod.rs)
- **Location**: `src/core/ast/`
- **Responsibility**: AST representation and construction
- **enums.rs**: Core AST data structures (Expression, Statement, Program, etc.)
- **builder.rs**: Converts Pest parse tree pairs into typed AST nodes
- **Key Types**: `Expression`, `Statement`, `Program`, `Block`

### hir_lowering/ (mod.rs, lower_expr.rs, lower_stmt.rs, scopes.rs, symbols.rs, project_semantic_items.rs)
- **Location**: `src/core/hir_lowering/`
- **Responsibility**: Type checking and High-level Intermediate Representation (HIR) generation
- **Process**:
  1. Traverses AST
  2. Resolves variable types and scope
  3. Validates type correctness
  4. Generates HIR with typed expressions and variable slots
  5. Manages symbol tables and module resolution
- **Key Types**: `HirBuilder`, `HirAst`, `HirExpression`, `ValueKind`, `CompilerState`, `Symbol`, `SymbolTable`
- **No allocations during type checking** - Uses arena-style scope management
- **Module System**: Handles dot-path imports and module resolution

### bytecode/ (opcode.rs, emitter.rs, mod.rs)
- **Location**: `src/core/bytecode/`
- **Responsibility**: Compiles HIR to bytecode instructions
- **opcode.rs**: Defines the instruction set (OpCode enum)
- **emitter.rs**: Emits bytecode from HIR AST
- **Key Types**: `OpCode`, `ByteCodeEmitter`
- **Optimizations**: Static number operations (AddNum, MulNum, etc.)

### vm.rs
- **Location**: `src/core/vm.rs`
- **Responsibility**: Executes bytecode instructions
- **Technology**: Stack-based virtual machine with NaN-boxed values
- **Key Features**:
  - Value representation using NaN boxing for efficient type tagging
  - Stack-based execution model
  - Function calls with frame management
  - Thunk evaluation for lazy function application
  - Composed thunk support for function composition
- **Key Types**: `VM`, `Value`, `ValueHeap`
- **No heap allocations during execution** - Uses pre-allocated heap for strings/thunks

### engine.rs
- **Location**: `src/core/engine.rs`
- **Responsibility**: Orchestrates the entire compilation and execution pipeline
- **Process**:
  1. Parses source file
  2. Builds HIR with type checking
  3. Emits bytecode
  4. Executes in VM
- **Key Types**: `Engine`, `BytecodeFunction`, `RunArtifacts`
- **Built-in Functions**: Registers native functions (e.g., `print`)
- **Module Registration**: Supports registering modules for import system

### projectLoader.rs
- **Location**: `src/core/projectLoader.rs`
- **Responsibility**: Loads melon projects from `melon.json` files
- **Features**:
  - Resolves project entry point
  - Handles project dependencies
  - Manages project structure

### lsp/ (main.rs, server.rs, compiler_state.rs, diagnostics.rs, hover.rs, completion.rs, semantic_tokens.rs)
- **Location**: `src/lsp/`
- **Responsibility**: Language Server Protocol implementation for IDE support
- **Features**:
  - Real-time diagnostics (parse errors, type errors)
  - Hover information (variable types, function signatures)
  - Code completion
  - Semantic tokens for syntax highlighting
- **Key Types**: `CantaLoopLSPServer`, `CompilerState`
- **Technology**: tower-lsp async framework

## Data Flow

### Compilation Phase

1. **Parse**: Source → AST
   - `parse_program()` in `src/core/parser.rs`
   - Uses Pest grammar from `src/grammar/grammar.pest`
   - Builds AST via `src/core/ast/builder.rs`

2. **Analyze**: AST → HIR
   - `HirBuilder::build()` in `src/core/hir_lowering/mod.rs`
   - Performs type inference and checking
   - Creates variable slots and function IDs
   - Generates HIR AST with typed expressions
   - Resolves imports and module symbols

3. **Compile**: HIR → Bytecode
   - `ByteCodeEmitter::emit_program()` in `src/core/bytecode/emitter.rs`
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

## Project Management

### Melon Tool

The `melon` binary (`src/melon/main.rs`) provides project management:

- **`melon new [name]`**: Creates a new project with `melon.json` configuration
- **`melon run [--watch|-w] [--debug]`**: Runs a project with optional watch mode and debug output
- **Project Structure**: Projects use `melon.json` to define entry point and dependencies
- **Watch Mode**: Automatically rebuilds and reruns on file changes
- **Debug Output**: Generates `ast.json`, `hir.json`, and `bytecode.txt` in `.melon/debug/`

## Testing

Tests are located in `tests/` directory:
- `test_parser.rs`: Parser correctness
- `test_ast.rs`: AST construction
- `test_semantic.rs`: Type checking and HIR lowering
- `test_bytecode.rs`: Bytecode emission
- `test_vm.rs`: VM execution
- `test_integration.rs`: End-to-end tests
- `test_modules.rs`: Module system and imports
- `test_pub_fn_*.rs`: Public function and import testing

