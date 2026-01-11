//! Hover handler.

use tower_lsp::lsp_types::*;

use crate::lsp::server::CantaLoopServer;
use crate::lsp::mapping::spans::{LineIndex, position_to_byte_offset};
use crate::core::hir_lowering::SymbolId;

/// Handle textDocument/hover.
pub async fn handle_hover(
    server: &CantaLoopServer,
    params: HoverParams,
) -> tower_lsp::jsonrpc::Result<Option<Hover>> {
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

    // Get compiler snapshot for this file
    // Use last good snapshot if current compilation failed
    let snapshot = match server.compiler_state.get_snapshot_for_file(file_id).await {
        Some(s) => s,
        None => return Ok(None),
    };
    
    // CRITICAL: Only provide hover if file parsed successfully
    // Check if CST exists for this file - if not, file failed to parse
    if snapshot.cst(file_id).is_none() {
        // File failed to parse - return None (no hover data available)
        return Ok(None);
    }

    // Find symbol at this position
    // Phase 3: Use CST identity-based lookup (Span → CstId → SymbolId)
    let symbols_at_pos: Vec<_> = snapshot.symbols_at_offset(file_id, byte_offset).collect();
    eprintln!("[HOVER] Found {} symbol(s) at byte {}:", symbols_at_pos.len(), byte_offset);
    for (span, symbol_id) in &symbols_at_pos {
        if let Some(info) = snapshot.symbol_info(*symbol_id) {
            eprintln!("[HOVER]   - span={}..{}, SymbolId({}), name='{}', kind={:?}", 
                span.start, span.end, symbol_id.0, info.name, info.kind);
        }
    }
    let symbol_id = symbols_at_pos.first().map(|(_, symbol_id)| {
        // Debug: log what symbol we found
        if let Some(info) = snapshot.symbol_info(*symbol_id) {
            eprintln!("[HOVER] Using first symbol: SymbolId({}), name='{}', kind={:?}", 
                symbol_id.0, info.name, info.kind);
        }
        *symbol_id
    });

    // Build hover content from symbol information
    let hover_content = match symbol_id {
        Some(sym_id) => {
            build_hover_content(sym_id, &snapshot)
        }
        None => None,
    };

    match hover_content {
        Some(content) => Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: content,
            }),
            range: None, // Could optionally provide range highlighting
        })),
        None => Ok(None),
    }
}

/// Build hover content from symbol information.
fn build_hover_content(symbol_id: SymbolId, snapshot: &crate::core::lsp_api::CompilerSnapshot) -> Option<String> {
    // Get symbol info from snapshot
    let info = snapshot.symbol_info(symbol_id)?;
    let hir = snapshot.hir()?;
    
    eprintln!("[HOVER] build_hover_content: SymbolId({}), name='{}', kind={:?}", 
        symbol_id.0, info.name, info.kind);
    
    // Build hover content based on symbol kind and type
    let mut parts = Vec::new();
    
    match &info.kind {
        crate::core::hir_lowering::SymbolKind::Function => {
            // For functions, try to get full signature from HIR
            // Search for function by name in HIR
            let mut found_func = None;
            for (func_id, func) in &hir.functions {
                if func.name == info.name {
                    eprintln!("[HOVER] Found matching function in HIR: func_id={:?}, name='{}'", func_id, func.name);
                    found_func = Some(func);
                    break; // Take first match (might be wrong if multiple functions with same name exist)
                }
            }
            
            if let Some(func) = found_func {
                    // Format function signature
                    let arrow = if func.signature.is_effectful { "~>" } else { "->" };
                    let params_str = if func.signature.params.is_empty() {
                        "()".to_string()
                    } else {
                        format!("({})", func.signature.params.iter()
                            .map(|p| format_value_kind(p))
                            .collect::<Vec<_>>()
                            .join(", "))
                    };
                    let return_str = format_value_kind(&func.signature.return_type);
                    
                    parts.push(format!("**function** `{}`", info.name));
                    parts.push(format!("```cantaloop\nfn {}{} {} {}\n```", 
                        info.name, params_str, arrow, return_str));
                    
                    // Add effect information
                    if func.signature.is_effectful {
                        parts.push("\n*Effectful function* — requires execution marker (`!`)".to_string());
                    } else {
                        parts.push("\n*Pure function* — no side effects".to_string());
                    }
                    
                    return Some(parts.join("\n\n"));
                } else {
                    eprintln!("[HOVER] WARNING: No matching function found in HIR for name '{}'", info.name);
                }
            
            // Fallback: use type info
            parts.push(format!("**function** `{}`", info.name));
            parts.push(format!("```cantaloop\n{}\n```", format_value_kind(&info.ty)));
        }
        crate::core::hir_lowering::SymbolKind::Variable => {
            parts.push(format!("**variable** `{}`", info.name));
            parts.push(format!("```cantaloop\n{}\n```", format_value_kind(&info.ty)));
        }
        crate::core::hir_lowering::SymbolKind::Parameter => {
            parts.push(format!("**parameter** `{}`", info.name));
            parts.push(format!("```cantaloop\n{}\n```", format_value_kind(&info.ty)));
        }
        crate::core::hir_lowering::SymbolKind::Module => {
            parts.push(format!("**module** `{}`", info.name));
        }
    }
    
    Some(parts.join("\n\n"))
}

/// Format ValueKind as a readable type string.
fn format_value_kind(kind: &crate::core::hir_lowering::ValueKind) -> String {
    match kind {
        crate::core::hir_lowering::ValueKind::Number => "num".to_string(),
        crate::core::hir_lowering::ValueKind::String => "string".to_string(),
        crate::core::hir_lowering::ValueKind::Boolean => "bool".to_string(),
        crate::core::hir_lowering::ValueKind::Any => "any".to_string(),
        crate::core::hir_lowering::ValueKind::Unknown => "unknown".to_string(),
        crate::core::hir_lowering::ValueKind::Void => "void".to_string(),
        crate::core::hir_lowering::ValueKind::Function(sig) => sig.clone(),
        crate::core::hir_lowering::ValueKind::Thunk(sig) => sig.clone(),
        crate::core::hir_lowering::ValueKind::Callable => "callable".to_string(),
        crate::core::hir_lowering::ValueKind::Array(inner) => {
            format!("{}[]", format_value_kind(inner))
        }
        crate::core::hir_lowering::ValueKind::Struct(name) => name.clone(),
    }
}
