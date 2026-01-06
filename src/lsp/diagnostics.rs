use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Range};

use crate::core::hir_lowering::{CompilerState, HirExpression, HirError};
use super::text_utils;
use super::hover;


/// Format a HIR error as a user-friendly message.
pub fn format_hir_error(e: &HirError) -> String {
    match e {
        HirError::UnknownVariable(msg) => msg.clone(),
        HirError::VariableAlreadyDeclared(msg) => msg.clone(),
        HirError::TypeMismatch { variable, expected, actual } => {
            format!(
                "Type mismatch for variable '{}': expected {}, got {}",
                variable,
                hover::format_value_kind(expected),
                hover::format_value_kind(actual)
            )
        }
        HirError::BinaryOpTypeError { operator, lhs_type, rhs_type, expected } => {
            format!(
                "{} operation requires {}, but got {} and {}",
                operator,
                expected,
                hover::format_value_kind(lhs_type),
                hover::format_value_kind(rhs_type)
            )
        }
        HirError::TypeError(msg) => msg.clone(),
        HirError::NotImplemented => "Not implemented".to_string(),
    }
}

/// Find the location of an error using compiler state (AST/HIR) instead of text parsing.
/// Uses symbol table and semantic items to find exact spans.
pub fn find_error_location(error: &HirError, state: &CompilerState) -> (usize, usize) {
    let line_index = match &state.line_index {
        Some(idx) => idx,
        None => return (0, 0), // No line index available
    };
    
    match error {
        HirError::BinaryOpTypeError { .. } => {
            // Find operator in semantic items
            for item in &state.semantic_items {
                if matches!(item.kind, crate::core::hir_lowering::SemanticItemKind::Operator) {
                    // Use the first operator we find
                    // TODO: Match by operator string when semantic items store operator text
                    let (line, col) = line_index.lookup(item.span.start);
                    return (line as usize, col as usize);
                }
            }
        }
        HirError::TypeMismatch { variable, .. } => {
            // Use symbol table to find the variable's definition span
            let symbols = state.symbols.find_by_name(variable);
            if let Some(symbol) = symbols.first() {
                if let Some(span) = symbol.defined_at {
                    let (line, col) = line_index.lookup(span.start);
                    return (line as usize, col as usize);
                }
            }
            // Fallback: find variable in semantic items (for variables not in symbol table)
            for item in &state.semantic_items {
                if matches!(item.kind, crate::core::hir_lowering::SemanticItemKind::Variable) {
                    // We'd need to match by name, but semantic items don't store names
                    // This is a limitation - we should enhance semantic items
                }
            }
        }
        HirError::UnknownVariable(msg) => {
            // Extract variable name from error message
            let _var_name = text_utils::extract_variable_name_from_message(msg);
            // Use symbol table to find where this variable is used (not defined)
            // Look in semantic items for variable references
            for item in &state.semantic_items {
                if matches!(item.kind, crate::core::hir_lowering::SemanticItemKind::Variable) {
                    // TODO: Match by variable name when semantic items store names
                    // For now, return the first variable span as a fallback
                    let (line, col) = line_index.lookup(item.span.start);
                    return (line as usize, col as usize);
                }
            }
        }
        HirError::VariableAlreadyDeclared(msg) => {
            // Extract variable name from error message
            let var_name = text_utils::extract_variable_name_from_message(msg);
            // Use symbol table to find the variable's definition
            let symbols = state.symbols.find_by_name(&var_name);
            if let Some(symbol) = symbols.first() {
                if let Some(span) = symbol.defined_at {
                    let (line, col) = line_index.lookup(span.start);
                    return (line as usize, col as usize);
                }
            }
        }
        HirError::TypeError(msg) => {
            // Handle module-related errors by walking the AST
            if msg.contains("Module '") && msg.contains("' not found") {
                // Extract module name from error message: "Module 'utils' not found"
                if let Some(start) = msg.find("Module '") {
                    let module_start = start + 8; // "Module '" is 8 chars
                    if let Some(end) = msg[module_start..].find("' not found") {
                        // Safe string slicing - use get() to handle multi-byte characters
                        let module_name = match msg.get(module_start..module_start + end) {
                            Some(s) => s,
                            None => return (0, 0), // Skip if invalid char boundary
                        };
                        
                        // Check if this file IS the module by walking AST
                        let is_current_file_module = state.ast.blocks.iter().any(|block| {
                            block.statements.iter().any(|stmt| {
                                if let crate::core::ast::Statement::Mod { identifier } = stmt {
                                    identifier == module_name
                                } else {
                                    false
                                }
                            })
                        });
                        
                        if is_current_file_module {
                            // This file IS the module, so the error is likely from usage elsewhere
                            // Look for module member access in semantic items
                            for item in &state.semantic_items {
                                if matches!(item.kind, crate::core::hir_lowering::SemanticItemKind::Module) {
                                    // Check if this module reference matches (we'd need to store the name)
                                    // For now, return first module reference
                                    let (line, col) = line_index.lookup(item.span.start);
                                    return (line as usize, col as usize);
                                }
                            }
                            return (0, 0); // Suppress if we can't find usage
                        }
                        
                        // This file is NOT the module, so look for "use ... from <module_name>" in AST
                        for block in &state.ast.blocks {
                            for stmt in &block.statements {
                                if let crate::core::ast::Statement::Use { path, .. } = stmt {
                                    let module_path = path.join(".");
                                    if module_path == module_name {
                                        // Find the span for this use statement in semantic items
                                        for item in &state.semantic_items {
                                            if matches!(item.kind, crate::core::hir_lowering::SemanticItemKind::Module) {
                                                // We'd need to match by name, but for now use first match
                                                let (line, col) = line_index.lookup(item.span.start);
                                                return (line as usize, col as usize);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // For other TypeError messages, try to find in symbol table or semantic items
            // Look for patterns like "Function or constant 'X' not found" or "Member 'X' not found"
            if let Some(start) = msg.find('\'') {
                let name_start = start + 1;
                if let Some(end) = msg[name_start..].find('\'') {
                    // Safe string slicing - use get() to handle multi-byte characters
                    let name = match msg.get(name_start..name_start + end) {
                        Some(s) => s,
                        None => return (0, 0), // Skip if invalid char boundary
                    };
                    // Try symbol table first
                    let symbols = state.symbols.find_by_name(name);
                    if let Some(symbol) = symbols.first() {
                        if let Some(span) = symbol.defined_at {
                            let (line, col) = line_index.lookup(span.start);
                            return (line as usize, col as usize);
                        }
                    }
                    // Fallback: look in semantic items (for references, not definitions)
                    for item in &state.semantic_items {
                        if matches!(item.kind, crate::core::hir_lowering::SemanticItemKind::Function) {
                            // We'd need to match by name, but semantic items don't store names
                            // This is a limitation
                        }
                    }
                }
            }
        }
        _ => {
            // For other errors, don't fall back to arbitrary line - return (0, 0) explicitly
            // This prevents unrelated errors from appearing at line 0 or line 4
        }
    }
    (0, 0)
}

/// Improve parse error messages with more helpful suggestions.
pub fn improve_parse_error_message(text: &str, line: usize, error_msg: &str) -> String {
    if error_msg.contains("let_statement") || error_msg.contains("let") {
        let lines: Vec<&str> = text.lines().collect();
        if line < lines.len() {
            let error_line = lines[line];
            if error_line.contains("let ") && !error_line.contains(":") {
                return "Parse error in let statement. Type annotation is optional: `let identifier [: type] = expression`".to_string();
            } else if error_line.contains("let ") && error_line.contains(":") && !error_line.contains("=") {
                return "Missing assignment in let statement. Expected: `let identifier : type = expression`".to_string();
            } else {
                return format!("Parse error in let statement: {}", error_msg);
            }
        }
    } else if error_msg.contains("function_statement") || error_msg.contains("argument") {
        let lines: Vec<&str> = text.lines().collect();
        if line < lines.len() {
            let error_line = lines[line];
            if error_line.contains("fn ") && error_line.contains('(') {
                if let Some(args_start) = error_line.find('(') {
                    // Safe string slicing - use get() to handle multi-byte characters
                    let args_part = match error_line.get(args_start..) {
                        Some(s) => s,
                        None => return format!("Parse error: {}", error_msg), // Skip if invalid char boundary
                    };
                    if args_part.matches(':').count() == 0 && args_part.contains(|c: char| c.is_alphabetic()) {
                        return "Missing type annotation in function argument. Expected: `fn name(arg: type, ...)`".to_string();
                    } else {
                        return format!("Parse error in function arguments: {}", error_msg);
                    }
                }
            }
        }
    } else if error_msg.contains("if_statement") || error_msg.contains("if") {
        let lines: Vec<&str> = text.lines().collect();
        if line < lines.len() {
            let error_line = lines[line];
            if error_line.contains("if ") {
                // Check if there's a missing expression or brace
                if error_line.contains("if ") && !error_line.contains('{') {
                    // Check if there's an expression after "if"
                    let after_if = error_line.split("if ").nth(1).unwrap_or("").trim();
                    if after_if.is_empty() {
                        return "Parse error in if statement. Expected: `if expression {{ ... }}`. Parentheses are optional: `if condition {{ ... }}` or `if (condition) {{ ... }}`".to_string();
                    } else if !after_if.contains('{') {
                        return "Parse error in if statement. Missing opening brace `{{`. Expected: `if expression {{ ... }}`".to_string();
                    }
                } else if error_msg.contains("Empty expression") {
                    return "Parse error in if statement. Missing condition expression. Expected: `if condition { ... }`".to_string();
                } else {
                    return format!("Parse error in if statement: {}. Note: Parentheses are optional: `if condition {{ ... }}` or `if (condition) {{ ... }}`", error_msg);
                }
            }
        }
    } else if error_msg.contains("while_statement") || error_msg.contains("while") {
        let lines: Vec<&str> = text.lines().collect();
        if line < lines.len() {
            let error_line = lines[line];
            if error_line.contains("while ") {
                // Check if there's a missing expression or brace
                if error_line.contains("while ") && !error_line.contains('{') {
                    // Check if there's an expression after "while"
                    let after_while = error_line.split("while ").nth(1).unwrap_or("").trim();
                    if after_while.is_empty() {
                        return "Parse error in while statement. Expected: `while condition {{ ... }}`. Example: `while i < 5 {{ ... }}`".to_string();
                    } else if !after_while.contains('{') {
                        return "Parse error in while statement. Missing opening brace `{{`. Expected: `while condition {{ ... }}`".to_string();
                    }
                } else {
                    return format!("Parse error in while statement: {}. Expected: `while condition {{ ... }}`", error_msg);
                }
            }
        }
    } else if error_msg.contains("for_statement") || error_msg.contains("for ") {
        let lines: Vec<&str> = text.lines().collect();
        if line < lines.len() {
            let error_line = lines[line];
            if error_line.contains("for ") {
                // Check if there's a missing "in", "..", or brace
                if error_line.contains("for ") && !error_line.contains("..") {
                    return "Parse error in for statement. Expected: `for x in start..end {{ ... }}`. Example: `for i in 0..5 {{ ... }}`".to_string();
                } else if error_line.contains("for ") && !error_line.contains(" in ") && !error_line.contains(" in{") {
                    return "Parse error in for statement. Missing `in`. Expected: `for x in start..end {{ ... }}`".to_string();
                } else if error_line.contains("for ") && !error_line.contains('{') {
                    return "Parse error in for statement. Missing opening brace `{{`. Expected: `for x in start..end {{ ... }}`".to_string();
                } else {
                    return format!("Parse error in for statement: {}. Expected: `for x in start..end {{ ... }}`", error_msg);
                }
            }
        }
    }
    format!("Parse error: {}", error_msg)
}

/// Create a diagnostic from a range and message.
pub fn create_diagnostic(range: Range, message: String) -> Diagnostic {
    Diagnostic {
        range,
        severity: Some(DiagnosticSeverity::ERROR),
        code: None,
        code_description: None,
        source: Some("CantaLoop".to_string()),
        message,
        related_information: None,
        tags: None,
        data: None,
    }
}

/// Find unused variables in the compiler state.
pub fn find_unused_variables(state: &CompilerState) -> Vec<(String, usize, usize)> {
    let mut unused = Vec::new();
    let hir = &state.hir;
    
    // Collect all declared variables (from let statements)
    let mut declared_vars: std::collections::HashMap<u32, String> = std::collections::HashMap::new();
    
    // Check top-level blocks
    for block in &hir.blocks {
        for stmt in &block.statements {
            if let crate::core::hir_lowering::HirStmt::Assign { slot, .. } = stmt {
                // Find the variable name for this slot
                for scope in &hir.scopes.scopes {
                    if let Some(var) = scope.vars.iter().find(|v| v.id == *slot) {
                        declared_vars.insert(*slot, var.name.clone());
                        break;
                    }
                }
            }
        }
    }
    
    // Check function bodies
    for (_, func) in &hir.functions {
        for stmt in &func.definition.body.statements {
            if let crate::core::hir_lowering::HirStmt::Assign { slot, .. } = stmt {
                // Find the variable name for this slot
                for scope in &hir.scopes.scopes {
                    if let Some(var) = scope.vars.iter().find(|v| v.id == *slot) {
                        declared_vars.insert(*slot, var.name.clone());
                        break;
                    }
                }
            }
        }
    }
    
    // Collect all used variables (from Identifier expressions)
    let mut used_vars: std::collections::HashSet<u32> = std::collections::HashSet::new();
    
    // Check top-level blocks
    for block in &hir.blocks {
        collect_used_vars_from_block(block, &hir, &mut used_vars);
    }
    
    // Check function bodies
    for (_, func) in &hir.functions {
        collect_used_vars_from_block(&func.definition.body, &hir, &mut used_vars);
    }
    
    // Find unused variables
    for (var_id, var_name) in &declared_vars {
        if !used_vars.contains(var_id) {
            unused.push((var_name.clone(), 0, 0));
        }
    }
    
    unused
}

fn collect_used_vars_from_block(
    block: &crate::core::hir_lowering::HirBlock,
    hir: &crate::core::hir_lowering::HirAst,
    used_vars: &mut std::collections::HashSet<u32>,
) {
    for stmt in &block.statements {
        collect_used_vars_from_stmt(stmt, hir, used_vars);
    }
}

fn collect_used_vars_from_stmt(
    stmt: &crate::core::hir_lowering::HirStmt,
    hir: &crate::core::hir_lowering::HirAst,
    used_vars: &mut std::collections::HashSet<u32>,
) {
    match stmt {
        crate::core::hir_lowering::HirStmt::Assign { value, .. } => {
            collect_used_vars_from_expr(value, hir, used_vars);
        }
        crate::core::hir_lowering::HirStmt::AssignIncrement { value, .. } => {
            collect_used_vars_from_expr(value, hir, used_vars);
        }
        crate::core::hir_lowering::HirStmt::AssignDecrement { value, .. } => {
            collect_used_vars_from_expr(value, hir, used_vars);
        }
        crate::core::hir_lowering::HirStmt::If { arms, else_block } => {
            for (condition, block) in arms {
                collect_used_vars_from_expr(condition, hir, used_vars);
                collect_used_vars_from_block(block, hir, used_vars);
            }
            collect_used_vars_from_block(else_block, hir, used_vars);
        }
        crate::core::hir_lowering::HirStmt::Match { expression, cases } => {
            collect_used_vars_from_expr(expression, hir, used_vars);
            for (pattern, block) in cases {
                if let Some(pattern_expr) = pattern {
                    collect_used_vars_from_expr(pattern_expr, hir, used_vars);
                }
                collect_used_vars_from_block(block, hir, used_vars);
            }
        }
        crate::core::hir_lowering::HirStmt::Return { value } => {
            collect_used_vars_from_expr(value, hir, used_vars);
        }
        crate::core::hir_lowering::HirStmt::Loop { body, .. } => {
            collect_used_vars_from_block(body, hir, used_vars);
        }
        crate::core::hir_lowering::HirStmt::Break { value } => {
            if let Some(expr) = value {
                collect_used_vars_from_expr(expr, hir, used_vars);
            }
        }
        crate::core::hir_lowering::HirStmt::Continue => {
            // Continue doesn't use any variables
        }
        crate::core::hir_lowering::HirStmt::Expression(expr) => {
            collect_used_vars_from_expr(expr, hir, used_vars);
        }
        crate::core::hir_lowering::HirStmt::Nop => {
            // No-op statement (used for use statements which are compile-time only)
        }
    }
}

fn collect_used_vars_from_expr(
    expr: &HirExpression,
    hir: &crate::core::hir_lowering::HirAst,
    used_vars: &mut std::collections::HashSet<u32>,
) {
    match expr {
        HirExpression::Identifier(var_id) => {
            used_vars.insert(*var_id);
        }
        HirExpression::Binary { lhs, rhs, .. } => {
            collect_used_vars_from_expr(lhs, hir, used_vars);
            collect_used_vars_from_expr(rhs, hir, used_vars);
        }
        HirExpression::Unary { operand, .. } => {
            collect_used_vars_from_expr(operand, hir, used_vars);
        }
        HirExpression::FunctionCall { args, .. } => {
            for arg in args {
                collect_used_vars_from_expr(arg, hir, used_vars);
            }
        }
        HirExpression::PostfixInvoke { operand, args } => {
            collect_used_vars_from_expr(operand, hir, used_vars);
            if let Some(arg_list) = args {
                for arg in arg_list {
                    collect_used_vars_from_expr(arg, hir, used_vars);
                }
            }
        }
        HirExpression::ComposeThunk { first, second } => {
            collect_used_vars_from_expr(first, hir, used_vars);
            collect_used_vars_from_expr(second, hir, used_vars);
        }
        _ => {}
    }
}

/// Find nested invoke patterns like: identifier(expression!)! or identifier!(expression!)!
/// Returns: Vec of (line, col, length) for each problematic pattern
pub fn find_nested_invoke_patterns(text: &str) -> Vec<(usize, usize, usize)> {
    let mut issues = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    
    // Functions that take regular values (not thunks) as arguments - these are exempt from the warning
    let value_functions = ["print"];
    
    for (line_num, line) in lines.iter().enumerate() {
        let line_str = line.trim();
        // Skip empty lines and comments
        if line_str.is_empty() || line_str.starts_with("//") {
            continue;
        }
        
        // Use char_indices to work with char boundaries instead of byte boundaries
        let char_indices: Vec<(usize, char)> = line.char_indices().collect();
        let mut char_pos = 0;
        
        while char_pos < char_indices.len() {
            let (byte_pos, ch) = char_indices[char_pos];
            
            // Skip whitespace
            if ch.is_whitespace() {
                char_pos += 1;
                continue;
            }
            
            // Skip comments
            if ch == '/' && char_pos + 1 < char_indices.len() {
                let (_, next_ch) = char_indices[char_pos + 1];
                if next_ch == '/' {
                    break; // Rest of line is comment
                }
            }
            
            // Look for identifier
            if !ch.is_alphanumeric() && ch != '_' {
                char_pos += 1;
                continue;
            }
            
            let ident_start_byte = byte_pos;
            let mut ident_end_char_pos = char_pos;
            
            // Find the end of the identifier
            while ident_end_char_pos < char_indices.len() {
                let (_, ch) = char_indices[ident_end_char_pos];
                if ch.is_alphanumeric() || ch == '_' {
                    ident_end_char_pos += 1;
                } else {
                    break;
                }
            }
            
            if ident_end_char_pos <= char_pos {
                char_pos += 1;
                continue;
            }
            
            // Get the byte position of the end of the identifier
            let ident_end_byte = if ident_end_char_pos < char_indices.len() {
                char_indices[ident_end_char_pos].0
            } else {
                line.len()
            };
            
            // Safe string slicing - get() returns None if indices are invalid
            let identifier = match line.get(ident_start_byte..ident_end_byte) {
                Some(s) => s,
                None => {
                    char_pos += 1;
                    continue;
                }
            };
            
            // Skip whitespace after identifier using char-based indexing
            let mut after_ident_char_pos = ident_end_char_pos;
            while after_ident_char_pos < char_indices.len() {
                let (_, ch) = char_indices[after_ident_char_pos];
                if ch.is_whitespace() {
                    after_ident_char_pos += 1;
                } else {
                    break;
                }
            }
            
            if after_ident_char_pos >= char_indices.len() {
                char_pos = ident_end_char_pos + 1;
                continue;
            }
            
            // Check for pattern: identifier!(expression!)!
            let (_, after_ident_ch) = char_indices[after_ident_char_pos];
            if after_ident_ch == '!' {
                let bang_char_pos = after_ident_char_pos;
                let mut after_bang_char_pos = bang_char_pos + 1;
                // Skip whitespace after !
                while after_bang_char_pos < char_indices.len() {
                    let (_, ch) = char_indices[after_bang_char_pos];
                    if ch.is_whitespace() {
                        after_bang_char_pos += 1;
                    } else {
                        break;
                    }
                }
                
                if after_bang_char_pos < char_indices.len() {
                    let (_, after_bang_ch) = char_indices[after_bang_char_pos];
                    if after_bang_ch == '(' {
                        // Found identifier!( - now check if there's a nested ! pattern
                        let paren_start_char_pos = after_bang_char_pos;
                        let mut paren_count = 1;
                        let mut check_char_pos = paren_start_char_pos + 1;
                        let mut found_inner_bang = false;
                        
                        // Scan through the function call arguments using char-based indexing
                        while check_char_pos < char_indices.len() && paren_count > 0 {
                            let (_, ch) = char_indices[check_char_pos];
                            
                            if ch == '(' {
                                paren_count += 1;
                            } else if ch == ')' {
                                paren_count -= 1;
                                if paren_count == 0 {
                                    // Found closing paren - check if there's a ! after it
                                    let after_paren_char_pos = check_char_pos + 1;
                                    // Skip whitespace after closing paren
                                    let mut after_paren_char_pos_skip = after_paren_char_pos;
                                    while after_paren_char_pos_skip < char_indices.len() {
                                        let (_, ch) = char_indices[after_paren_char_pos_skip];
                                        if ch.is_whitespace() {
                                            after_paren_char_pos_skip += 1;
                                        } else {
                                            break;
                                        }
                                    }
                                    
                                    if after_paren_char_pos_skip < char_indices.len() {
                                        let (after_paren_byte, after_paren_ch) = char_indices[after_paren_char_pos_skip];
                                        if after_paren_ch == '!' {
                                            // Found pattern: identifier!(...)! - this is problematic
                                            // Skip if this function takes regular values (not thunks) as arguments
                                            if !value_functions.iter().any(|&name| identifier == name) {
                                                // Check if there was a ! inside the arguments
                                                if found_inner_bang {
                                                    // This is a nested ! pattern: identifier!(expression!)! or identifier!(identifier!(...))!
                                                    // Report the outer ! position (convert byte pos to column)
                                                    let (_, col) = text_utils::byte_position_to_line_col(text, after_paren_byte);
                                                    issues.push((line_num, col, 1));
                                                } else {
                                                    // Even without inner !, identifier!(...)! is confusing
                                                    // Report it as a warning
                                                    let (_, col) = text_utils::byte_position_to_line_col(text, after_paren_byte);
                                                    issues.push((line_num, col, 1));
                                                }
                                            }
                                        }
                                    }
                                    char_pos = check_char_pos + 1;
                                    break;
                                }
                            } else if ch == '!' && paren_count == 1 {
                                // Found ! inside the function call arguments (at the top level)
                                // Check if it's followed by ) or whitespace then )
                                let mut after_inner_bang_char_pos = check_char_pos + 1;
                                while after_inner_bang_char_pos < char_indices.len() {
                                    let (_, ch) = char_indices[after_inner_bang_char_pos];
                                    if ch.is_whitespace() {
                                        after_inner_bang_char_pos += 1;
                                    } else {
                                        break;
                                    }
                                }
                                if after_inner_bang_char_pos < char_indices.len() {
                                    let (_, ch) = char_indices[after_inner_bang_char_pos];
                                    if ch == ')' {
                                        // Found pattern like: identifier!(expression!) - inner bang before closing paren
                                        found_inner_bang = true;
                                    }
                                }
                            } else if (ch.is_alphanumeric() || ch == '_') && paren_count == 1 {
                                // Check for nested identifier!(...) pattern inside arguments
                                let nested_ident_char_start = check_char_pos;
                                let mut nested_ident_char_end = nested_ident_char_start;
                                while nested_ident_char_end < char_indices.len() {
                                    let (_, nested_ch) = char_indices[nested_ident_char_end];
                                    if nested_ch.is_alphanumeric() || nested_ch == '_' {
                                        nested_ident_char_end += 1;
                                    } else {
                                        break;
                                    }
                                }
                                
                                if nested_ident_char_end > nested_ident_char_start {
                                    // Skip whitespace after identifier
                                    let mut after_nested_ident_char_pos = nested_ident_char_end;
                                    while after_nested_ident_char_pos < char_indices.len() {
                                        let (_, ch) = char_indices[after_nested_ident_char_pos];
                                        if ch.is_whitespace() {
                                            after_nested_ident_char_pos += 1;
                                        } else {
                                            break;
                                        }
                                    }
                                    
                                    // Check if identifier is followed by ! then (
                                    if after_nested_ident_char_pos < char_indices.len() {
                                        let (_, ch) = char_indices[after_nested_ident_char_pos];
                                        if ch == '!' {
                                            // Found nested identifier!( pattern - mark as problematic
                                            found_inner_bang = true;
                                        }
                                    }
                                }
                            }
                            
                            check_char_pos += 1;
                        }
                        
                        if paren_count > 0 {
                            // Unclosed paren, skip this
                            char_pos = ident_end_char_pos + 1;
                        }
                        continue;
                    }
                }
            }
            
            // Check for pattern: identifier(expression!)! (original pattern)
            // Check if we have a valid identifier followed by (
            if after_ident_char_pos < char_indices.len() {
                let (_, after_ident_ch) = char_indices[after_ident_char_pos];
                if after_ident_ch == '(' {
                    // Found identifier( - now check if there's a nested ! pattern
                    let paren_start_char_pos = after_ident_char_pos;
                    let mut paren_count = 1;
                    let mut check_char_pos = paren_start_char_pos + 1;
                    let mut found_inner_bang = false;
                    
                    // Scan through the function call arguments using char-based indexing
                    while check_char_pos < char_indices.len() && paren_count > 0 {
                        let (_, ch) = char_indices[check_char_pos];
                        
                        if ch == '(' {
                            paren_count += 1;
                        } else if ch == ')' {
                            paren_count -= 1;
                            if paren_count == 0 {
                                // Found closing paren - check if there's a ! after it
                                let after_paren_char_pos = check_char_pos + 1;
                                // Skip whitespace after closing paren
                                let mut after_paren_char_pos_skip = after_paren_char_pos;
                                while after_paren_char_pos_skip < char_indices.len() {
                                    let (_, ch) = char_indices[after_paren_char_pos_skip];
                                    if ch.is_whitespace() {
                                        after_paren_char_pos_skip += 1;
                                    } else {
                                        break;
                                    }
                                }
                                
                                if after_paren_char_pos_skip < char_indices.len() {
                                    let (after_paren_byte, after_paren_ch) = char_indices[after_paren_char_pos_skip];
                                    if after_paren_ch == '!' {
                                        // Found pattern: identifier(...)!
                                        // Skip if this function takes regular values (not thunks) as arguments
                                        if !value_functions.iter().any(|&name| identifier == name) {
                                            // Check if there was a ! inside the arguments
                                            if found_inner_bang {
                                                // This is a nested ! pattern: identifier(expression!)! or identifier(identifier!(...))!
                                                // Report the outer ! position (convert byte pos to column)
                                                let (_, col) = text_utils::byte_position_to_line_col(text, after_paren_byte);
                                                issues.push((line_num, col, 1));
                                            }
                                        }
                                    }
                                }
                                char_pos = check_char_pos + 1;
                                break;
                            }
                        } else if ch == '!' && paren_count == 1 {
                            // Found ! inside the function call arguments (at the top level)
                            // Check if it's followed by ) or whitespace then )
                            let mut after_bang_char_pos = check_char_pos + 1;
                            while after_bang_char_pos < char_indices.len() {
                                let (_, ch) = char_indices[after_bang_char_pos];
                                if ch.is_whitespace() {
                                    after_bang_char_pos += 1;
                                } else {
                                    break;
                                }
                            }
                            if after_bang_char_pos < char_indices.len() {
                                let (_, ch) = char_indices[after_bang_char_pos];
                                if ch == ')' {
                                    // Found pattern like: identifier(expression!) - inner bang before closing paren
                                    found_inner_bang = true;
                                }
                            }
                        } else if (ch.is_alphanumeric() || ch == '_') && paren_count == 1 {
                            // Check for nested identifier!(...) pattern inside arguments
                            let nested_ident_char_start = check_char_pos;
                            let mut nested_ident_char_end = nested_ident_char_start;
                            while nested_ident_char_end < char_indices.len() {
                                let (_, nested_ch) = char_indices[nested_ident_char_end];
                                if nested_ch.is_alphanumeric() || nested_ch == '_' {
                                    nested_ident_char_end += 1;
                                } else {
                                    break;
                                }
                            }
                            
                            if nested_ident_char_end > nested_ident_char_start {
                                // Skip whitespace after identifier
                                let mut after_nested_ident_char_pos = nested_ident_char_end;
                                while after_nested_ident_char_pos < char_indices.len() {
                                    let (_, ch) = char_indices[after_nested_ident_char_pos];
                                    if ch.is_whitespace() {
                                        after_nested_ident_char_pos += 1;
                                    } else {
                                        break;
                                    }
                                }
                                
                                // Check if identifier is followed by ! then (
                                if after_nested_ident_char_pos < char_indices.len() {
                                    let (_, ch) = char_indices[after_nested_ident_char_pos];
                                    if ch == '!' {
                                        // Found nested identifier!( pattern - mark as problematic
                                        found_inner_bang = true;
                                    }
                                }
                            }
                        }
                        
                        check_char_pos += 1;
                    }
                    
                    if paren_count > 0 {
                        // Unclosed paren, skip this
                        char_pos = ident_end_char_pos + 1;
                    }
                }
            } else {
                char_pos += 1;
            }
        }
    }
    
    issues
}

