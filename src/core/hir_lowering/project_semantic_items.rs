//! Editor projection: CompilerState → Semantic Items
//! 
//! This module projects semantic items (keywords, operators, identifiers, types)
//! from compiler state for LSP semantic tokenization.
//! 
//! This is a pure projection function - it does not perform analysis,
//! only projects what the compiler already knows into editor-friendly form.

use std::collections::HashMap;
use bitflags::bitflags;

use super::{HirAst, ValueKind, Span};

/// Unified semantic item kind for LSP tokenization.
/// Represents all tokenizable items from the compiler's perspective.
#[derive(Debug, Clone, PartialEq)]
pub enum SemanticItemKind {
    Function,
    Variable,
    Parameter,
    Keyword,
    Operator,
    Type,
    Module,
}

bitflags! {
    /// Semantic modifiers for LSP tokenization.
    /// These encode additional properties of semantic items (e.g., thunks, readonly).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SemanticModifiers: u32 {
        const THUNK = 1 << 0;
        const READONLY = 1 << 1;
    }
}

/// A semantic item representing a tokenizable unit in the source code.
/// All semantic items (keywords, operators, identifiers, types) flow from AST analysis.
#[derive(Debug, Clone)]
pub struct SemanticItem {
    pub kind: SemanticItemKind,
    pub span: Span,
    pub modifiers: SemanticModifiers,
}

impl SemanticItem {
    pub fn function(span: Span) -> Self {
        Self { kind: SemanticItemKind::Function, span, modifiers: SemanticModifiers::empty() }
    }
    
    pub fn variable(span: Span) -> Self {
        Self { kind: SemanticItemKind::Variable, span, modifiers: SemanticModifiers::empty() }
    }
    
    pub fn parameter(span: Span) -> Self {
        Self { kind: SemanticItemKind::Parameter, span, modifiers: SemanticModifiers::empty() }
    }
    
    pub fn keyword(span: Span) -> Self {
        Self { kind: SemanticItemKind::Keyword, span, modifiers: SemanticModifiers::empty() }
    }
    
    pub fn operator(span: Span) -> Self {
        Self { kind: SemanticItemKind::Operator, span, modifiers: SemanticModifiers::empty() }
    }
    
    pub fn r#type(span: Span) -> Self {
        Self { kind: SemanticItemKind::Type, span, modifiers: SemanticModifiers::empty() }
    }
    
    pub fn module(span: Span) -> Self {
        Self { kind: SemanticItemKind::Module, span, modifiers: SemanticModifiers::empty() }
    }
    
    /// Create a semantic item with modifiers.
    pub fn with_modifiers(mut self, modifiers: SemanticModifiers) -> Self {
        self.modifiers = modifiers;
        self
    }
}

