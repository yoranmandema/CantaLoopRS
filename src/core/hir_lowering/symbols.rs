//! Symbol table and symbol management for HIR lowering.
//! 
//! Symbols are the named entities in the program (variables, functions, parameters, modules).
//! All symbols have stable IDs for fast lookups, cross-file resolution, and editor features.

use std::collections::HashMap;

use super::{HirAst, ValueKind, Span, ScopeId, FunctionSignature};

/// Format a function signature as a type string for display.
fn format_function_type_string(sig: &FunctionSignature) -> String {
    fn format_kind_recursive(kind: &ValueKind) -> String {
        match kind {
            ValueKind::Number => "num".to_string(),
            ValueKind::String => "string".to_string(),
            ValueKind::Boolean => "bool".to_string(),
            ValueKind::Unknown => "unknown".to_string(),
            ValueKind::Function(ty) => ty.clone(),
            ValueKind::Thunk(ty) => ty.clone(),
            ValueKind::Void => "void".to_string(),
            ValueKind::Array(inner) => {
                let inner_str = format_kind_recursive(inner);
                format!("{}[]", inner_str)
            }
        }
    }
    let format_kind = |kind: &ValueKind| -> String {
        format_kind_recursive(kind)
    };
    
    let params: Vec<String> = sig.params.iter()
        .map(|p| format_kind(p))
        .collect();
    
    let param_str = if params.len() == 1 {
        params[0].clone()
    } else {
        format!("({})", params.join(","))
    };
    
    let return_str = format_kind(&sig.return_type);
    format!("{} -> {}", param_str, return_str)
}

/// Unique identifier for a symbol (variable, function, parameter, module).
/// Symbols are the named entities in the program that can be referenced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SymbolId(pub u32);

/// Unique identifier for a type.
/// Types are interned and deduplicated, so identical types share the same TypeId.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeId(pub u32);

/// Kind of symbol in the symbol table.
#[derive(Debug, Clone, PartialEq)]
pub enum SymbolKind {
    Function,
    Variable,
    Parameter,
    Module,
}

/// A symbol in the symbol table, representing a named entity in the program.
/// 
/// This is the ID-based representation. All references to symbols use SymbolId,
/// not strings. The name is stored for display and debugging, but lookups use IDs.
#[derive(Debug, Clone)]
pub struct Symbol {
    /// Unique identifier for this symbol.
    pub id: SymbolId,
    /// Human-readable name (for display, debugging, and initial lookup).
    /// After symbol resolution, all references use `id`, not `name`.
    pub name: String,
    /// Kind of symbol (function, variable, parameter, module).
    pub kind: SymbolKind,
    /// Type of this symbol (as ValueKind for now; will migrate to TypeId).
    pub ty: ValueKind,
    /// Source code span where this symbol is defined.
    pub defined_at: Option<Span>,
    /// Scope in which this symbol is defined.
    pub scope: ScopeId,
}

/// Symbol table mapping names to symbols.
/// For now, we use a Vec to preserve order and allow multiple symbols with the same name
/// (shadowing). The LSP can filter by scope as needed.
#[derive(Debug, Clone)]
pub struct SymbolTable {
    pub symbols: Vec<Symbol>,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self {
            symbols: Vec::new(),
        }
    }

    /// Find symbols by name (may return multiple due to shadowing).
    pub fn find_by_name(&self, name: &str) -> Vec<&Symbol> {
        self.symbols.iter().filter(|s| s.name == name).collect()
    }

    /// Get all symbols in a scope (for completion).
    pub fn get_all(&self) -> &[Symbol] {
        &self.symbols
    }
}

