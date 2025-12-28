/// Abstract Syntax Tree types and builder for the CantaLoop language.
/// 
/// This module contains the AST representation of parsed source code,
/// including expressions, statements, and program structure.

#[path = "ast_enums.rs"]
pub mod enums;

#[path = "ast_builder.rs"]
pub mod builder;

pub use enums::*;
pub use builder::build_program;

