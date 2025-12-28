/// Language Server Protocol implementation for IDE support.

pub mod server;
pub mod compiler_state;
mod text_utils;
mod hover;
mod diagnostics;
mod semantic_tokens;
mod completion;

pub use server::CantaLoopLSPServer;