/// Build a symbol table from HIR, extracting spans from AST.
pub fn build_symbol_table(hir: &HirAst, ast: &crate::core::ast::Program, source: &str) -> SymbolTable {
    let mut table = SymbolTable::new();
    
    // Extract all identifier spans from AST
    let span_map = extract_identifier_spans(ast, source);
    
    // Track which spans we've used for each name
    let mut used_spans: HashMap<String, usize> = HashMap::new();
    
    // Symbol ID counter - each symbol gets a unique ID
    let mut next_symbol_id = 0u32;

    // Helper to get next unused span for a name
    let mut get_span = |name: &str| -> Option<Span> {
        if let Some(spans) = span_map.get(name) {
            let index = used_spans.entry(name.to_string()).or_insert(0);
            if *index < spans.len() {
                let span = spans[*index];
                *index += 1;
                Some(span)
            } else {
                spans.first().copied()
            }
        } else {
            None
        }
    };

    // Add all functions (both regular and built-in)
    for (_, func) in &hir.functions {
        let symbol_id = SymbolId(next_symbol_id);
        next_symbol_id += 1;
        // Functions are defined in the root scope (scope 0)
        let scope = ScopeId(0);
        table.symbols.push(Symbol {
            id: symbol_id,
            name: func.name.clone(),
            kind: SymbolKind::Function,
            ty: ValueKind::Function(
                format_function_type_string(&func.signature),
            ),
            defined_at: get_span(&func.name),
            scope,
        });
    }

    // Add imported functions (they're not in hir.functions, but in import_table)
    for (name, func_id) in &hir.import_table {
        // Check if this is a constant (variable ID) or a function
        let is_constant = hir.scopes.scopes.iter().any(|scope| {
            scope.vars.iter().any(|v| v.id == *func_id)
        });
        
        if is_constant {
            // It's a constant - add as a variable
            if let Some(var) = hir.scopes.scopes.iter()
                .find_map(|scope| scope.vars.iter().find(|v| v.id == *func_id)) {
                let symbol_id = SymbolId(next_symbol_id);
                next_symbol_id += 1;
                let scope = ScopeId(0);
                table.symbols.push(Symbol {
                    id: symbol_id,
                    name: name.clone(),
                    kind: SymbolKind::Variable,
                    ty: var.kind.clone(),
                    defined_at: get_span(name),
                    scope,
                });
            }
        } else {
            // It's a function - try to get signature from hir.functions, or use generic type
            let func_type = if let Some(func) = hir.functions.get(func_id) {
                ValueKind::Function(format_function_type_string(&func.signature))
            } else {
                // Function from another module - use generic function type
                // This happens when importing from other modules
                ValueKind::Function("unknown -> unknown".to_string())
            };
            
            let symbol_id = SymbolId(next_symbol_id);
            next_symbol_id += 1;
            // Imported symbols are in root scope
            let scope = ScopeId(0);
            table.symbols.push(Symbol {
                id: symbol_id,
                name: name.clone(),
                kind: SymbolKind::Function,
                ty: func_type,
                defined_at: get_span(name),
                scope,
            });
        }
    }

    // Add all variables from all scopes
    for (scope_idx, scope) in hir.scopes.scopes.iter().enumerate() {
        let scope_id = ScopeId(scope_idx);
        for var in &scope.vars {
            // Check if this is a parameter (parameters are usually at function scope start)
            let kind = if scope.vars.iter().position(|v| v.id == var.id) < Some(3) {
                // Heuristic: first few variables in a scope are often parameters
                // This is approximate - ideally we'd track parameter vs variable
                SymbolKind::Parameter
            } else {
                SymbolKind::Variable
            };
            let symbol_id = SymbolId(next_symbol_id);
            next_symbol_id += 1;
            table.symbols.push(Symbol {
                id: symbol_id,
                name: var.name.clone(),
                kind,
                ty: var.kind.clone(),
                defined_at: get_span(&var.name),
                scope: scope_id,
            });
        }
    }

    table
}

/// Build symbol table without spans (fallback when source is not available).
pub fn build_symbol_table_without_spans(hir: &HirAst) -> SymbolTable {
    let mut table = SymbolTable::new();
    let mut next_symbol_id = 0u32;

    // Add all functions (both regular and built-in)
    for (_, func) in &hir.functions {
        let symbol_id = SymbolId(next_symbol_id);
        next_symbol_id += 1;
        table.symbols.push(Symbol {
            id: symbol_id,
            name: func.name.clone(),
            kind: SymbolKind::Function,
            ty: ValueKind::Function(
                format_function_type_string(&func.signature),
            ),
            defined_at: None,
            scope: ScopeId(0),
        });
    }

    // Add imported functions
    for (name, func_id) in &hir.import_table {
        // Check if this is a constant (variable ID) or a function
        let is_constant = hir.scopes.scopes.iter().any(|scope| {
            scope.vars.iter().any(|v| v.id == *func_id)
        });
        
        if is_constant {
            // It's a constant - add as a variable
            if let Some(var) = hir.scopes.scopes.iter()
                .find_map(|scope| scope.vars.iter().find(|v| v.id == *func_id)) {
                let symbol_id = SymbolId(next_symbol_id);
                next_symbol_id += 1;
                table.symbols.push(Symbol {
                    id: symbol_id,
                    name: name.clone(),
                    kind: SymbolKind::Variable,
                    ty: var.kind.clone(),
                    defined_at: None,
                    scope: ScopeId(0),
                });
            }
        } else {
            // It's a function - try to get signature from hir.functions, or use generic type
            let func_type = if let Some(func) = hir.functions.get(func_id) {
                ValueKind::Function(format_function_type_string(&func.signature))
            } else {
                // Function from another module - use generic function type
                ValueKind::Function("unknown -> unknown".to_string())
            };
            
            let symbol_id = SymbolId(next_symbol_id);
            next_symbol_id += 1;
            table.symbols.push(Symbol {
                id: symbol_id,
                name: name.clone(),
                kind: SymbolKind::Function,
                ty: func_type,
                defined_at: None,
                scope: ScopeId(0),
            });
        }
    }

    // Add all variables from all scopes
    for (scope_idx, scope) in hir.scopes.scopes.iter().enumerate() {
        let scope_id = ScopeId(scope_idx);
        for var in &scope.vars {
            let symbol_id = SymbolId(next_symbol_id);
            next_symbol_id += 1;
            table.symbols.push(Symbol {
                id: symbol_id,
                name: var.name.clone(),
                kind: SymbolKind::Variable,
                ty: var.kind.clone(),
                defined_at: None,
                scope: scope_id,
            });
        }
    }

    table
}

