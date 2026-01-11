//! Symbol table and symbol management for HIR lowering.
//!
//! Symbols are the named entities in the program (variables, functions, parameters, modules).
//! All symbols have stable IDs for fast lookups, cross-file resolution, and editor features.

use std::collections::HashMap;

use super::{FunctionSignature, HirAst, ScopeId, Span, ValueKind};
use serde::Serialize;

/// Format a function signature as a type string for display.
fn format_function_type_string(sig: &FunctionSignature) -> String {
    fn format_kind_recursive(kind: &ValueKind) -> String {
        match kind {
            ValueKind::Any => "any".to_string(),
            ValueKind::Number => "num".to_string(),
            ValueKind::String => "string".to_string(),
            ValueKind::Boolean => "bool".to_string(),
            ValueKind::Unknown => "unknown".to_string(),
            ValueKind::Function(ty) => ty.clone(),
            ValueKind::Thunk(ty) => ty.clone(),
            ValueKind::Callable => "callable".to_string(),
            ValueKind::Void => "void".to_string(),
            ValueKind::Struct(name) => name.clone(),
            ValueKind::Array(inner) => {
                let inner_str = format_kind_recursive(inner);
                format!("{}[]", inner_str)
            }
        }
    }
    let format_kind = |kind: &ValueKind| -> String { format_kind_recursive(kind) };

    let params: Vec<String> = sig.params.iter().map(|p| format_kind(p)).collect();

    let param_str = if params.is_empty() {
        "()".to_string()
    } else if params.len() == 1 {
        params[0].clone()
    } else {
        format!("({})", params.join(","))
    };

    let return_str = format_kind(&sig.return_type);
    format!("{} -> {}", param_str, return_str)
}

/// Unique identifier for a symbol (variable, function, parameter, module).
/// Symbols are the named entities in the program that can be referenced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct SymbolId(pub u32);

/// Unique identifier for a type.
/// Types are interned and deduplicated, so identical types share the same TypeId.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct TypeId(pub u32);

/// Kind of symbol in the symbol table.
#[derive(Debug, Clone, PartialEq, Serialize)]
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
#[derive(Debug, Clone, Serialize)]
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
#[derive(Debug, Clone, Serialize)]
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

/// Extract the module name from AST.
fn extract_module_name(ast: &crate::core::ast::Program) -> Option<String> {
    use crate::core::ast::Statement;
    for block in &ast.blocks {
        for stmt in &block.statements {
            if let Statement::Mod { identifier } = stmt {
                return Some(identifier.clone());
            }
        }
    }
    None
}

