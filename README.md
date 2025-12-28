# CantaLoopRS

A programmable functional programming language implementation in Rust.

## Features

- Parser using Pest
- Semantic analysis and type checking
- Bytecode compilation
- Virtual machine execution
- **Language Server Protocol (LSP) support** - See [QUICKSTART.md](QUICKSTART.md) for setup

## Quick Start

### Running Programs

```bash
cargo run --bin CantaLoopRS -- examples/helloworld.mln
```

### LSP Setup

To use the LSP in your editor (Cursor/VS Code) for `.mln` files, see [QUICKSTART.md](QUICKSTART.md).

The LSP provides:

- Parse error diagnostics
- Hover information
- Code completion
- Syntax highlighting (with extension)

## Building

```bash
# Build the main interpreter
cargo build --release

# Build the LSP server
cargo build --release --bin cantaloop-lsp
```

## Project Structure

- `src/parser.rs` - Pest-based parser
- `src/semantic_analyser.rs` - Type checking and HIR generation
- `src/bytecode.rs` - Bytecode emission (with `bytecode_emitter.rs`, `bytecode_opcode.rs`)
- `src/vm.rs` - Virtual machine implementation
- `src/lsp.rs` - Language Server Protocol implementation (with `lsp_server.rs`)
- `src/ast.rs` - Abstract Syntax Tree (with `ast_enums.rs`, `ast_builder.rs`)
- `examples/` - Example `.mln` files

See [ARCHITECTURE.md](ARCHITECTURE.md) for a detailed description of the system architecture.
