//! LSP request handlers.
//!
//! Each handler is a thin adapter that:
//! 1. Converts LSP types to compiler types
//! 2. Queries compiler state
//! 3. Converts compiler results back to LSP types

pub mod initialize;
pub mod document;
pub mod diagnostics;
pub mod hover;
pub mod goto;
pub mod tokens;
pub mod completion;
pub mod document_symbol;
pub mod code_action;
pub mod formatting;

