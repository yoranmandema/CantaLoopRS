/// Abstract Syntax Tree types and builder for the CantaLoop language.
/// 
/// This module contains the AST representation of parsed source code,
/// including expressions, statements, and program structure.
/// 
/// **Note:** The AST builder is kept for reference, but the main compilation path
/// now uses CST → AST lowering: `Text → CST → AST`
/// This ensures source spans are preserved in the CST layer for LSP integration.

pub mod enums;
pub mod builder;

pub use enums::*;
// Keep builder available for reference, but mark as deprecated
#[allow(deprecated)]
pub use builder::build_program;

