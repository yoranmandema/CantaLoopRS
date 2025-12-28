/// Language Server Protocol implementation for CantaLoop.
/// 
/// Provides IDE features like:
/// - Syntax error diagnostics
/// - Hover information
/// - Code completion
/// - Type information

pub mod server;

pub use server::CantaLoopLSPServer;

