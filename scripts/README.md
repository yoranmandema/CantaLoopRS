# CantaLoopRS Development Scripts

## Refactoring Helper

The `refactor_helper.py` script uses tree-sitter to parse Rust code and provide accurate refactoring assistance.

### Installation

```bash
pip install -r scripts/requirements.txt
```

### Usage

Find all structs:
```bash
python scripts/refactor_helper.py find-struct
```

Find a specific struct:
```bash
python scripts/refactor_helper.py find-struct ASTNode
```

Find all impl blocks for a type:
```bash
python scripts/refactor_helper.py find-impl CompilerState
```

Find all calls to a function:
```bash
python scripts/refactor_helper.py find-calls compile_expr
```

Find struct fields:
```bash
python scripts/refactor_helper.py find-field CompilerState symbols
```

Find enums:
```bash
python scripts/refactor_helper.py find-enum
python scripts/refactor_helper.py find-enum Opcode
```

Find traits:
```bash
python scripts/refactor_helper.py find-trait
python scripts/refactor_helper.py find-trait Visitor
```

List all types in the codebase:
```bash
python scripts/refactor_helper.py list-types
```

### Why Tree-sitter?

Tree-sitter provides accurate AST parsing that:
- Handles complex Rust syntax correctly (lifetimes, generics, macros)
- Avoids false positives from string literals and comments
- Provides precise line/column information
- Understands context (field access vs struct definition)

This is far more reliable than regex-based approaches for refactoring tasks.
