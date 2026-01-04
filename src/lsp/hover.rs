use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Range};

use crate::core::hir_lowering::ValueKind;
use crate::core::ast::{Expression, Literal};

/// Format a ValueKind as a string for display.
pub fn format_value_kind(kind: &ValueKind) -> String {
    match kind {
        ValueKind::Any => "Any".to_string(),
        ValueKind::Number => "Number".to_string(),
        ValueKind::String => "String".to_string(),
        ValueKind::Boolean => "Boolean".to_string(),
        ValueKind::Unknown => "Unknown".to_string(),
        ValueKind::Function(ty) => ty.clone(),
        ValueKind::Thunk(ty) => ty.clone(),
        ValueKind::Void => "Void".to_string(),
        ValueKind::Struct(name) => name.clone(),
        ValueKind::Array(inner) => {
            let inner_str = format_value_kind(inner);
            format!("Array<{}>", inner_str)
        }
    }
}

/// Extract constant value from AST if the identifier is a constant.
/// Returns the formatted value string if found.
/// Also checks imported constants from HIR and evaluated constants in HIR.
pub fn find_constant_value(ast: &crate::core::ast::Program, hir: &crate::core::hir_lowering::HirAst, identifier: &str) -> Option<String> {
    // First check evaluated constants in HIR (these are constants that have been compiled)
    // Constants are stored with their name in ast.constants
    if let Some(constant) = hir.constants.iter().find(|c| c.name == identifier) {
        return Some(format_constant_value(&constant.value));
    }
    
    // Then check imported constants
    if let Some(value) = hir.imported_constant_values.get(identifier) {
        return Some(format_constant_value(value));
    }
    
    // Finally check local constants in AST (for constants that haven't been compiled yet)
    for block in &ast.blocks {
        for stmt in &block.statements {
            if let crate::core::ast::Statement::Const { identifier: const_name, expression, .. } = stmt {
                if const_name == identifier {
                    // Extract value from expression if it's a literal
                    return extract_literal_value(expression);
                }
            }
        }
    }
    
    None
}

/// Format a ConstantValue as a string for display.
fn format_constant_value(value: &crate::core::hir_lowering::ConstantValue) -> String {
    match value {
        crate::core::hir_lowering::ConstantValue::Number(n) => n.to_string(),
        crate::core::hir_lowering::ConstantValue::String(s) => format!("\"{}\"", s),
        crate::core::hir_lowering::ConstantValue::Boolean(b) => b.to_string(),
        crate::core::hir_lowering::ConstantValue::None => "None".to_string(),
    }
}

/// Extract literal value from an expression.
fn extract_literal_value(expr: &Expression) -> Option<String> {
    match expr {
        Expression::Literal(lit) => {
            Some(match lit {
                Literal::Number(n) => n.to_string(),
                Literal::String(s) => format!("\"{}\"", s),
                Literal::Boolean(b) => b.to_string(),
            })
        }
        _ => None, // Complex expressions - can't extract value statically
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

