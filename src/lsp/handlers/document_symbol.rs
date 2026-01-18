//! Document symbols handler.

use tower_lsp::lsp_types::*;

use crate::lsp::server::CantaLoopServer;
use crate::lsp::mapping::spans::LineIndex;

/// Handle textDocument/documentSymbol.
pub async fn handle_document_symbol(
    server: &CantaLoopServer,
    params: DocumentSymbolParams,
) -> tower_lsp::jsonrpc::Result<Option<DocumentSymbolResponse>> {
    let uri = &params.text_document.uri;

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

    // Get compiler snapshot
    let snapshot = match server.compiler_state.get_snapshot_for_file(file_id).await {
        Some(s) => s,
        None => return Ok(None),
    };

    // Check if CST exists
    if snapshot.cst(file_id).is_none() {
        return Ok(None);
    }

    let line_index = LineIndex::new(&source_text);
    let mut symbols = Vec::new();

    // Build symbols from symbol table
    if let Some(symbol_table) = snapshot.symbol_table() {
        // Group symbols by kind and collect definitions
        let mut definitions: Vec<_> = symbol_table.symbol_to_definition.iter().collect();
        definitions.sort_by_key(|(_, span)| span.start);

        for (symbol_id, def_span) in definitions {
            if let Some(info) = snapshot.symbol_info(*symbol_id) {
                // Only include user-defined symbols in the outline
                if info.stability != crate::core::lsp_api::SymbolStability::UserDefined {
                    continue;
                }

                let kind = match info.kind {
                    crate::core::hir_lowering::SymbolKind::Function => SymbolKind::FUNCTION,
                    crate::core::hir_lowering::SymbolKind::Variable => SymbolKind::VARIABLE,
                    crate::core::hir_lowering::SymbolKind::Parameter => SymbolKind::VARIABLE,
                    crate::core::hir_lowering::SymbolKind::Field => SymbolKind::FIELD,
                    crate::core::hir_lowering::SymbolKind::Module => SymbolKind::MODULE,
                    crate::core::hir_lowering::SymbolKind::Type => SymbolKind::CLASS,
                };

                let range = line_index.hir_span_to_range(*def_span);
                let selection_range = range; // For now, use the same range

                // `DocumentSymbol::deprecated` is deprecated in upstream lsp-types in favor of tags.
                // We keep it as `None` for older clients and silence the deprecation warning.
                #[allow(deprecated)]
                symbols.push(DocumentSymbol {
                    name: info.name.clone(),
                    detail: Some(format_type(&info.ty)),
                    kind,
                    tags: None,
                    deprecated: None,
                    range,
                    selection_range,
                    children: None,
                });
            }
        }
    }

    // Sort symbols by position
    symbols.sort_by_key(|s| s.range.start);

    Ok(Some(DocumentSymbolResponse::Nested(symbols)))
}

/// Format a type for display in symbol details.
fn format_type(ty: &crate::core::hir_lowering::ValueKind) -> String {
    match ty {
        crate::core::hir_lowering::ValueKind::Number => "num".to_string(),
        crate::core::hir_lowering::ValueKind::String => "string".to_string(),
        crate::core::hir_lowering::ValueKind::Boolean => "bool".to_string(),
        crate::core::hir_lowering::ValueKind::Any => "any".to_string(),
        crate::core::hir_lowering::ValueKind::Unknown => "unknown".to_string(),
        crate::core::hir_lowering::ValueKind::Void => "void".to_string(),
        crate::core::hir_lowering::ValueKind::TypeVar(id) => format!("T{}", id),
        crate::core::hir_lowering::ValueKind::Function(sig) => sig.clone(),
        crate::core::hir_lowering::ValueKind::Thunk(sig) => sig.clone(),
        crate::core::hir_lowering::ValueKind::FnSig { params, return_type, is_effectful } => {
            let p: Vec<String> = params.iter().map(format_type).collect();
            let param_str = if p.is_empty() {
                "()".to_string()
            } else if p.len() == 1 {
                p[0].clone()
            } else {
                format!("({})", p.join(","))
            };
            let arrow = if *is_effectful { "~>" } else { "->" };
            format!("{} {} {}", param_str, arrow, format_type(return_type))
        }
        crate::core::hir_lowering::ValueKind::ThunkSig { params, return_type, is_effectful } => {
            let p: Vec<String> = params.iter().map(format_type).collect();
            let param_str = if p.is_empty() {
                "()".to_string()
            } else if p.len() == 1 {
                p[0].clone()
            } else {
                format!("({})", p.join(","))
            };
            let arrow = if *is_effectful { "~>" } else { "->" };
            format!("{} {} {}", param_str, arrow, format_type(return_type))
        }
        crate::core::hir_lowering::ValueKind::Callable => "callable".to_string(),
        crate::core::hir_lowering::ValueKind::Array(inner) => format!("{}[]", format_type(inner)),
        crate::core::hir_lowering::ValueKind::Struct(name) => name.clone(),
    }
}
