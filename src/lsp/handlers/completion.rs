//! Completion handler.

use tower_lsp::lsp_types::*;

use crate::lsp::server::CantaLoopServer;
use crate::lsp::mapping::spans::position_to_byte_offset;
use crate::core::hir_lowering::{SymbolKind, ValueKind};

/// Handle textDocument/completion.
pub async fn handle_completion(
    server: &CantaLoopServer,
    params: CompletionParams,
) -> tower_lsp::jsonrpc::Result<Option<CompletionResponse>> {
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
    let snapshot = match server.compiler_state.get_snapshot_for_file(file_id).await {
        Some(s) => s,
        None => return Ok(Some(CompletionResponse::Array(vec![]))),
    };

    // Check if CST exists (file parsed successfully)
    if snapshot.cst(file_id).is_none() {
        return Ok(Some(CompletionResponse::Array(vec![])));
    }

    // Get the word at the cursor position (for filtering)
    let word_at_cursor = get_word_at_offset(&source_text, byte_offset);
    
    // Build completion items from available symbols
    let mut items = Vec::new();

    // Add symbols from the symbol table
    if let Some(symbols) = snapshot.symbol_table() {
        for (symbol_id, info) in &symbols.symbol_info {
            // Filter by word prefix if user is typing
            if let Some(word) = &word_at_cursor {
                if !info.name.starts_with(word) {
                    continue;
                }
            }

            let kind = match info.kind {
                SymbolKind::Function => CompletionItemKind::FUNCTION,
                SymbolKind::Variable => CompletionItemKind::VARIABLE,
                SymbolKind::Parameter => CompletionItemKind::VARIABLE,
                SymbolKind::Field => CompletionItemKind::FIELD,
                SymbolKind::Module => CompletionItemKind::MODULE,
                SymbolKind::Type => CompletionItemKind::CLASS,
            };

            let detail = format_type(&info.ty);
            let documentation = build_completion_documentation(symbol_id, &snapshot);

            items.push(CompletionItem {
                label: info.name.clone(),
                kind: Some(kind),
                detail: Some(detail),
                documentation: documentation.map(|d| Documentation::String(d)),
                deprecated: None,
                preselect: None,
                sort_text: Some(format!("{:04}", items.len())),
                filter_text: Some(info.name.clone()),
                insert_text: Some(info.name.clone()),
                insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                insert_text_mode: None,
                text_edit: None,
                additional_text_edits: None,
                commit_characters: None,
                command: None,
                data: None,
                tags: None,
                label_details: None,
            });
        }
    }

    // Add keywords
    let keywords = vec![
        "fn", "let", "const", "pub", "struct", "if", "else", "match",
        "loop", "while", "for", "in", "break", "continue", "return",
        "use", "from", "mod", "pure", "effect", "true", "false",
    ];

    for keyword in keywords {
        if let Some(word) = &word_at_cursor {
            if !keyword.starts_with(word) {
                continue;
            }
        }

        items.push(CompletionItem {
            label: keyword.to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: None,
            documentation: None,
            deprecated: None,
            preselect: None,
            sort_text: Some(format!("k{:04}", items.len())),
            filter_text: Some(keyword.to_string()),
            insert_text: Some(keyword.to_string()),
            insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
            insert_text_mode: None,
            text_edit: None,
            additional_text_edits: None,
            commit_characters: None,
            command: None,
            data: None,
            tags: None,
            label_details: None,
        });
    }

    Ok(Some(CompletionResponse::Array(items)))
}

/// Get the word at the given byte offset (for filtering completions).
fn get_word_at_offset(source: &str, offset: usize) -> Option<String> {
    if offset > source.len() {
        return None;
    }

    // Find the start of the word
    let mut start = offset;
    while start > 0 {
        let ch = source.as_bytes()[start - 1] as char;
        if ch.is_alphanumeric() || ch == '_' {
            start -= 1;
        } else {
            break;
        }
    }

    // Find the end of the word
    let mut end = offset;
    while end < source.len() {
        let ch = source.as_bytes()[end] as char;
        if ch.is_alphanumeric() || ch == '_' {
            end += 1;
        } else {
            break;
        }
    }

    if start < end {
        Some(source[start..end].to_string())
    } else {
        None
    }
}

/// Format a type for display in completion details.
fn format_type(ty: &ValueKind) -> String {
    match ty {
        ValueKind::Number => "num".to_string(),
        ValueKind::String => "string".to_string(),
        ValueKind::Boolean => "bool".to_string(),
        ValueKind::Any => "any".to_string(),
        ValueKind::Unknown => "unknown".to_string(),
        ValueKind::Void => "void".to_string(),
        ValueKind::TypeVar(id) => format!("T{}", id),
        ValueKind::Function(sig) => sig.clone(),
        ValueKind::Thunk(sig) => sig.clone(),
        ValueKind::FnSig { params, return_type, is_effectful } => {
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
        ValueKind::ThunkSig { params, return_type, is_effectful } => {
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
        ValueKind::Callable => "callable".to_string(),
        ValueKind::Array(inner) => format!("{}[]", format_type(inner)),
        ValueKind::Struct(name) => name.clone(),
    }
}

/// Build documentation for a completion item.
fn build_completion_documentation(
    symbol_id: &crate::core::hir_lowering::SymbolId,
    snapshot: &crate::core::lsp_api::CompilerSnapshot,
) -> Option<String> {
    let info = snapshot.symbol_info(*symbol_id)?;
    let hir = snapshot.hir()?;

    match &info.kind {
        SymbolKind::Function => {
            if let Some(entity_id) = info.entity_id {
                if let Some(func) = hir.functions.get(&entity_id) {
                    let arrow = if func.signature.is_effectful { "~>" } else { "->" };
                    let params_str = if func.signature.params.is_empty() {
                        "()".to_string()
                    } else {
                        format!("({})", func.signature.params.iter()
                            .map(|p| format_type(p))
                            .collect::<Vec<_>>()
                            .join(", "))
                    };
                    let return_str = format_type(&func.signature.return_type);
                    
                    let mut doc = format!("```cantaloop\nfn {}{} {} {}\n```", 
                        info.name, params_str, arrow, return_str);
                    
                    if func.signature.is_effectful {
                        doc.push_str("\n\n*Effectful function* — requires execution marker (`!`)");
                    } else {
                        doc.push_str("\n\n*Pure function* — no side effects");
                    }
                    
                    return Some(doc);
                }
            }
        }
        _ => {}
    }

    Some(format!("**{}** `{}`\n\n```cantaloop\n{}\n```", 
        match info.kind {
            SymbolKind::Function => "function",
            SymbolKind::Variable => "variable",
            SymbolKind::Parameter => "parameter",
            SymbolKind::Field => "field",
            SymbolKind::Module => "module",
            SymbolKind::Type => "type",
        },
        info.name,
        format_type(&info.ty)
    ))
}
