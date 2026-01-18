//! Initialize handler.

use tower_lsp::lsp_types::*;

/// Handle initialize request.
pub fn handle_initialize(_params: InitializeParams) -> tower_lsp::jsonrpc::Result<InitializeResult> {
    Ok(InitializeResult {
        server_info: Some(ServerInfo {
            name: "cantaloop-lsp".to_string(),
            version: Some("0.1.0".to_string()),
        }),
        capabilities: ServerCapabilities {
            text_document_sync: Some(TextDocumentSyncCapability::Options(
                TextDocumentSyncOptions {
                    open_close: Some(true),
                    change: Some(TextDocumentSyncKind::INCREMENTAL),
                    will_save: None,
                    will_save_wait_until: None,
                    save: None,
                },
            )),
            hover_provider: Some(HoverProviderCapability::Simple(true)),
            definition_provider: Some(OneOf::Left(true)),
            references_provider: Some(OneOf::Left(true)),
            semantic_tokens_provider: Some(
                SemanticTokensServerCapabilities::SemanticTokensOptions(
                    SemanticTokensOptions {
                        work_done_progress_options: WorkDoneProgressOptions {
                            work_done_progress: None,
                        },
                        legend: SemanticTokensLegend {
                            token_types: vec![
                                SemanticTokenType::FUNCTION,
                                SemanticTokenType::VARIABLE,
                                SemanticTokenType::PARAMETER,
                                SemanticTokenType::KEYWORD,
                                SemanticTokenType::OPERATOR,
                                SemanticTokenType::STRING,
                                SemanticTokenType::NUMBER,
                                SemanticTokenType::COMMENT,
                                SemanticTokenType::TYPE,
                                // Note: CantaLoop-specific tokens (effect, execution_marker) 
                                // would need to be registered via client/registerCapability
                                // For now, we use standard types
                            ],
                            token_modifiers: vec![],
                        },
                        range: None, // Range requests not implemented yet
                        full: Some(SemanticTokensFullOptions::Bool(true)),
                    },
                ),
            ),
            workspace: Some(WorkspaceServerCapabilities {
                workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                    supported: Some(true),
                    change_notifications: Some(OneOf::Left(true)),
                }),
                file_operations: None,
            }),
            completion_provider: Some(CompletionOptions {
                trigger_characters: Some(vec![".".to_string(), ":".to_string()]),
                all_commit_characters: None,
                resolve_provider: Some(false),
                work_done_progress_options: WorkDoneProgressOptions {
                    work_done_progress: None,
                },
                completion_item: None,
            }),
            document_symbol_provider: Some(OneOf::Left(true)),
            code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
            document_formatting_provider: Some(OneOf::Left(true)),
            document_range_formatting_provider: Some(OneOf::Left(true)),
            ..Default::default()
        },
    })
}
