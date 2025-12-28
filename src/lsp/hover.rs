use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Range};

use crate::core::hir_lowering::ValueKind;

/// Format a ValueKind as a string for display.
pub fn format_value_kind(kind: &ValueKind) -> String {
    match kind {
        ValueKind::Number => "Number".to_string(),
        ValueKind::String => "String".to_string(),
        ValueKind::Boolean => "Boolean".to_string(),
        ValueKind::Unknown => "Unknown".to_string(),
        ValueKind::Function(ty) => ty.clone(),
        ValueKind::Thunk(ty) => ty.clone(),
        ValueKind::Void => "Void".to_string(),
    }
}

/// Format a function signature as a string.
pub fn format_function_signature(func: &crate::core::hir_lowering::Function) -> String {
    let params: Vec<String> = func.signature.params.iter()
        .map(|p| format_value_kind(p))
        .collect();
    let return_type = format_value_kind(&func.signature.return_type);
    format!("fn {}({}) -> {}", func.name, params.join(", "), return_type)
}

/// Create hover content from markdown string and range.
pub fn create_hover_content(markdown: String, range: Range) -> Hover {
    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: markdown,
        }),
        range: Some(range),
    }
}