/// Build a symbol table from HIR, extracting spans from CST.
pub fn build_symbol_table(
    hir: &HirAst,
    ast: &crate::core::ast::Program,
    cst: &crate::core::cst::CstProgram,
) -> SymbolTable {
    let mut table = SymbolTable::new();

    // Extract all identifier spans from CST (which has accurate spans from the parser)
    let span_map = extract_identifier_spans_from_cst(cst);

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

    // Determine the current module name (default to "__main__" if not found)
    let current_module = extract_module_name(ast).unwrap_or_else(|| "__main__".to_string());

    // Add all functions (both regular and built-in)
    // CRITICAL: Built-in functions may be registered with qualified names (e.g., "std.print")
    // but also need to be accessible with unqualified names (e.g., "print")
    // Since register_builtin_function uses function ID as key, only one name per ID exists in hir.functions.
    // We create symbols for the name in hir.functions, and the matching logic in build_semantic_index
    // will match identifiers to symbols by unqualified name.
    let mut function_ids_seen = std::collections::HashSet::new();
    for (func_id, func) in &hir.functions {
        // Skip duplicate function IDs (shouldn't happen, but defensive)
        if !function_ids_seen.insert(*func_id) {
            continue;
        }
        
        let symbol_id = SymbolId(next_symbol_id);
        next_symbol_id += 1;
        // Functions are defined in the root scope (scope 0)
        let scope = ScopeId(0);
        
        // CRITICAL: For built-in functions with qualified names, also create a symbol with unqualified name
        // This allows code to call built-in functions directly (e.g., "print" instead of "std.print")
        let qualified_name = func.name.clone();
        let unqualified_name = if let Some(dot_pos) = qualified_name.find('.') {
            qualified_name[dot_pos + 1..].to_string()
        } else {
            qualified_name.clone()
        };
        
        // Add symbol with the name from hir.functions (may be qualified or unqualified)
        table.symbols.push(Symbol {
            id: symbol_id,
            name: qualified_name.clone(),
            kind: SymbolKind::Function,
            ty: ValueKind::Function(format_function_type_string(&func.signature)),
            defined_at: get_span(&qualified_name).or_else(|| get_span(&unqualified_name)),
            scope,
        });
        
        // If the function name is qualified, also add a symbol with unqualified name
        // This ensures code can call built-in functions directly
        if qualified_name != unqualified_name {
            let unqualified_symbol_id = SymbolId(next_symbol_id);
            next_symbol_id += 1;
            table.symbols.push(Symbol {
                id: unqualified_symbol_id,
                name: unqualified_name.clone(),
                kind: SymbolKind::Function,
                ty: ValueKind::Function(format_function_type_string(&func.signature)),
                defined_at: get_span(&unqualified_name).or_else(|| get_span(&qualified_name)),
                scope,
            });
        }
    }

    // Add imported functions (they're not in hir.functions, but in import_table)
    // Only include imports for the CURRENT module, not all modules
    let current_imports = hir.module_imports.get(&current_module);
    if let Some(imports) = current_imports {
        for (name, func_id) in imports {
            // Check if this is a constant (variable ID) or a function
            let is_constant = hir
                .scopes
                .scopes
                .iter()
                .any(|scope| scope.vars.iter().any(|v| v.id == *func_id));

            if is_constant {
                // It's a constant - add as a variable
                if let Some(var) = hir
                    .scopes
                    .scopes
                    .iter()
                    .find_map(|scope| scope.vars.iter().find(|v| v.id == *func_id))
                {
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
    }
    
    // CRITICAL: Add module symbols (std, bevy, functional, etc.)
    // Module names are extracted from multiple sources:
    // 1. module_imports keys (modules that have imports registered)
    // 2. Function names with module qualifiers (e.g., "std.print" -> "std")
    // 3. Use statement paths (e.g., "use print from std" -> "std")
    let mut module_names: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Extract module names from module_imports (modules that have imports registered)
    for module_name in hir.module_imports.keys() {
        module_names.insert(module_name.clone());
    }

    // Also check if any function names contain module qualifiers (e.g., "std.print")
    // This helps us identify stdlib modules even if they haven't been imported yet
    for func in hir.functions.values() {
        if let Some(dot_pos) = func.name.find('.') {
            let module_name = &func.name[..dot_pos];
            module_names.insert(module_name.to_string());
        }
    }

    // CRITICAL FIX: Extract module names from Use statements in CST
    // This ensures modules like "std" and "functional" in "use print from std"
    // get symbols created, even if they're not in module_imports yet
    eprintln!("[LSP] [SYMBOLS] Extracting module names from Use statements in CST...");
    eprintln!("[LSP] [SYMBOLS] CST has {} blocks", cst.blocks.len());
    for (block_idx, block) in cst.blocks.iter().enumerate() {
        eprintln!("[LSP] [SYMBOLS] Block {} has {} statements", block_idx, block.node.statements.len());
        for (stmt_idx, stmt) in block.node.statements.iter().enumerate() {
            match &stmt.node {
                crate::core::cst::CstStatement::Use { path, selector, .. } => {
                    eprintln!("[LSP] [SYMBOLS] Statement {}: Use statement with {} path segments", stmt_idx, path.len());
                    for path_segment in path {
                        eprintln!("[LSP] [SYMBOLS] Found module in Use statement: '{}'", path_segment.node);
                        module_names.insert(path_segment.node.clone());
                    }
                    // Also log the selector for debugging
                    match &selector.node {
                        crate::core::cst::CstImportSelector::Single(name) => {
                            eprintln!("[LSP] [SYMBOLS] Importing: {}", name.node);
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
    eprintln!("[LSP] [SYMBOLS] Total module names collected: {} -> {:?}", module_names.len(), module_names);
    
    // Create symbols for all identified modules
    for module_name in module_names {
        // Only create a symbol if we haven't already created one for this module
        if !table.symbols.iter().any(|s| s.name == module_name && s.kind == SymbolKind::Module) {
            let symbol_id = SymbolId(next_symbol_id);
            next_symbol_id += 1;
            table.symbols.push(Symbol {
                id: symbol_id,
                name: module_name.clone(),
                kind: SymbolKind::Module,
                ty: ValueKind::Unknown, // Modules don't have types
                defined_at: get_span(&module_name),
                scope: ScopeId(0),
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

    // Determine the current module name (default to "__main__" if not found in any module)
    // Note: Without AST, we can't determine the module name, so we default to "__main__"
    // This is acceptable since this function is only used as a fallback
    let current_module = "__main__".to_string();

    // Add all functions (both regular and built-in)
    for (_, func) in &hir.functions {
        let symbol_id = SymbolId(next_symbol_id);
        next_symbol_id += 1;
        table.symbols.push(Symbol {
            id: symbol_id,
            name: func.name.clone(),
            kind: SymbolKind::Function,
            ty: ValueKind::Function(format_function_type_string(&func.signature)),
            defined_at: None,
            scope: ScopeId(0),
        });
    }

    // Add imported functions
    // Only include imports for the CURRENT module, not all modules
    let current_imports = hir.module_imports.get(&current_module);
    if let Some(imports) = current_imports {
        for (name, func_id) in imports {
        // Check if this is a constant (variable ID) or a function
        let is_constant = hir
            .scopes
            .scopes
            .iter()
            .any(|scope| scope.vars.iter().any(|v| v.id == *func_id));

        if is_constant {
            // It's a constant - add as a variable
            if let Some(var) = hir
                .scopes
                .scopes
                .iter()
                .find_map(|scope| scope.vars.iter().find(|v| v.id == *func_id))
            {
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

/// Extract identifier spans from CST (which has accurate spans from the parser).
pub fn extract_identifier_spans_from_cst(
    cst: &crate::core::cst::CstProgram,
) -> HashMap<String, Vec<Span>> {
    use crate::core::cst::{CstExpr, CstStatement};
    
    let mut span_map: HashMap<String, Vec<Span>> = HashMap::new();

    // Helper to convert CST span (u32) to HIR span (usize)
    let cst_to_hir_span = |cst_span: crate::core::cst::Span| -> Span {
        Span::new(cst_span.start as usize, cst_span.end as usize)
    };

    // Helper to add a span for an identifier (inline to avoid closure borrow issues)
    // We'll add spans directly instead of using a closure

    // Helper to walk expressions in CST recursively
    fn walk_cst_expr(expr: &crate::core::cst::Spanned<CstExpr>, span_map: &mut HashMap<String, Vec<Span>>) {
        use crate::core::cst::CstExpr;
        
        let cst_to_hir_span = |cst_span: crate::core::cst::Span| -> Span {
            Span::new(cst_span.start as usize, cst_span.end as usize)
        };
        
        match &expr.node {
            CstExpr::Identifier(name) => {
                span_map
                    .entry(name.node.clone())
                    .or_insert_with(Vec::new)
                    .push(cst_to_hir_span(name.span));
            }
            CstExpr::FunctionCall { callee, arguments, .. } => {
                walk_cst_expr(callee, span_map);
                for arg in arguments {
                    // Arguments are Spanned<CstCallArgument>, extract the expression if present
                    if let crate::core::cst::CstCallArgument::Expr(e) = &arg.node {
                        walk_cst_expr(e, span_map);
                    }
                }
            }
            CstExpr::PartialCall { func, args, .. } => {
                walk_cst_expr(func, span_map);
                for arg in args {
                    if let crate::core::cst::CstCallArgument::Expr(e) = &arg.node {
                        walk_cst_expr(e, span_map);
                    }
                }
            }
            CstExpr::MemberAccess { object, members, .. } => {
                walk_cst_expr(object, span_map);
                for member in members {
                    span_map
                        .entry(member.node.clone())
                        .or_insert_with(Vec::new)
                        .push(cst_to_hir_span(member.span));
                }
            }
            CstExpr::Prefix { rhs, .. } => {
                walk_cst_expr(rhs, span_map);
            }
            CstExpr::Postfix { lhs, .. } => {
                walk_cst_expr(lhs, span_map);
            }
            CstExpr::Infix { lhs, rhs, .. } => {
                walk_cst_expr(lhs, span_map);
                walk_cst_expr(rhs, span_map);
            }
            CstExpr::Compose { lhs, rhs, .. } => {
                walk_cst_expr(lhs, span_map);
                walk_cst_expr(rhs, span_map);
            }
            CstExpr::Loop { init_vars, body, .. } => {
                for (var_name, _, expr) in init_vars {
                    span_map
                        .entry(var_name.node.clone())
                        .or_insert_with(Vec::new)
                        .push(cst_to_hir_span(var_name.span));
                    walk_cst_expr(expr, span_map);
                }
                // Walk body expressions  
                for stmt in &body.node.statements {
                    if let crate::core::cst::CstStatement::Expression(expr) = &stmt.node {
                        walk_cst_expr(expr, span_map);
                    }
                }
            }
            CstExpr::StructInit { struct_name, fields, .. } => {
                span_map
                    .entry(struct_name.node.clone())
                    .or_insert_with(Vec::new)
                    .push(cst_to_hir_span(struct_name.span));
                for field in fields {
                    walk_cst_expr(&field.node.value, span_map);
                }
            }
            CstExpr::FieldAccess { object, field, .. } => {
                walk_cst_expr(object, span_map);
                span_map
                    .entry(field.node.clone())
                    .or_insert_with(Vec::new)
                    .push(cst_to_hir_span(field.span));
            }
            CstExpr::ArrayIndex { array, indices, .. } => {
                walk_cst_expr(array, span_map);
                for idx_spec in indices {
                    match &idx_spec.node {
                        crate::core::cst::CstIndexSpec::Single(expr) => walk_cst_expr(expr, span_map),
                        crate::core::cst::CstIndexSpec::Range { start, end, step, .. } => {
                            if let Some(expr) = start {
                                walk_cst_expr(expr, span_map);
                            }
                            if let Some(expr) = end {
                                walk_cst_expr(expr, span_map);
                            }
                            if let Some((_, expr)) = step {
                                walk_cst_expr(expr, span_map);
                            }
                        }
                        crate::core::cst::CstIndexSpec::InclusiveRange { start, end, .. } => {
                            if let Some(expr) = start {
                                walk_cst_expr(expr, span_map);
                            }
                            if let Some(expr) = end {
                                walk_cst_expr(expr, span_map);
                            }
                        }
                    }
                }
            }
            CstExpr::Array { elements, .. } => {
                for elem in elements {
                    walk_cst_expr(elem, span_map);
                }
            }
            CstExpr::Closure { arguments, body, .. } => {
                for arg in arguments {
                    span_map
                        .entry(arg.node.identifier.node.clone())
                        .or_insert_with(Vec::new)
                        .push(cst_to_hir_span(arg.node.identifier.span));
                }
                match body {
                    crate::core::cst::CstClosureBody::Expression(expr) => walk_cst_expr(expr, span_map),
                    crate::core::cst::CstClosureBody::Block(block) => {
                        for stmt in &block.node.statements {
                            if let crate::core::cst::CstStatement::Expression(expr) = &stmt.node {
                                walk_cst_expr(expr, span_map);
                            }
                        }
                    }
                }
            }
            CstExpr::Group { inner, .. } => {
                walk_cst_expr(inner, span_map);
            }
            CstExpr::Literal(_) => {}
            _ => {
                log::debug!("Unhandled CstExpr variant in walk_cst_expr: {:?}", std::mem::discriminant(&expr.node));
            }
        }
    }

    // Helper to walk statements recursively (for nested blocks)
    fn walk_cst_statements(statements: &[crate::core::cst::Spanned<CstStatement>], span_map: &mut HashMap<String, Vec<Span>>) {
        use crate::core::cst::CstStatement;
        
        let cst_to_hir_span = |cst_span: crate::core::cst::Span| -> Span {
            Span::new(cst_span.start as usize, cst_span.end as usize)
        };
        
        fn walk_cst_expr_inner(expr: &crate::core::cst::Spanned<CstExpr>, span_map: &mut HashMap<String, Vec<Span>>) {
            use crate::core::cst::CstExpr;
            let cst_to_hir_span = |cst_span: crate::core::cst::Span| -> Span {
                Span::new(cst_span.start as usize, cst_span.end as usize)
            };
            match &expr.node {
                CstExpr::Identifier(name) => {
                    span_map
                        .entry(name.node.clone())
                        .or_insert_with(Vec::new)
                        .push(cst_to_hir_span(name.span));
                }
                CstExpr::FunctionCall { callee, arguments, .. } => {
                    walk_cst_expr_inner(callee, span_map);
                    for arg in arguments {
                        // Arguments are Spanned<CstCallArgument>, extract the expression if present
                        if let crate::core::cst::CstCallArgument::Expr(e) = &arg.node {
                            walk_cst_expr_inner(e, span_map);
                        }
                    }
                }
                CstExpr::PartialCall { func, args, .. } => {
                    walk_cst_expr_inner(func, span_map);
                    for arg in args {
                        if let crate::core::cst::CstCallArgument::Expr(e) = &arg.node {
                            walk_cst_expr_inner(e, span_map);
                        }
                    }
                }
                CstExpr::MemberAccess { object, members, .. } => {
                    walk_cst_expr_inner(object, span_map);
                    for member in members {
                        span_map
                            .entry(member.node.clone())
                            .or_insert_with(Vec::new)
                            .push(cst_to_hir_span(member.span));
                    }
                }
                CstExpr::Prefix { rhs, .. } => walk_cst_expr_inner(rhs, span_map),
                CstExpr::Postfix { lhs, .. } => walk_cst_expr_inner(lhs, span_map),
                CstExpr::Infix { lhs, rhs, .. } => {
                    walk_cst_expr_inner(lhs, span_map);
                    walk_cst_expr_inner(rhs, span_map);
                }
                CstExpr::Compose { lhs, rhs, .. } => {
                    walk_cst_expr_inner(lhs, span_map);
                    walk_cst_expr_inner(rhs, span_map);
                }
                CstExpr::Loop { init_vars, body, .. } => {
                    for (var_name, _, expr) in init_vars {
                        span_map
                            .entry(var_name.node.clone())
                            .or_insert_with(Vec::new)
                            .push(cst_to_hir_span(var_name.span));
                        walk_cst_expr_inner(expr, span_map);
                    }
                    // Walk body expressions
                    for stmt in &body.node.statements {
                        if let crate::core::cst::CstStatement::Expression(expr) = &stmt.node {
                            walk_cst_expr_inner(expr, span_map);
                        }
                    }
                }
                CstExpr::StructInit { struct_name, fields, .. } => {
                    // Extract struct name
                    span_map
                        .entry(struct_name.node.clone())
                        .or_insert_with(Vec::new)
                        .push(cst_to_hir_span(struct_name.span));
                    // Extract field values (they are expressions)
                    for field in fields {
                        walk_cst_expr_inner(&field.node.value, span_map);
                    }
                }
                CstExpr::FieldAccess { object, field, .. } => {
                    walk_cst_expr_inner(object, span_map);
                    // Extract field name
                    span_map
                        .entry(field.node.clone())
                        .or_insert_with(Vec::new)
                        .push(cst_to_hir_span(field.span));
                }
                CstExpr::ArrayIndex { array, indices, .. } => {
                    walk_cst_expr_inner(array, span_map);
                    for idx_spec in indices {
                        match &idx_spec.node {
                            crate::core::cst::CstIndexSpec::Single(expr) => walk_cst_expr_inner(expr, span_map),
                            crate::core::cst::CstIndexSpec::Range { start, end, step, .. } => {
                                if let Some(expr) = start {
                                    walk_cst_expr_inner(expr, span_map);
                                }
                                if let Some(expr) = end {
                                    walk_cst_expr_inner(expr, span_map);
                                }
                                if let Some((_, expr)) = step {
                                    walk_cst_expr_inner(expr, span_map);
                                }
                            }
                            crate::core::cst::CstIndexSpec::InclusiveRange { start, end, .. } => {
                                if let Some(expr) = start {
                                    walk_cst_expr_inner(expr, span_map);
                                }
                                if let Some(expr) = end {
                                    walk_cst_expr_inner(expr, span_map);
                                }
                            }
                        }
                    }
                }
                CstExpr::Array { elements, .. } => {
                    for elem in elements {
                        walk_cst_expr_inner(elem, span_map);
                    }
                }
                CstExpr::Closure { arguments, body, .. } => {
                    // Extract closure parameter identifiers
                    for arg in arguments {
                        span_map
                            .entry(arg.node.identifier.node.clone())
                            .or_insert_with(Vec::new)
                            .push(cst_to_hir_span(arg.node.identifier.span));
                    }
                    // Walk closure body
                    match body {
                        crate::core::cst::CstClosureBody::Expression(expr) => walk_cst_expr_inner(expr, span_map),
                        crate::core::cst::CstClosureBody::Block(block) => {
                            for stmt in &block.node.statements {
                                match &stmt.node {
                                    crate::core::cst::CstStatement::Expression(expr) => walk_cst_expr_inner(expr, span_map),
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                CstExpr::Group { inner, .. } => {
                    walk_cst_expr_inner(inner, span_map);
                }
                CstExpr::Literal(_) => {
                    // Literals don't contain identifiers
                }
                _ => {
                    // Log unhandled expression types for debugging
                    log::debug!("Unhandled CstExpr variant in identifier extraction: {:?}", std::mem::discriminant(&expr.node));
                }
            }
        }

        for stmt in statements {
            match &stmt.node {
                CstStatement::Let { identifier, expression, .. } => {
                    span_map
                        .entry(identifier.node.clone())
                        .or_insert_with(Vec::new)
                        .push(cst_to_hir_span(identifier.span));
                    walk_cst_expr_inner(expression, span_map);
                }
                CstStatement::Assign { identifier, expression, .. } => {
                    span_map
                        .entry(identifier.node.clone())
                        .or_insert_with(Vec::new)
                        .push(cst_to_hir_span(identifier.span));
                    walk_cst_expr_inner(expression, span_map);
                }
                CstStatement::AssignIncrement { identifier, expression, .. } |
                CstStatement::AssignDecrement { identifier, expression, .. } => {
                    span_map
                        .entry(identifier.node.clone())
                        .or_insert_with(Vec::new)
                        .push(cst_to_hir_span(identifier.span));
                    walk_cst_expr_inner(expression, span_map);
                }
                CstStatement::Return { expression, .. } => {
                    walk_cst_expr_inner(expression, span_map);
                }
                CstStatement::Expression(expression) => {
                    walk_cst_expr_inner(expression, span_map);
                }
                CstStatement::Break { break_keyword: _, expression } => {
                    if let Some(expr) = expression {
                        walk_cst_expr_inner(expr, span_map);
                    }
                }
                CstStatement::If { arms, else_block, .. } => {
                    for (condition, block) in arms {
                        walk_cst_expr_inner(condition, span_map);
                        walk_cst_statements(&block.node.statements, span_map);
                    }
                    if let Some(else_block) = else_block {
                        walk_cst_statements(&else_block.node.statements, span_map);
                    }
                }
                CstStatement::Match { expression, cases, .. } => {
                    walk_cst_expr_inner(expression, span_map);
                    for (pattern, block) in cases {
                        if let Some(pattern) = pattern {
                            walk_cst_expr_inner(pattern, span_map);
                        }
                        walk_cst_statements(&block.node.statements, span_map);
                    }
                }
                CstStatement::While { condition, body, .. } => {
                    walk_cst_expr_inner(condition, span_map);
                    walk_cst_statements(&body.node.statements, span_map);
                }
                CstStatement::Loop { body, .. } => {
                    walk_cst_statements(&body.node.statements, span_map);
                }
                CstStatement::For { var_name, start, end, body, .. } => {
                    span_map
                        .entry(var_name.node.clone())
                        .or_insert_with(Vec::new)
                        .push(cst_to_hir_span(var_name.span));
                    walk_cst_expr_inner(start, span_map);
                    walk_cst_expr_inner(end, span_map);
                    walk_cst_statements(&body.node.statements, span_map);
                }
                CstStatement::FunctionDeclaration { body, .. } => {
                    walk_cst_statements(&body.node.statements, span_map);
                }
                _ => {}
            }
        }
    }

    // Walk through CST blocks and statements
    for block in &cst.blocks {
        for stmt in &block.node.statements {
            match &stmt.node {
                CstStatement::FunctionDeclaration { identifier, arguments, body, .. } => {
                    // Extract function name
                    span_map
                        .entry(identifier.node.clone())
                        .or_insert_with(Vec::new)
                        .push(cst_to_hir_span(identifier.span));
                    
                    // Extract function parameter identifiers
                    for arg in arguments {
                        // CstArgument has an identifier field
                        span_map
                            .entry(arg.node.identifier.node.clone())
                            .or_insert_with(Vec::new)
                            .push(cst_to_hir_span(arg.node.identifier.span));
                    }
                    
                    // Also walk the function body to find identifiers inside
                    walk_cst_statements(&body.node.statements, &mut span_map);
                }
                CstStatement::Let { identifier, expression, .. } => {
                    span_map
                        .entry(identifier.node.clone())
                        .or_insert_with(Vec::new)
                        .push(cst_to_hir_span(identifier.span));
                    walk_cst_expr(expression, &mut span_map);
                }
                CstStatement::Const { identifier, expression, .. } => {
                    span_map
                        .entry(identifier.node.clone())
                        .or_insert_with(Vec::new)
                        .push(cst_to_hir_span(identifier.span));
                    walk_cst_expr(expression, &mut span_map);
                }
                CstStatement::Assign { identifier, expression, .. } => {
                    span_map
                        .entry(identifier.node.clone())
                        .or_insert_with(Vec::new)
                        .push(cst_to_hir_span(identifier.span));
                    walk_cst_expr(expression, &mut span_map);
                }
                CstStatement::AssignIncrement { identifier, expression, .. } => {
                    span_map
                        .entry(identifier.node.clone())
                        .or_insert_with(Vec::new)
                        .push(cst_to_hir_span(identifier.span));
                    walk_cst_expr(expression, &mut span_map);
                }
                CstStatement::AssignDecrement { identifier, expression, .. } => {
                    span_map
                        .entry(identifier.node.clone())
                        .or_insert_with(Vec::new)
                        .push(cst_to_hir_span(identifier.span));
                    walk_cst_expr(expression, &mut span_map);
                }
                CstStatement::For { var_name, start, end, body, .. } => {
                    span_map
                        .entry(var_name.node.clone())
                        .or_insert_with(Vec::new)
                        .push(cst_to_hir_span(var_name.span));
                    walk_cst_expr(start, &mut span_map);
                    walk_cst_expr(end, &mut span_map);
                    walk_cst_statements(&body.node.statements, &mut span_map);
                }
                CstStatement::Mod { identifier } => {
                    span_map
                        .entry(identifier.node.clone())
                        .or_insert_with(Vec::new)
                        .push(cst_to_hir_span(identifier.span));
                }
                CstStatement::Return { expression, .. } => {
                    walk_cst_expr(expression, &mut span_map);
                }
                CstStatement::If { arms, else_block, .. } => {
                    for (condition, block) in arms {
                        walk_cst_expr(condition, &mut span_map);
                        walk_cst_statements(&block.node.statements, &mut span_map);
                    }
                    if let Some(else_block) = else_block {
                        walk_cst_statements(&else_block.node.statements, &mut span_map);
                    }
                }
                CstStatement::Match { expression, cases, .. } => {
                    walk_cst_expr(expression, &mut span_map);
                    for (pattern, block) in cases {
                        if let Some(pattern) = pattern {
                            walk_cst_expr(pattern, &mut span_map);
                        }
                        walk_cst_statements(&block.node.statements, &mut span_map);
                    }
                }
                CstStatement::While { condition, body, .. } => {
                    walk_cst_expr(condition, &mut span_map);
                    walk_cst_statements(&body.node.statements, &mut span_map);
                }
                CstStatement::Loop { body, .. } => {
                    walk_cst_statements(&body.node.statements, &mut span_map);
                }
                CstStatement::Expression(expression) => {
                    walk_cst_expr(expression, &mut span_map);
                }
                CstStatement::Break { break_keyword: _, expression } => {
                    if let Some(expr) = expression {
                        walk_cst_expr(expr, &mut span_map);
                    }
                }
                CstStatement::Continue { continue_keyword: _ } => {}
                CstStatement::Use { selector, path, .. } => {
                    // Extract identifiers from use statement selector
                    match &selector.node {
                        crate::core::cst::CstImportSelector::Single(name) => {
                            span_map
                                .entry(name.node.clone())
                                .or_insert_with(Vec::new)
                                .push(cst_to_hir_span(name.span));
                        }
                        crate::core::cst::CstImportSelector::Multiple(names) => {
                            for name in names {
                                span_map
                                    .entry(name.node.clone())
                                    .or_insert_with(Vec::new)
                                    .push(cst_to_hir_span(name.span));
                            }
                        }
                        crate::core::cst::CstImportSelector::Wildcard(_) => {
                            // Wildcard import doesn't have specific identifiers to extract
                        }
                    }
                    
                    // Extract module path identifiers (e.g., "std" in "use print from std")
                    for path_segment in path {
                        span_map
                            .entry(path_segment.node.clone())
                            .or_insert_with(Vec::new)
                            .push(cst_to_hir_span(path_segment.span));
                    }
                }
                CstStatement::Struct { name, fields, .. } => {
                    // Extract struct name
                    span_map
                        .entry(name.node.clone())
                        .or_insert_with(Vec::new)
                        .push(cst_to_hir_span(name.span));
                    
                    // Extract struct field names
                    for field in fields {
                        span_map
                            .entry(field.node.name.node.clone())
                            .or_insert_with(Vec::new)
                            .push(cst_to_hir_span(field.node.name.span));
                    }
                }
            }
        }
    }

    span_map
}

/// Extract identifier spans from AST by walking the tree and finding identifiers in source text.
/// DEPRECATED: Use extract_identifier_spans_from_cst instead, which has accurate spans from the parser.
fn extract_identifier_spans(
    ast: &crate::core::ast::Program,
    source: &str,
) -> HashMap<String, Vec<Span>> {
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
                        if line_text[..col_in_line.min(line_text.len())]
                            .find("//")
                            .is_some()
                        {
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
                    if let Some(span) = find_identifier_span(&identifier.name, current_pos) {
                        span_map
                            .entry(identifier.name.clone())
                            .or_insert_with(Vec::new)
                            .push(span);
                        current_pos = span.end;
                    }
                }
                crate::core::ast::Statement::Let { identifier, .. } => {
                    if let Some(span) = find_identifier_span(&identifier.name, current_pos) {
                        span_map
                            .entry(identifier.name.clone())
                            .or_insert_with(Vec::new)
                            .push(span);
                        current_pos = span.end;
                    }
                }
                crate::core::ast::Statement::Const { identifier, .. } => {
                    if let Some(span) = find_identifier_span(&identifier.name, current_pos) {
                        span_map
                            .entry(identifier.name.clone())
                            .or_insert_with(Vec::new)
                            .push(span);
                        current_pos = span.end;
                    }
                }
                crate::core::ast::Statement::Assign { identifier, .. }
                | crate::core::ast::Statement::AssignIncrement { identifier, .. }
                | crate::core::ast::Statement::AssignDecrement { identifier, .. } => {
                    if let Some(span) = find_identifier_span(identifier, current_pos) {
                        span_map
                            .entry(identifier.clone())
                            .or_insert_with(Vec::new)
                            .push(span);
                        current_pos = span.end;
                    }
                }
                crate::core::ast::Statement::For { var_name, .. } => {
                    if let Some(span) = find_identifier_span(var_name, current_pos) {
                        span_map
                            .entry(var_name.clone())
                            .or_insert_with(Vec::new)
                            .push(span);
                        current_pos = span.end;
                    }
                }
                crate::core::ast::Statement::Mod { identifier } => {
                    if let Some(span) = find_identifier_span(identifier, current_pos) {
                        span_map
                            .entry(identifier.clone())
                            .or_insert_with(Vec::new)
                            .push(span);
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
                        if line_text[..col_in_line.min(line_text.len())]
                            .find("//")
                            .is_some()
                        {
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
                if let Some(span) = find_identifier_span(&name.name, *current_pos) {
                    span_map
                        .entry(name.name.clone())
                        .or_insert_with(Vec::new)
                        .push(span);
                    *current_pos = span.end;
                }
            }
            Expression::FunctionCall {
                callee, arguments, ..
            } => {
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
                if let Some(span) = find_identifier_span(&member.name, *current_pos) {
                    span_map
                        .entry(member.name.clone())
                        .or_insert_with(Vec::new)
                        .push(span);
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
                        span_map
                            .entry(var_name.clone())
                            .or_insert_with(Vec::new)
                            .push(span);
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
        Statement::Let { expression, .. }
        | Statement::Const { expression, .. }
        | Statement::Assign { expression, .. }
        | Statement::AssignIncrement { expression, .. }
        | Statement::AssignDecrement { expression, .. }
        | Statement::Return { expression, .. }
        | Statement::Expression(expression) => {
            walk_expr(
                expression,
                source,
                span_map,
                current_pos,
                &find_identifier_span,
            );
        }
        Statement::If {
            arms, else_block, ..
        } => {
            for (expr, _) in arms {
                walk_expr(expr, source, span_map, current_pos, &find_identifier_span);
            }
            if let Some(block) = else_block {
                for stmt in &block.statements {
                    walk_expression_for_spans(stmt, source, span_map, current_pos);
                }
            }
        }
        Statement::Match {
            expression, cases, ..
        } => {
            walk_expr(
                expression,
                source,
                span_map,
                current_pos,
                &find_identifier_span,
            );
            for (opt_expr, block) in cases {
                if let Some(expr) = opt_expr {
                    walk_expr(expr, source, span_map, current_pos, &find_identifier_span);
                }
                for stmt in &block.statements {
                    walk_expression_for_spans(stmt, source, span_map, current_pos);
                }
            }
        }
        Statement::While {
            condition, body, ..
        } => {
            walk_expr(
                condition,
                source,
                span_map,
                current_pos,
                &find_identifier_span,
            );
            for stmt in &body.statements {
                walk_expression_for_spans(stmt, source, span_map, current_pos);
            }
        }
        Statement::Loop { body, .. }
        | Statement::For { body, .. }
        | Statement::FunctionDeclaration { body, .. } => {
            for stmt in &body.statements {
                walk_expression_for_spans(stmt, source, span_map, current_pos);
            }
        }
        _ => {}
    }
}
