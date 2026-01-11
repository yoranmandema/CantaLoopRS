# CantaLoop Documentation System

CantaLoop includes a comprehensive documentation system that supports structured documentation for functions, types, constants, and modules. This document describes how to use and work with documentation in CantaLoop.

## Overview

The documentation system follows a three-phase approach:

- **Phase 1**: Minimal viable docs — parse and attach documentation
- **Phase 2**: Structured text — support for tags like `@param`, `@returns`, `@effects`
- **Phase 3**: Doc-aware tooling — linting, extraction, and execution-focused rendering

## Syntax

### Basic Documentation

Documentation comments use triple slashes (`///`) and must appear directly before a declaration:

```cantaloop
/// Computes the sum of two numbers.
fn add(a: num, b: num) -> num {
    a + b
}

/// A mathematical constant.
const PI = 3.14159;

/// Represents a point in 2D space.
struct Point {
    x: num,
    y: num,
}

/// A utility module for math operations.
mod math {
    // ...
}
```

### Rules

1. **Only `///` syntax** — Other comment styles (`//`, `/* */`) are not documentation comments
2. **UTF-8 text** — Plain text only, no special parsing
3. **Attaches to next declaration** — Documentation must immediately precede the declaration
4. **Allowed targets**: `fn`, `const`, `let`, `struct`, `mod`
5. **Disallowed**: Expressions, statements (except `let`), code inside expressions

### Multiple Lines

Documentation comments can span multiple lines. Each line must start with `///`:

```cantaloop
/// Computes the factorial of a number.
/// 
/// This function uses recursion to calculate the factorial.
/// Returns 1 for n = 0 or n = 1.
fn factorial(n: num) -> num {
    if n <= 1 {
        1
    } else {
        n * factorial(n - 1)
    }
}
```

## Structured Documentation (Phase 2)

### Tags

Documentation supports structured tags for parameters, return values, and effects:

#### `@param <name> <description>`

Documents a function parameter:

```cantaloop
/// Calculates the distance between two points.
/// @param x1 The x-coordinate of the first point
/// @param y1 The y-coordinate of the first point
/// @param x2 The x-coordinate of the second point
/// @param y2 The y-coordinate of the second point
/// @returns The Euclidean distance between the points
fn distance(x1: num, y1: num, x2: num, y2: num) -> num {
    let dx = x2 - x1;
    let dy = y2 - y1;
    sqrt(dx * dx + dy * dy)
}
```

#### `@returns <description>`

Documents the return value:

```cantaloop
/// Generates a random number between 0 and 1.
/// @returns A floating-point number in the range [0.0, 1.0)
fn random() -> num {
    // ... implementation
}
```

#### `@effects <description>`

Documents execution effects (especially important for effectful functions):

```cantaloop
/// Prints a message to the console.
/// @effects Executes I/O operation to write to stdout
fn print(message: str) -> void {
    // ... implementation
}

/// Reads input from the user.
/// @effects Executes I/O operation and may await user input
fn read_line() -> str ~> {
    // ... implementation
}
```

#### Custom Tags

You can use any other `@tag` syntax. Unknown tags are preserved as raw text:

```cantaloop
/// Performs a complex computation.
/// @param input The input data
/// @returns The processed result
/// @since 1.0.0
/// @deprecated Use `new_function` instead
fn old_function(input: str) -> str {
    // ...
}
```

### Main Description

The main description is the text that appears before any tags. It's typically the first paragraph explaining what the declaration does:

```cantaloop
/// Calculates the greatest common divisor of two integers.
/// 
/// This uses the Euclidean algorithm for efficient computation.
/// 
/// @param a First integer
/// @param b Second integer
/// @returns The GCD of a and b
fn gcd(a: num, b: num) -> num {
    if b == 0 { a } else { gcd(b, a % b) }
}
```

## Native Module Documentation

Native Rust modules and functions can also have documentation. Use the `melon_module!` macro with documentation:

