/// Abstract Syntax Tree types for the CantaLoop language.
///
/// This module contains the AST representation of parsed source code,
/// including expressions, statements, and program structure.
///
/// **Note:** The main compilation path uses CST → AST lowering: `Text → CST → AST`
/// This ensures source spans are preserved in the CST layer for LSP integration.

pub mod enums;

pub use enums::*;

