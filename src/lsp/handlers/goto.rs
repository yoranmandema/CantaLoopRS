//! Go-to definition and references handlers.

use tower_lsp::lsp_types::*;

use crate::lsp::server::CantaLoopServer;
use crate::lsp::mapping::spans::{LineIndex, position_to_byte_offset};

/// Handle textDocument/definition.
pub async fn handle_goto_definition(
    server: &CantaLoopServer,
    params: GotoDefinitionParams,
) -> tower_lsp::jsonrpc::Result<Option<GotoDefinitionResponse>> {
    let uri = &params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    // Get source text and file ID
    let (file_id, source_text) = {
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

    // Convert LSP position to byte offset
    let byte_offset = position_to_byte_offset(position, &source_text);

    // Get compiler snapshot
    let snapshot = match server.compiler_state.get_snapshot().await {
        Some(s) => s,
        None => return Ok(None),
    };

    // Try to find symbol at this position
    // Phase 3: Use CST identity-based lookup (Span → CstId → SymbolId)
    let symbol_id = snapshot
        .symbols_at_offset(file_id, byte_offset)
        .next()
        .map(|(_, symbol_id)| symbol_id);

    // Get definition span for the symbol
    let def_span = match symbol_id {
        Some(sym_id) => snapshot.definition_span_for_symbol(sym_id),
        None => None,
    };

    // Convert definition span to LSP location
    match def_span {
        Some(span) => {
            let line_index = LineIndex::new(&source_text);
            let range = line_index.hir_span_to_range(span);
            
            // Get the URI for the definition (for now, assume same file)
            let def_uri = {
                let source_manager = server.source_manager.read().await;
                source_manager.get_uri(file_id).cloned()
            };
            
            match def_uri {
                Some(uri) => Ok(Some(GotoDefinitionResponse::Scalar(Location {
                    uri,
                    range,
                }))),
                None => Ok(None),
            }
        }
        None => Ok(None),
    }
}

/// Handle textDocument/references.
pub async fn handle_references(
    server: &CantaLoopServer,
    params: ReferenceParams,
) -> tower_lsp::jsonrpc::Result<Option<Vec<Location>>> {
    let uri = &params.text_document_position.text_document.uri;
    let position = params.text_document_position.position;

    // Get source text and file ID
    let (file_id, source_text) = {
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

    // Convert LSP position to byte offset
    let byte_offset = position_to_byte_offset(position, &source_text);

    // Get compiler snapshot
    let snapshot = match server.compiler_state.get_snapshot().await {
        Some(s) => s,
        None => return Ok(None),
    };

    // Find symbol at this position
    // Phase 3: Use CST identity-based lookup (Span → CstId → SymbolId)
    let symbol_id = snapshot
        .symbols_at_offset(file_id, byte_offset)
        .next()
        .map(|(_, symbol_id)| symbol_id);

    // Get all reference spans for the symbol
    let reference_spans = match symbol_id {
        Some(sym_id) => snapshot.spans_for_symbol(sym_id),
        None => None,
    };

    // Convert spans to LSP locations
    match reference_spans {
        Some(spans) => {
            let line_index = LineIndex::new(&source_text);
            let mut locations = Vec::new();
            
            // For now, all references are in the same file
            // In the future, this should handle cross-file references
            let file_uri = {
                let source_manager = server.source_manager.read().await;
                source_manager.get_uri(file_id).cloned()
            };
            
            if let Some(uri) = file_uri {
                for span in spans {
                    let range = line_index.hir_span_to_range(*span);
                    locations.push(Location {
                        uri: uri.clone(),
                        range,
                    });
                }
            }
            
            Ok(Some(locations))
        }
        None => Ok(None),
    }
}