/// Extract identifier spans from AST by walking the tree and finding identifiers in source text.
fn extract_identifier_spans(ast: &crate::core::ast::Program, source: &str) -> HashMap<String, Vec<Span>> {
    let mut span_map: HashMap<String, Vec<Span>> = HashMap::new();
    
    // Helper to find identifier position in source text
    let find_identifier_span = |name: &str, start_from: usize| -> Option<Span> {
        let name_bytes = name.as_bytes();
        for i in start_from..source.len() {
            if i + name_bytes.len() <= source.len() {
                if &source.as_bytes()[i..i + name_bytes.len()] == name_bytes {
                    // Check word boundaries
                    let before_ok = i == 0 || {
                        let ch = source.as_bytes()[i - 1] as char;
                        !ch.is_alphanumeric() && ch != '_'
                    };
                    let after_ok = i + name_bytes.len() >= source.len() || {
                        let ch = source.as_bytes()[i + name_bytes.len()] as char;
                        !ch.is_alphanumeric() && ch != '_'
                    };
                    
                    if before_ok && after_ok {
                        // Check if it's in a comment
                        let line_start = source[..i].rfind('\n').map(|p| p + 1).unwrap_or(0);
                        let line_text = &source[line_start..];
                        let col_in_line = i - line_start;
                        
                        // Simple check: if we find // before this position on the same line
                        if line_text[..col_in_line.min(line_text.len())].find("//").is_some() {
                            continue; // Skip if in comment
                        }
                        
                        return Some(Span::new(i, i + name_bytes.len()));
                    }
                }
            }
        }
        None
    };
    
    // Walk AST to find all identifiers
    let mut current_pos = 0;
    for block in &ast.blocks {
        for stmt in &block.statements {
            match stmt {
                crate::core::ast::Statement::FunctionDeclaration { identifier, .. } => {
                    if let Some(span) = find_identifier_span(identifier, current_pos) {
                        span_map.entry(identifier.clone()).or_insert_with(Vec::new).push(span);
                        current_pos = span.end;
                    }
                }
                crate::core::ast::Statement::Let { identifier, .. } => {
                    if let Some(span) = find_identifier_span(identifier, current_pos) {
                        span_map.entry(identifier.clone()).or_insert_with(Vec::new).push(span);
                        current_pos = span.end;
                    }
                }
                crate::core::ast::Statement::Const { identifier, .. } => {
                    if let Some(span) = find_identifier_span(identifier, current_pos) {
                        span_map.entry(identifier.clone()).or_insert_with(Vec::new).push(span);
                        current_pos = span.end;
                    }
                }
                crate::core::ast::Statement::Assign { identifier, .. } |
                crate::core::ast::Statement::AssignIncrement { identifier, .. } |
                crate::core::ast::Statement::AssignDecrement { identifier, .. } => {
                    if let Some(span) = find_identifier_span(identifier, current_pos) {
                        span_map.entry(identifier.clone()).or_insert_with(Vec::new).push(span);
                        current_pos = span.end;
                    }
                }
                crate::core::ast::Statement::For { var_name, .. } => {
                    if let Some(span) = find_identifier_span(var_name, current_pos) {
                        span_map.entry(var_name.clone()).or_insert_with(Vec::new).push(span);
                        current_pos = span.end;
                    }
                }
                crate::core::ast::Statement::Mod { identifier } => {
                    if let Some(span) = find_identifier_span(identifier, current_pos) {
                        span_map.entry(identifier.clone()).or_insert_with(Vec::new).push(span);
                        current_pos = span.end;
                    }
                }
                _ => {}
            }
            
            // Also walk expressions to find identifiers
            walk_expression_for_spans(stmt, source, &mut span_map, &mut current_pos);
        }
    }
    
    span_map
}

