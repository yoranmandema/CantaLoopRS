/// Language Server Protocol implementation for CantaLoop.
/// 
/// Provides IDE features like:
/// - Syntax error diagnostics
/// - Hover information
/// - Code completion
/// - Type information

#[path = "lsp_server.rs"]
pub mod server;

pub use server::CantaLoopLSPServer;

