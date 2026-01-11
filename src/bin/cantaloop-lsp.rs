//! CantaLoop Language Server Protocol implementation.

use tower_lsp::{LspService, Server};
use cantaloop::lsp::server;

#[tokio::main]
async fn main() {
    // Install panic hook to log panics before crashing
    std::panic::set_hook(Box::new(|panic_info| {
        eprintln!("LSP PANIC: {:?}", panic_info);
        eprintln!("Location: {:?}", panic_info.location());
        if let Some(payload) = panic_info.payload().downcast_ref::<&str>() {
            eprintln!("Payload: {}", payload);
        } else if let Some(payload) = panic_info.payload().downcast_ref::<String>() {
            eprintln!("Payload: {}", payload);
        }
    }));
    
    let pid = std::process::id();
    eprintln!("CantaLoop LSP started, pid={}", pid);
    
    // Initialize logger if not already initialized
    let _ = env_logger::try_init();
    
    // Run server with error handling
    let (service, socket) = LspService::new(|client| server::CantaLoopServer::new(client));
    
    // Server::serve() runs until the stream closes (client disconnects)
    // This will exit when VS Code closes the connection
    Server::new(tokio::io::stdin(), tokio::io::stdout(), socket)
        .serve(service)
        .await;
    
    eprintln!("LSP server shutting down");
}
