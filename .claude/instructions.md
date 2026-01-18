# CantaLoopRS Project Context

This is a functional programming language compiler and runtime written in Rust, featuring a complete toolchain including parser, type checker, bytecode compiler, VM, and LSP support.

## Key Architecture Components

### Language Features
- **Pure vs Effectful Functions**: Functions are marked with `pure` or `effect` keywords to track side effects at compile time
- **Lazy Evaluation**: Uses thunks and function composition for deferred execution
- **Module System**: Dot-path imports with project loader for module resolution
- **NaN-boxed Values**: Efficient value representation in the VM using NaN-boxing technique

### Compilation Pipeline
1. **Parser** (Pest-based) → AST (Abstract Syntax Tree)
2. **HIR Lowering** → Type checking and semantic analysis → HIR (High-level IR)
3. **Bytecode Emitter** → Stack-based bytecode with optimization passes
4. **VM Execution** → Stack-based virtual machine with NaN-boxed values

### Project Structure
- `src/core/` - Core language implementation
  - `parser.rs` - Pest-based parser with Pratt parsing
  - `ast/` - AST definitions and structures
  - `hir_lowering/` - Type checking, symbol resolution, HIR generation
  - `bytecode/` - Bytecode compilation (emitter, opcode)
  - `vm.rs` - Stack-based virtual machine
  - `engine.rs` - Compilation and execution orchestration
  - `projectLoader.rs` - Module resolution and project loading
- `src/lsp/` - Language Server Protocol implementation
- `src/melon/` - CLI project management tool
- `src/stdlib/` - Standard library modules
- `examples/` - Example `.mln` files and projects

## Code Conventions

### Error Handling
- Use descriptive error messages with span information
- Leverage the `Diagnostic` system for user-facing errors
- Include context about what was expected vs what was found

### Rust Idioms
- Follow Rust borrowing patterns and ownership rules
- Use `Result<T, E>` for fallible operations
- Prefer `Option<T>` over sentinel values
- Use builder patterns for complex object construction

### Architecture Principles
- Maintain clear separation between AST, HIR, and bytecode layers
- Keep the parser free of semantic analysis
- Perform all type checking during HIR lowering
- Emit optimized bytecode as final step

## Common Commands

### Building and Testing
```bash
cargo build --release              # Build all binaries
cargo test                         # Run test suite
cargo check                        # Fast compile check
cargo clippy                       # Linting
cargo fmt                          # Format code
```

### Running CantaLoop Code
```bash
cargo run --bin melon -- run file.mln           # Run a file
cargo run --bin melon -- run --debug file.mln   # Run with debug output
cargo run --bin melon -- run --watch            # Watch mode
cargo run --bin melon -- docs ./src             # Extract documentation
```

### Development Tools
```bash
python scripts/refactor_helper.py find-struct StructName     # Find struct definitions
python scripts/refactor_helper.py find-impl StructName       # Find impl blocks
python scripts/refactor_helper.py find-calls function_name   # Find function calls
python scripts/refactor_helper.py rename-field OldName NewName  # Rename struct fields
```

## Refactoring Strategy

For large Rust refactors, use the Python tree-sitter-based refactoring tool in `scripts/refactor_helper.py`:
1. Parse the codebase with tree-sitter-rust for accurate AST analysis
2. Find all occurrences of structs, functions, or patterns
3. Generate precise edit locations with line/column information
4. Make targeted edits rather than searching with grep

This approach is more reliable than regex-based search and handles Rust's complex syntax correctly.

## Documentation System

CantaLoop supports structured documentation tags:
- `@param name description` - Parameter documentation
- `@returns description` - Return value documentation
- `@effects description` - Side effects documentation
- `@example code` - Usage examples

See DOCUMENTATION.md for complete documentation system guide.

## Current Work

Recent changes focus on:
- Improved pure/effectful function usage tracking
- LSP enhancements for better IDE integration
- Native module support for interfacing with Rust code
- Semantic token generation improvements
