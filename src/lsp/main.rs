//! CantaLoop Language Server Protocol implementation.
//!
//! Tower-LSP server implementation.

use tower_lsp::{LspService, Server};

mod handlers;
mod mapping;
mod server;

#[tokio::main]
async fn main() {
    // CRITICAL: Install panic hook to prevent server from exiting on panic
    // Tower-LSP will exit if a panic escapes, causing VS Code to respawn the server
    // This creates a "double-spawn / dead pipe" where VS Code writes to the old pipe
    std::panic::set_hook(Box::new(|panic_info| {
        eprintln!("[LSP PANIC] {}", panic_info);
        // Log panic location if available
        if let Some(location) = panic_info.location() {
            eprintln!("[LSP PANIC] at {}:{}:{}", location.file(), location.line(), location.column());
        }
        // Log panic payload if available
        if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            eprintln!("[LSP PANIC] message: {}", s);
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            eprintln!("[LSP PANIC] message: {}", s);
        }
    }));
    
    eprintln!("[LSP] CantaLoop LSP booting...");
    
    let (service, socket) = LspService::new(|client| server::CantaLoopServer::new(client));
    
    // CRITICAL: main() must NEVER return
    // If main() returns, Tower-LSP exits, VS Code respawns, and we get a dead pipe
    // This is the #1 mistake in Tower-LSP - you MUST never have code after serve().await
    Server::new(tokio::io::stdin(), tokio::io::stdout(), socket)
        .serve(service)
        .await;
    
    // NEVER REACHED: If you see "[LSP ERROR] CantaLoop LSP exited", the server crashed
    // This log is intentionally placed to detect if main() returns unexpectedly
    eprintln!("[LSP ERROR] CantaLoop LSP exited unexpectedly - this should never happen!");
}
