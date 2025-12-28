// LSP server binary entry point
use CantaLoopRS::lsp::CantaLoopLSPServer;
use tower_lsp::{LspService, Server};

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| CantaLoopLSPServer::new(client));
    Server::new(stdin, stdout, socket).serve(service).await;
}