```rust
lazy_static::lazy_static! {
    pub static ref MATH_MODULE: StdModule = melon_module! {
        module math {
            /// Computes the sum of an array of numbers.
            /// @param numbers Array of numbers to sum
            /// @returns The sum of all numbers in the array
            fn sum(numbers: Array<num>) -> num {
                |args, _heap| {
                    // ... implementation
                }
            }
        }
    };
}
```

## Tooling (Phase 3)

### LSP Integration

The Language Server Protocol (LSP) provides rich documentation support:

- **Hover**: Hovering over a declaration shows its documentation with syntax highlighting
- **Execution-focused rendering**: Execution-related words like "await", "execute", "lazy", "effect" are emphasized
- **Effectful badges**: Functions with effects show a ⚡ badge
- **Structured display**: Parameters, returns, and effects are shown in organized sections

### Documentation Extraction

Use the `melon docs` command to extract all documentation from your project:

```bash
# Extract docs from src/ directory
melon docs ./src

# Extract docs from a specific file
melon docs ./src/main.mln
```

The output is JSON format (canonical) with all documentation in source order:

```json
[
  {
    "file": "src/main.mln",
    "identifier": "add",
    "text": "Computes the sum of two numbers.\n@param a First number\n@param b Second number\n@returns The sum of a and b",
    "tags": {
      "params": {
        "a": "First number",
        "b": "Second number"
      },
      "returns": "The sum of a and b",
      "effects": null,
      "other": {}
    }
  }
]
```

### Documentation Linting (Optional)

The LSP includes optional documentation linting that can warn about:

- Public functions without documentation
- Exported constants without documentation
- Trivial or incomplete documentation ("TODO", "fix", etc.)
- Effectful functions without effect documentation

**Note**: Linting is disabled by default and only produces warnings (never errors). It's a tool-side feature, not part of the compiler core.

## Best Practices

### Document Public APIs

Always document public (`pub`) functions, constants, and modules:

```cantaloop
/// Exports a utility function for external use.
pub fn public_api(param: str) -> num {
    // ...
}
```

### Document Effects

For effectful functions (those with `~>` return type), always document their execution behavior:

```cantaloop
/// Reads from a file.
/// @param path The file path to read
/// @returns The file contents
/// @effects Executes I/O operation to read from disk
fn read_file(path: str) -> str ~> {
    // ...
}
```

### Use Clear Descriptions

Write clear, concise descriptions:

```cantaloop
// Good
/// Calculates the area of a circle given its radius.

// Bad
/// TODO: fix this
/// area
```

### Organize with Tags

Use structured tags to organize complex documentation:

```cantaloop
/// Performs matrix multiplication.
/// 
/// This function multiplies two matrices using standard matrix multiplication rules.
/// The matrices must be compatible (number of columns in first equals rows in second).
/// 
/// @param a First matrix (m×n)
/// @param b Second matrix (n×p)
/// @returns Resulting matrix (m×p)
/// @effects Performs O(mnp) arithmetic operations
fn matmul(a: Matrix, b: Matrix) -> Matrix {
    // ...
}
```

## Philosophy

CantaLoop's documentation system follows these principles:

1. **Docs attach to syntax, not meaning** — Documentation is associated with the declaration in source code, not with semantic meanings
2. **Docs explain execution, never control it** — Documentation is purely informational; it never affects code execution
3. **Minimal and honest** — The system is simple and straightforward, avoiding complex features that could break the model
4. **Tooling-focused** — Advanced features like linting and extraction are tooling concerns, not language features

## Examples

See `examples/doc_example.mln` for a complete example demonstrating all documentation features.

## Implementation Details

- Documentation is parsed during CST building
- Attached to declarations via identifier spans
- Stored in `CompilerState` for LSP access
- Lowered from CST to AST with identifier names as keys
- Native modules integrate documentation through `StdFunction`, `StdModule`, and `StdStruct`

---

For more information, see:
- [README.md](README.md) — General project overview
- [NATIVE_MODULES.md](NATIVE_MODULES.md) — Native module documentation
- [BUILD.md](BUILD.md) — Building and development
