use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind, CompletionResponse, Documentation};

use crate::core::hir_lowering::CompilerState;
use super::hover;

/// Generate completion items for a given position in the document.
pub fn generate_completions(
    text: &str,
    line: usize,
    char_pos: usize,
    state: Option<&CompilerState>,
) -> CompletionResponse {
    let lines: Vec<&str> = text.lines().collect();
    if line >= lines.len() {
        return CompletionResponse::Array(Vec::new());
    }

    let line_text = &lines[line];
    let prefix = if char_pos <= line_text.len() {
        &line_text[..char_pos]
    } else {
        line_text
    };

    let mut items = Vec::new();

    // Add built-in functions (filter by prefix)
    if prefix.ends_with("pr") || prefix.ends_with("print") || prefix.is_empty() {
        items.push(CompletionItem {
            label: "print".to_string(),
            kind: Some(CompletionItemKind::FUNCTION),
            detail: Some("fn print(str: String) -> String".to_string()),
            documentation: Some(Documentation::String("Prints a string to the console.".to_string())),
            ..Default::default()
        });
    }

    // Add keywords (filter by prefix)
    let keywords = vec!["fn", "if", "else", "return", "let", "true", "false", "loop", "while", "for", "in", "break", "continue", "use", "mod", "pub", "const"];
    for keyword in keywords {
        if prefix.is_empty() || keyword.starts_with(prefix.trim()) {
            items.push(CompletionItem {
                label: keyword.to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                ..Default::default()
            });
        }
    }

    // Add type names when user is typing a type annotation
    let trimmed_prefix = prefix.trim();
    let is_after_colon = trimmed_prefix.ends_with(':') || trimmed_prefix.ends_with(": ");
    let is_in_let = trimmed_prefix.contains("let ") && (trimmed_prefix.contains(':') || is_after_colon);
    let is_in_function_arg = trimmed_prefix.contains('(') && (trimmed_prefix.contains(':') || is_after_colon);
    
    if is_after_colon || is_in_let || is_in_function_arg {
        let type_names = ["num", "number", "string", "str", "boolean", "bool", "void"];
        for type_name in &type_names {
            if prefix.is_empty() || type_name.starts_with(prefix.trim()) || is_after_colon {
                items.push(CompletionItem {
                    label: type_name.to_string(),
                    kind: Some(CompletionItemKind::TYPE_PARAMETER),
                    detail: Some(match *type_name {
                        "num" | "number" => "Number type".to_string(),
                        "string" | "str" => "String type".to_string(),
                        "boolean" | "bool" => "Boolean type".to_string(),
                        "void" => "Void type (no return value)".to_string(),
                        _ => "Type".to_string(),
                    }),
                    ..Default::default()
                });
            }
        }
        
        // Check if user is typing a type annotation that might need -> or ~>
        let trimmed_prefix = prefix.trim();
        let last_word = trimmed_prefix.split_whitespace().last().unwrap_or("");
        if type_names.iter().any(|&tn| last_word == tn) && trimmed_prefix.ends_with(last_word) {
            // Suggest function type syntax
            items.push(CompletionItem {
                label: format!("{} -> {}", last_word, last_word),
                kind: Some(CompletionItemKind::TYPE_PARAMETER),
                detail: Some("Function type".to_string()),
                insert_text: Some(format!("{} -> {}", last_word, last_word)),
                ..Default::default()
            });
            // Suggest thunk type syntax
            items.push(CompletionItem {
                label: format!("{} ~> {}", last_word, last_word),
                kind: Some(CompletionItemKind::TYPE_PARAMETER),
                detail: Some("Thunk type (prepared call)".to_string()),
                insert_text: Some(format!("{} ~> {}", last_word, last_word)),
                ..Default::default()
            });
        }
    }

    // Add variables and functions from compiler state symbol table
    if let Some(state) = state {
        // Use symbol table - single source of truth
        for symbol in state.symbols.get_all() {
            if prefix.is_empty() || symbol.name.starts_with(prefix.trim()) {
                let type_str = hover::format_value_kind(&symbol.ty);
                let kind = match symbol.kind {
                    crate::core::hir_lowering::SymbolKind::Function => CompletionItemKind::FUNCTION,
                    crate::core::hir_lowering::SymbolKind::Variable | 
                    crate::core::hir_lowering::SymbolKind::Parameter => CompletionItemKind::VARIABLE,
                    crate::core::hir_lowering::SymbolKind::Module => CompletionItemKind::MODULE,
                };
                items.push(CompletionItem {
                    label: symbol.name.clone(),
                    kind: Some(kind),
                    detail: Some(format!("Type: {}", type_str)),
                    ..Default::default()
                });
            }
        }
    }

    CompletionResponse::Array(items)
}

