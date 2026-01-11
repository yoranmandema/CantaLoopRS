# CantaLoop

A functional programming language implementation in Rust with a complete toolchain including parser, type checker, bytecode compiler, virtual machine, and Language Server Protocol support.

## Features

- **Parser** using Pest with Pratt parser for operator precedence
- **Semantic analysis and type checking** with HIR (High-level Intermediate Representation) lowering
- **Bytecode compilation** with optimization passes
- **Stack-based virtual machine** with NaN-boxed values for efficient execution
- **Language Server Protocol (LSP) support** for IDE integration
- **Documentation system** with structured tags (`@param`, `@returns`, `@effects`) and tooling support
- **Project management tool** (`melon`) for creating and running projects
- **Module system** with dot-path imports
- **Lazy evaluation** with thunks and function composition
- **Standard library** with math, string, array, and functional utilities

## Quick Start

### Using the Melon Tool (Recommended)

The `melon` tool provides project management and execution:

```bash
# Create a new project
cargo run --bin melon -- new myproject
cd myproject

# Run the project
cargo run --bin melon -- run

# Run with watch mode (auto-rebuild on file changes)
cargo run --bin melon -- run --watch

# Run with debug output (AST, HIR, bytecode)
cargo run --bin melon -- run --debug

# Extract documentation from source files
cargo run --bin melon -- docs ./src
```

### Running Individual Files

```bash
# Build the interpreter
cargo build --release

# Run a single file
cargo run --bin melon -- run examples/language_features/helloworld.mln
```

### LSP Setup

The LSP provides real-time diagnostics, hover information, code completion, semantic tokens, and documentation support. See [BUILD.md](BUILD.md) for building and installing the VS Code/Cursor extension.

## Documentation

CantaLoop supports comprehensive documentation with structured tags. See [DOCUMENTATION.md](DOCUMENTATION.md) for complete documentation system guide.

## Building

```bash
# Build all binaries
cargo build --release

# Build specific binaries
cargo build --release --bin melon          # Project manager
cargo build --release --bin cantaloop-lsp  # LSP server
```

## Project Structure

- `src/core/` - Core language implementation
  - `parser.rs` - Pest-based parser
  - `ast/` - Abstract Syntax Tree (enums, builder)
  - `hir_lowering/` - Type checking and HIR generation
  - `bytecode/` - Bytecode compilation (emitter, opcode)
  - `vm.rs` - Stack-based virtual machine
  - `engine.rs` - Compilation and execution orchestration
  - `projectLoader.rs` - Project loading and module resolution
- `src/lsp/` - Language Server Protocol implementation
- `src/melon/` - Project management CLI tool
- `src/stdlib/` - Standard library modules
- `examples/` - Example `.mln` files and projects
- `documentation/` - Architecture and design documentation

See [documentation/ARCHITECTURE.md](documentation/ARCHITECTURE.md) for a detailed description of the system architecture.
