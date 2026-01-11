//! Tower-LSP server implementation.
//!
//! This module contains the main LSP server struct and initialization logic.

use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use tower_lsp::lsp_types::*;

use crate::core::source_manager::SourceManager;
use crate::core::compiler_state::CompilerState;


/// CantaLoop LSP server.
///
/// This is a thin protocol adapter over the compiler session.
/// All language logic lives in the compiler.
pub struct CantaLoopServer {
    pub(crate) client: Client,
    /// Source file manager
    pub(crate) source_manager: Arc<RwLock<SourceManager>>,
    /// Compiler state manager
    pub(crate) compiler_state: Arc<CompilerState>,
}

impl CantaLoopServer {
    pub fn new(client: Client) -> Self {
        let source_manager = Arc::new(RwLock::new(SourceManager::new()));
        let compiler_state = Arc::new(CompilerState::new(source_manager.clone()));
        
        Self {
            client,
            source_manager,
            compiler_state,
        }
    }

}

#[tower_lsp::async_trait]
impl LanguageServer for CantaLoopServer {
    async fn initialize(&self, params: InitializeParams) -> tower_lsp::jsonrpc::Result<InitializeResult> {
        // CRITICAL: Clear all caches on initialize
        // VSCode can reconnect without restarting the process, so we must treat this as a fresh world
        self.compiler_state.clear_all_caches().await;
        
        // Extract and check workspace folders
        let workspace_folders = params.workspace_folders.as_ref()
            .map(|folders| folders.iter().map(|f| f.uri.to_string()).collect::<Vec<_>>());
        
        // Update workspace folders and clear caches if they changed
        let folders_changed = self.compiler_state.update_workspace_folders(workspace_folders).await;
        
        if folders_changed {
            self.client.log_message(
                MessageType::INFO,
                "Workspace folders changed - all caches cleared".to_string(),
            ).await;
        }
        
        crate::lsp::handlers::initialize::handle_initialize(params)
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client.log_message(MessageType::INFO, "CantaLoop LSP initialized").await;
        self.client.log_message(MessageType::INFO, "Waiting for files to open...").await;
    }

    async fn shutdown(&self) -> tower_lsp::jsonrpc::Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        // CRITICAL: Panic hook in main() will catch panics and log them
        // Handlers should handle errors gracefully - panics will be logged but won't exit the server
        // Note: For async functions, catch_unwind doesn't work, so we rely on the panic hook
        crate::lsp::handlers::document::handle_did_open(self, params).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        // CRITICAL: Panic hook in main() will catch panics and log them
        // Handlers should handle errors gracefully - panics will be logged but won't exit the server
        crate::lsp::handlers::document::handle_did_change(self, params).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        // CRITICAL: Panic hook in main() will catch panics and log them
        // Handlers should handle errors gracefully - panics will be logged but won't exit the server
        crate::lsp::handlers::document::handle_did_close(self, params).await;
    }

    async fn hover(&self, params: HoverParams) -> tower_lsp::jsonrpc::Result<Option<Hover>> {
        crate::lsp::handlers::hover::handle_hover(self, params).await
    }

    async fn goto_definition(&self, params: GotoDefinitionParams) -> tower_lsp::jsonrpc::Result<Option<GotoDefinitionResponse>> {
        crate::lsp::handlers::goto::handle_goto_definition(self, params).await
    }

    async fn references(&self, params: ReferenceParams) -> tower_lsp::jsonrpc::Result<Option<Vec<Location>>> {
        crate::lsp::handlers::goto::handle_references(self, params).await
    }

    async fn semantic_tokens_full(&self, params: SemanticTokensParams) -> tower_lsp::jsonrpc::Result<Option<SemanticTokensResult>> {
        crate::lsp::handlers::tokens::handle_semantic_tokens_full(self, params).await
    }
}