/// Collect all semantic items (keywords, operators, identifiers, types) from AST.
/// This creates a unified view of all tokenizable items, eliminating the need for text scanning in LSP.
/// Also annotates items with modifiers (e.g., THUNK for thunk variables).
/// 
/// This is the single source of truth for LSP semantic tokenization.
pub fn collect_semantic_items(ast: &crate::core::ast::Program, source: &str, hir: &HirAst) -> Vec<SemanticItem> {
    let mut items = Vec::new();
    
    // Build a map of variable names to their types for thunk detection
    let mut var_types: HashMap<String, ValueKind> = HashMap::new();
    for scope in &hir.scopes.scopes {
        for var in &scope.vars {
            var_types.insert(var.name.clone(), var.kind.clone());
        }
    }
    
    // Helper to find keyword span in source text
    let find_keyword_span = |keyword: &str, after_pos: usize| -> Option<Span> {
        let keyword_bytes = keyword.as_bytes();
        for i in after_pos..source.len() {
            if i + keyword_bytes.len() <= source.len() {
                if &source.as_bytes()[i..i + keyword_bytes.len()] == keyword_bytes {
                    // Check word boundaries
                    let before_ok = i == 0 || {
                        let ch = source.as_bytes()[i - 1] as char;
                        !ch.is_alphanumeric() && ch != '_'
                    };
                    let after_ok = i + keyword_bytes.len() >= source.len() || {
                        let ch = source.as_bytes()[i + keyword_bytes.len()] as char;
                        !ch.is_alphanumeric() && ch != '_'
                    };
                    
                    if before_ok && after_ok {
                        // Check if it's in a comment
                        let line_start = source[..i].rfind('\n').map(|p| p + 1).unwrap_or(0);
                        let line_text = &source[line_start..];
                        let col_in_line = i - line_start;
                        if line_text[..col_in_line.min(line_text.len())].find("//").is_none() {
                            return Some(Span::new(i, i + keyword_bytes.len()));
                        }
                    }
                }
            }
        }
        None
    };
    
    // Helper to find operator span in source text
    let find_operator_span = |op: &str, after_pos: usize| -> Option<Span> {
        let op_bytes = op.as_bytes();
        for i in after_pos..source.len() {
            if i + op_bytes.len() <= source.len() {
                if &source.as_bytes()[i..i + op_bytes.len()] == op_bytes {
                    // Check if it's in a comment
                    let line_start = source[..i].rfind('\n').map(|p| p + 1).unwrap_or(0);
                    let line_text = &source[line_start..];
                    let col_in_line = i - line_start;
                    if line_text[..col_in_line.min(line_text.len())].find("//").is_none() {
                        return Some(Span::new(i, i + op_bytes.len()));
                    }
                }
            }
        }
        None
    };
    
    let mut current_pos = 0;
    
    // Walk AST to collect semantic items
    for block in &ast.blocks {
        for stmt in &block.statements {
            match stmt {
                crate::core::ast::Statement::FunctionDeclaration { identifier, arguments, return_type, .. } => {
                    // Keyword: fn
                    if let Some(span) = find_keyword_span("fn", current_pos) {
                        items.push(SemanticItem::keyword(span));
                        current_pos = span.end;
                    }
                    // Function name
                    if let Some(span) = find_identifier_span(identifier, source, current_pos) {
                        items.push(SemanticItem::function(span));
                        current_pos = span.end;
                    }
                    // Type annotations in arguments
                    for arg in arguments {
                        if let Some(span) = find_type_annotation_span(&arg.kind, source, current_pos) {
                            items.push(SemanticItem::r#type(span));
                        }
                    }
                    // Return type annotation
                    if let Some(return_type_str) = return_type {
                        if let Some(arrow_span) = find_operator_span("->", current_pos) {
                            items.push(SemanticItem::operator(arrow_span));
                            if let Some(span) = find_type_annotation_span(return_type_str, source, arrow_span.end) {
                                items.push(SemanticItem::r#type(span));
                            }
                        }
                    }
                }
                crate::core::ast::Statement::Let { identifier, type_annotation, .. } => {
                    // Keyword: let
                    if let Some(span) = find_keyword_span("let", current_pos) {
                        items.push(SemanticItem::keyword(span));
                        current_pos = span.end;
                    }
                    // Variable name - check if it's a thunk
                    if let Some(span) = find_identifier_span(identifier, source, current_pos) {
                        let mut item = SemanticItem::variable(span);
                        // Check if this variable is a thunk
                        if let Some(ty) = var_types.get(identifier) {
                            if matches!(ty, ValueKind::Thunk(_)) {
                                item = item.with_modifiers(SemanticModifiers::THUNK);
                            }
                        }
                        items.push(item);
                        current_pos = span.end;
                    }
                    // Type annotation
                    if let Some(type_ann) = type_annotation {
                        if let Some(colon_span) = find_operator_span(":", current_pos) {
                            if let Some(span) = find_type_annotation_span(type_ann, source, colon_span.end) {
                                items.push(SemanticItem::r#type(span));
                            }
                        }
                    }
                }
                crate::core::ast::Statement::Const { identifier, .. } => {
                    if let Some(span) = find_keyword_span("const", current_pos) {
                        items.push(SemanticItem::keyword(span));
                        current_pos = span.end;
                    }
                    if let Some(span) = find_identifier_span(identifier, source, current_pos) {
                        let mut item = SemanticItem::variable(span);
                        // Check if this constant is a thunk (readonly + thunk)
                        if let Some(ty) = var_types.get(identifier) {
                            let mut modifiers = SemanticModifiers::READONLY;
                            if matches!(ty, ValueKind::Thunk(_)) {
                                modifiers |= SemanticModifiers::THUNK;
                            }
                            item = item.with_modifiers(modifiers);
                        } else {
                            item = item.with_modifiers(SemanticModifiers::READONLY);
                        }
                        items.push(item);
                        current_pos = span.end;
                    }
                }
                crate::core::ast::Statement::If { .. } => {
                    if let Some(span) = find_keyword_span("if", current_pos) {
                        items.push(SemanticItem::keyword(span));
                        current_pos = span.end;
                    }
                }
                crate::core::ast::Statement::Match { .. } => {
                    if let Some(span) = find_keyword_span("match", current_pos) {
                        items.push(SemanticItem::keyword(span));
                        current_pos = span.end;
                    }
                }
                crate::core::ast::Statement::Return { .. } => {
                    if let Some(span) = find_keyword_span("return", current_pos) {
                        items.push(SemanticItem::keyword(span));
                        current_pos = span.end;
                    }
                }
                crate::core::ast::Statement::Loop { .. } => {
                    if let Some(span) = find_keyword_span("loop", current_pos) {
                        items.push(SemanticItem::keyword(span));
                        current_pos = span.end;
                    }
                }
                crate::core::ast::Statement::While { .. } => {
                    if let Some(span) = find_keyword_span("while", current_pos) {
                        items.push(SemanticItem::keyword(span));
                        current_pos = span.end;
                    }
                }
                crate::core::ast::Statement::For { var_name, .. } => {
                    if let Some(span) = find_keyword_span("for", current_pos) {
                        items.push(SemanticItem::keyword(span));
                        current_pos = span.end;
                    }
                    if let Some(span) = find_identifier_span(var_name, source, current_pos) {
                        items.push(SemanticItem::variable(span));
                        current_pos = span.end;
                    }
                }
                crate::core::ast::Statement::Break { .. } => {
                    if let Some(span) = find_keyword_span("break", current_pos) {
                        items.push(SemanticItem::keyword(span));
                        current_pos = span.end;
                    }
                }
                crate::core::ast::Statement::Continue => {
                    if let Some(span) = find_keyword_span("continue", current_pos) {
                        items.push(SemanticItem::keyword(span));
                        current_pos = span.end;
                    }
                }
                crate::core::ast::Statement::Use { .. } => {
                    if let Some(span) = find_keyword_span("use", current_pos) {
                        items.push(SemanticItem::keyword(span));
                        current_pos = span.end;
                    }
                }
                crate::core::ast::Statement::Mod { identifier } => {
                    if let Some(span) = find_keyword_span("mod", current_pos) {
                        items.push(SemanticItem::keyword(span));
                        current_pos = span.end;
                    }
                    if let Some(span) = find_identifier_span(identifier, source, current_pos) {
                        items.push(SemanticItem::module(span));
                        current_pos = span.end;
                    }
                }
                _ => {}
            }
            
            // Walk expressions to find operators and identifiers
            collect_expression_operators(stmt, source, &mut items, &mut current_pos, &var_types);
        }
    }
    
    items
}

/// Helper to find identifier span in source text.
fn find_identifier_span(identifier: &str, source: &str, start_from: usize) -> Option<Span> {
    let name_bytes = identifier.as_bytes();
    for i in start_from..source.len() {
        if i + name_bytes.len() <= source.len() {
            if &source.as_bytes()[i..i + name_bytes.len()] == name_bytes {
                let before_ok = i == 0 || {
                    let ch = source.as_bytes()[i - 1] as char;
                    !ch.is_alphanumeric() && ch != '_'
                };
                let after_ok = i + name_bytes.len() >= source.len() || {
                    let ch = source.as_bytes()[i + name_bytes.len()] as char;
                    !ch.is_alphanumeric() && ch != '_'
                };
                if before_ok && after_ok {
                    let line_start = source[..i].rfind('\n').map(|p| p + 1).unwrap_or(0);
                    let line_text = &source[line_start..];
                    let col_in_line = i - line_start;
                    if line_text[..col_in_line.min(line_text.len())].find("//").is_none() {
                        return Some(Span::new(i, i + name_bytes.len()));
                    }
                }
            }
        }
    }
    None
}

/// Helper to find type annotation span in source text.
fn find_type_annotation_span(type_str: &str, source: &str, start_from: usize) -> Option<Span> {
    // Find type keywords in the type string
    let type_keywords = ["num", "string", "bool", "void", "any"];
    for keyword in type_keywords {
        if type_str.contains(keyword) {
            if let Some(span) = find_identifier_span(keyword, source, start_from) {
                return Some(span);
            }
        }
    }
    None
}

/// Walk expressions to collect operators and identifiers.
fn collect_expression_operators(
    stmt: &crate::core::ast::Statement,
    source: &str,
    items: &mut Vec<SemanticItem>,
    current_pos: &mut usize,
    var_types: &HashMap<String, ValueKind>,
) {
    use crate::core::ast::{Expression, Statement, BinaryOp, UnaryOp};
    
    fn walk_expr(
        expr: &Expression,
        source: &str,
        items: &mut Vec<SemanticItem>,
        current_pos: &mut usize,
        var_types: &HashMap<String, ValueKind>,
    ) {
        match expr {
            Expression::Infix { op, lhs, rhs, .. } => {
                // Recurse into left side first
                walk_expr(lhs, source, items, current_pos, var_types);
                // Extract operator
                let op_str = match op {
                    BinaryOp::Add => "+",
                    BinaryOp::Sub => "-",
                    BinaryOp::Mul => "*",
                    BinaryOp::Div => "/",
                    BinaryOp::Mod => "%",
                    BinaryOp::Pow => "^",
                    BinaryOp::Eq => "==",
                    BinaryOp::Ne => "!=",
                    BinaryOp::Gt => ">",
                    BinaryOp::Lt => "<",
                    BinaryOp::Ge => ">=",
                    BinaryOp::Le => "<=",
                    BinaryOp::And => "&&",
                    BinaryOp::Or => "||",
                };
                if let Some(span) = find_op_span_in_expr(op_str, source, *current_pos) {
                    items.push(SemanticItem::operator(span));
                    *current_pos = span.end;
                }
                // Recurse into right side
                walk_expr(rhs, source, items, current_pos, var_types);
            }
            Expression::Prefix { op, rhs, .. } => {
                // Extract operator
                let op_str = match op {
                    UnaryOp::Neg => "-",
                    UnaryOp::Increment => "++",
                    UnaryOp::Decrement => "--",
                    UnaryOp::Not => "!",
                };
                if let Some(span) = find_op_span_in_expr(op_str, source, *current_pos) {
                    items.push(SemanticItem::operator(span));
                    *current_pos = span.end;
                }
                // Recurse into operand
                walk_expr(rhs, source, items, current_pos, var_types);
            }
            Expression::Postfix { op, lhs, .. } => {
                // Recurse into operand first
                walk_expr(lhs, source, items, current_pos, var_types);
                // Extract operator
                let op_str = match op {
                    crate::core::ast::PostfixOp::Invoke => "!",
                };
                if let Some(span) = find_op_span_in_expr(op_str, source, *current_pos) {
                    items.push(SemanticItem::operator(span));
                    *current_pos = span.end;
                }
            }
            Expression::Compose { reverse, lhs, rhs, .. } => {
                // Recurse into left side
                walk_expr(lhs, source, items, current_pos, var_types);
                // Extract operator
                let op_str = if *reverse { "<|" } else { "|>" };
                if let Some(span) = find_op_span_in_expr(op_str, source, *current_pos) {
                    items.push(SemanticItem::operator(span));
                    *current_pos = span.end;
                }
                // Recurse into right side
                walk_expr(rhs, source, items, current_pos, var_types);
            }
            Expression::Identifier(name) => {
                // Identifier in expression - check if it's a thunk
                if let Some(span) = find_identifier_span_in_expr(name, source, *current_pos) {
                    let mut item = SemanticItem::variable(span);
                    // Check if this identifier is a thunk
                    if let Some(ty) = var_types.get(name) {
                        if matches!(ty, ValueKind::Thunk(_)) {
                            item = item.with_modifiers(SemanticModifiers::THUNK);
                        }
                    }
                    items.push(item);
                    *current_pos = span.end;
                }
            }
            Expression::FunctionCall { callee, arguments, .. } => {
                walk_expr(callee, source, items, current_pos, var_types);
                for arg in arguments {
                    walk_expr(arg, source, items, current_pos, var_types);
                }
            }
            Expression::PartialCall { func, args, .. } => {
                walk_expr(func, source, items, current_pos, var_types);
                for arg in args {
                    if let crate::core::ast::CallArgument::Expr(e) = arg {
                        walk_expr(e, source, items, current_pos, var_types);
                    }
                }
            }
            Expression::MemberAccess { object, member: _, .. } => {
                walk_expr(object, source, items, current_pos, var_types);
                if let Some(span) = find_op_span_in_expr(".", source, *current_pos) {
                    items.push(SemanticItem::operator(span));
                    *current_pos = span.end;
                }
                // Member name would be an identifier, but we handle identifiers separately
            }
            _ => {}
        }
    }
    
    fn find_identifier_span_in_expr(identifier: &str, source: &str, start_from: usize) -> Option<Span> {
        let name_bytes = identifier.as_bytes();
        for i in start_from..source.len() {
            if i + name_bytes.len() <= source.len() {
                if &source.as_bytes()[i..i + name_bytes.len()] == name_bytes {
                    let before_ok = i == 0 || {
                        let ch = source.as_bytes()[i - 1] as char;
                        !ch.is_alphanumeric() && ch != '_'
                    };
                    let after_ok = i + name_bytes.len() >= source.len() || {
                        let ch = source.as_bytes()[i + name_bytes.len()] as char;
                        !ch.is_alphanumeric() && ch != '_'
                    };
                    if before_ok && after_ok {
                        let line_start = source[..i].rfind('\n').map(|p| p + 1).unwrap_or(0);
                        let line_text = &source[line_start..];
                        let col_in_line = i - line_start;
                        if line_text[..col_in_line.min(line_text.len())].find("//").is_none() {
                            return Some(Span::new(i, i + name_bytes.len()));
                        }
                    }
                }
            }
        }
        None
    }
    
    fn find_op_span_in_expr(op_str: &str, source: &str, after_pos: usize) -> Option<Span> {
        let op_bytes = op_str.as_bytes();
        for i in after_pos..source.len() {
            if i + op_bytes.len() <= source.len() {
                if &source.as_bytes()[i..i + op_bytes.len()] == op_bytes {
                    let line_start = source[..i].rfind('\n').map(|p| p + 1).unwrap_or(0);
                    let line_text = &source[line_start..];
                    let col_in_line = i - line_start;
                    if line_text[..col_in_line.min(line_text.len())].find("//").is_none() {
                        return Some(Span::new(i, i + op_bytes.len()));
                    }
                }
            }
        }
        None
    }
    
    match stmt {
        Statement::Let { expression, .. } |
        Statement::Const { expression, .. } |
        Statement::Assign { expression, .. } |
        Statement::AssignIncrement { expression, .. } |
        Statement::AssignDecrement { expression, .. } |
        Statement::Return { expression, .. } |
        Statement::Expression(expression) => {
            walk_expr(expression, source, items, current_pos, var_types);
        }
        Statement::If { arms, else_block, .. } => {
            for (expr, _) in arms {
                walk_expr(expr, source, items, current_pos, var_types);
            }
            if let Some(block) = else_block {
                for stmt in &block.statements {
                    collect_expression_operators(stmt, source, items, current_pos, var_types);
                }
            }
        }
        Statement::Match { expression, cases, .. } => {
            walk_expr(expression, source, items, current_pos, var_types);
            for (opt_expr, block) in cases {
                if let Some(expr) = opt_expr {
                    walk_expr(expr, source, items, current_pos, var_types);
                }
                for stmt in &block.statements {
                    collect_expression_operators(stmt, source, items, current_pos, var_types);
                }
            }
        }
        Statement::While { condition, .. } => {
            walk_expr(condition, source, items, current_pos, var_types);
        }
        _ => {}
    }
}

