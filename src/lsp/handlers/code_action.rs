//! Code action handler.

use tower_lsp::lsp_types::*;

use crate::lsp::server::CantaLoopServer;

/// Handle textDocument/codeAction.
pub async fn handle_code_action(
    server: &CantaLoopServer,
    params: CodeActionParams,
) -> tower_lsp::jsonrpc::Result<Option<CodeActionResponse>> {
    let uri = &params.text_document.uri;

    // Get source text and file ID
    let (file_id, _source_text) = {
        let source_manager = server.source_manager.read().await;
        let file_id = match source_manager.get_file_id(uri) {
            Some(id) => id,
            None => return Ok(None),
        };
        let text = source_manager.get_file_text(file_id)
            .unwrap_or("")
            .to_string();
        (file_id, text)
    };

    // Get compiler snapshot
    let snapshot = match server.compiler_state.get_snapshot_for_file(file_id).await {
        Some(s) => s,
        None => return Ok(Some(vec![])),
    };

    // Get diagnostics for this file
    let _diagnostics = snapshot.diagnostics(file_id);
    let actions = Vec::new();

    // For now, we'll add basic code actions based on diagnostics
    // In the future, diagnostics should include Fix suggestions from the compiler
    // TODO: Implement compiler-generated code actions

    Ok(Some(actions))
}