/// Walk an expression to find identifier spans.
fn walk_expression_for_spans(
    expr_or_stmt: &crate::core::ast::Statement,
    source: &str,
    span_map: &mut HashMap<String, Vec<Span>>,
    current_pos: &mut usize,
) {
    use crate::core::ast::{Expression, Statement};
    
    let find_identifier_span = |name: &str, start_from: usize| -> Option<Span> {
        let name_bytes = name.as_bytes();
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
                        if line_text[..col_in_line.min(line_text.len())].find("//").is_some() {
                            continue;
                        }
                        return Some(Span::new(i, i + name_bytes.len()));
                    }
                }
            }
        }
        None
    };
    
    fn walk_expr(
        expr: &Expression,
        source: &str,
        span_map: &mut HashMap<String, Vec<Span>>,
        current_pos: &mut usize,
        find_identifier_span: &dyn Fn(&str, usize) -> Option<Span>,
    ) {
        match expr {
            Expression::Identifier(name) => {
                if let Some(span) = find_identifier_span(name, *current_pos) {
                    span_map.entry(name.clone()).or_insert_with(Vec::new).push(span);
                    *current_pos = span.end;
                }
            }
            Expression::FunctionCall { callee, arguments, .. } => {
                walk_expr(callee, source, span_map, current_pos, find_identifier_span);
                for arg in arguments {
                    walk_expr(arg, source, span_map, current_pos, find_identifier_span);
                }
            }
            Expression::PartialCall { func, args, .. } => {
                walk_expr(func, source, span_map, current_pos, find_identifier_span);
                for arg in args {
                    if let crate::core::ast::CallArgument::Expr(e) = arg {
                        walk_expr(e, source, span_map, current_pos, find_identifier_span);
                    }
                }
            }
            Expression::MemberAccess { object, member, .. } => {
                walk_expr(object, source, span_map, current_pos, find_identifier_span);
                if let Some(span) = find_identifier_span(member, *current_pos) {
                    span_map.entry(member.clone()).or_insert_with(Vec::new).push(span);
                    *current_pos = span.end;
                }
            }
            Expression::Prefix { rhs, .. } => {
                walk_expr(rhs, source, span_map, current_pos, find_identifier_span);
            }
            Expression::Postfix { lhs, .. } => {
                walk_expr(lhs, source, span_map, current_pos, find_identifier_span);
            }
            Expression::Infix { lhs, rhs, .. } => {
                walk_expr(lhs, source, span_map, current_pos, find_identifier_span);
                walk_expr(rhs, source, span_map, current_pos, find_identifier_span);
            }
            Expression::Compose { lhs, rhs, .. } => {
                walk_expr(lhs, source, span_map, current_pos, find_identifier_span);
                walk_expr(rhs, source, span_map, current_pos, find_identifier_span);
            }
            Expression::Loop { init_vars, .. } => {
                for (var_name, _) in init_vars {
                    if let Some(span) = find_identifier_span(var_name, *current_pos) {
                        span_map.entry(var_name.clone()).or_insert_with(Vec::new).push(span);
                        *current_pos = span.end;
                    }
                }
            }
            Expression::ArrayIndex { array, .. } => {
                // Recurse into array expression to find identifiers
                walk_expr(array, source, span_map, current_pos, find_identifier_span);
                // TODO: Handle index expressions to find identifiers in indices
            }
            _ => {}
        }
    }
    
    match expr_or_stmt {
        Statement::Let { expression, .. } |
        Statement::Const { expression, .. } |
        Statement::Assign { expression, .. } |
        Statement::AssignIncrement { expression, .. } |
        Statement::AssignDecrement { expression, .. } |
        Statement::Return { expression, .. } |
        Statement::Expression(expression) => {
            walk_expr(expression, source, span_map, current_pos, &find_identifier_span);
        }
        Statement::If { arms, else_block, .. } => {
            for (expr, _) in arms {
                walk_expr(expr, source, span_map, current_pos, &find_identifier_span);
            }
            if let Some(block) = else_block {
                for stmt in &block.statements {
                    walk_expression_for_spans(stmt, source, span_map, current_pos);
                }
            }
        }
        Statement::Match { expression, cases, .. } => {
            walk_expr(expression, source, span_map, current_pos, &find_identifier_span);
            for (opt_expr, block) in cases {
                if let Some(expr) = opt_expr {
                    walk_expr(expr, source, span_map, current_pos, &find_identifier_span);
                }
                for stmt in &block.statements {
                    walk_expression_for_spans(stmt, source, span_map, current_pos);
                }
            }
        }
        Statement::While { condition, body, .. } => {
            walk_expr(condition, source, span_map, current_pos, &find_identifier_span);
            for stmt in &body.statements {
                walk_expression_for_spans(stmt, source, span_map, current_pos);
            }
        }
        Statement::Loop { body, .. } |
        Statement::For { body, .. } |
        Statement::FunctionDeclaration { body, .. } => {
            for stmt in &body.statements {
                walk_expression_for_spans(stmt, source, span_map, current_pos);
            }
        }
        _ => {}
    }
}

