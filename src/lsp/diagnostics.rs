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

/// Find the location of an error in the source text.
pub fn find_error_location(text: &str, error: &HirError) -> (usize, usize) {
    let lines: Vec<&str> = text.lines().collect();
    
    match error {
        HirError::BinaryOpTypeError { operator, .. } => {
            for (line_num, line) in lines.iter().enumerate() {
                if let Some(pos) = line.find(operator) {
                    return (line_num, pos);
                }
            }
        }
        HirError::TypeMismatch { variable, .. } => {
            // Find the variable name in a let statement (after "let" keyword)
            // This ensures we highlight the variable declaration, not just any occurrence
            for (line_num, line) in lines.iter().enumerate() {
                // Look for "let variable" or "let variable:" pattern with word boundaries
                let let_pattern = format!("let {}", variable);
                if let Some(pos) = line.find(&let_pattern) {
                    // Check word boundary after variable name
                    let after_pos = pos + let_pattern.len();
                    let is_word_boundary = after_pos >= line.len() || {
                        let ch = line.chars().nth(after_pos);
                        ch.map_or(true, |c| !c.is_alphanumeric() && c != '_')
                    };
                    if is_word_boundary {
                        // Return position at the start of the variable name (after "let ")
                        return (line_num, pos + 4); // "let " is 4 characters
                    }
                }
            }
            // Fallback: find variable name anywhere (for cases where pattern doesn't match)
            if let Some((line_num, col)) = text_utils::find_variable_in_code(&lines, variable) {
                return (line_num, col);
            }
        }
        HirError::UnknownVariable(msg) => {
            let var_name = text_utils::extract_variable_name_from_message(msg);
            if let Some((line_num, col)) = text_utils::find_variable_in_code(&lines, &var_name) {
                return (line_num, col);
            }
        }
        HirError::VariableAlreadyDeclared(msg) => {
            let var_name = text_utils::extract_variable_name_from_message(msg);
            if let Some((line_num, col)) = text_utils::find_variable_in_code(&lines, &var_name) {
                return (line_num, col);
            }
        }
        _ => {
            // For other errors, try to find a keyword or identifier
            for (line_num, line) in lines.iter().enumerate() {
                if line.contains("let") || line.contains("=") {
                    return (line_num, 0);
                }
            }
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
                    let args_part = &error_line[args_start..];
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
        
        let line_bytes = line.as_bytes();
        let mut pos = 0;
        
        while pos < line_bytes.len() {
            // Skip whitespace
            while pos < line_bytes.len() && (line_bytes[pos] as char).is_whitespace() {
                pos += 1;
            }
            if pos >= line_bytes.len() {
                break;
            }
            
            // Skip comments
            if pos + 1 < line_bytes.len() && &line_bytes[pos..pos + 2] == b"//" {
                break; // Rest of line is comment
            }
            
            // Look for identifier
            let ident_start = pos;
            let mut ident_end = ident_start;
            while ident_end < line_bytes.len() {
                let ch = line_bytes[ident_end] as char;
                if ch.is_alphanumeric() || ch == '_' {
                    ident_end += 1;
                } else {
                    break;
                }
            }
            
            if ident_end <= ident_start {
                pos += 1;
                continue;
            }
            
            let identifier = &line[ident_start..ident_end];
            
            // Skip whitespace after identifier
            let mut after_ident = ident_end;
            while after_ident < line_bytes.len() && (line_bytes[after_ident] as char).is_whitespace() {
                after_ident += 1;
            }
            
            if after_ident >= line_bytes.len() {
                pos += 1;
                continue;
            }
            
            // Check for pattern: identifier!(expression!)!
            if line_bytes[after_ident] == b'!' {
                let bang_pos = after_ident;
                let mut after_bang = bang_pos + 1;
                // Skip whitespace after !
                while after_bang < line_bytes.len() && (line_bytes[after_bang] as char).is_whitespace() {
                    after_bang += 1;
                }
                
                if after_bang < line_bytes.len() && line_bytes[after_bang] == b'(' {
                    // Found identifier!( - now check if there's a nested ! pattern
                    let paren_start = after_bang;
                    let mut paren_count = 1;
                    let mut check_pos = paren_start + 1;
                    let mut found_inner_bang = false;
                    
                    // Scan through the function call arguments
                    while check_pos < line_bytes.len() && paren_count > 0 {
                        let ch = line_bytes[check_pos] as char;
                        
                        if ch == '(' {
                            paren_count += 1;
                        } else if ch == ')' {
                            paren_count -= 1;
                            if paren_count == 0 {
                                // Found closing paren - check if there's a ! after it
                                let after_paren_start = check_pos + 1;
                                // Skip whitespace after closing paren
                                let mut after_paren_pos = after_paren_start;
                                while after_paren_pos < line_bytes.len() && (line_bytes[after_paren_pos] as char).is_whitespace() {
                                    after_paren_pos += 1;
                                }
                                
                                if after_paren_pos < line_bytes.len() && line_bytes[after_paren_pos] == b'!' {
                                    // Found pattern: identifier!(...)! - this is problematic
                                    // Skip if this function takes regular values (not thunks) as arguments
                                    if !value_functions.iter().any(|&name| identifier == name) {
                                        // Check if there was a ! inside the arguments
                                        if found_inner_bang {
                                            // This is a nested ! pattern: identifier!(expression!)! or identifier!(identifier!(...))!
                                            // Report the outer ! position
                                            issues.push((line_num, after_paren_pos, 1));
                                        } else {
                                            // Even without inner !, identifier!(...)! is confusing
                                            // Report it as a warning
                                            issues.push((line_num, after_paren_pos, 1));
                                        }
                                    }
                                }
                                pos = check_pos + 1;
                                break;
                            }
                        } else if ch == '!' && paren_count == 1 {
                            // Found ! inside the function call arguments (at the top level)
                            // Check if it's followed by ) or whitespace then )
                            let mut after_inner_bang = check_pos + 1;
                            while after_inner_bang < line_bytes.len() && (line_bytes[after_inner_bang] as char).is_whitespace() {
                                after_inner_bang += 1;
                            }
                            if after_inner_bang < line_bytes.len() && line_bytes[after_inner_bang] == b')' {
                                // Found pattern like: identifier!(expression!) - inner bang before closing paren
                                found_inner_bang = true;
                            }
                        } else if (ch.is_alphanumeric() || ch == '_') && paren_count == 1 {
                            // Check for nested identifier!(...) pattern inside arguments
                            let nested_ident_start = check_pos;
                            let mut nested_ident_end = nested_ident_start;
                            while nested_ident_end < line_bytes.len() {
                                let nested_ch = line_bytes[nested_ident_end] as char;
                                if nested_ch.is_alphanumeric() || nested_ch == '_' {
                                    nested_ident_end += 1;
                                } else {
                                    break;
                                }
                            }
                            
                            if nested_ident_end > nested_ident_start {
                                // Skip whitespace after identifier
                                let mut after_nested_ident = nested_ident_end;
                                while after_nested_ident < line_bytes.len() && (line_bytes[after_nested_ident] as char).is_whitespace() {
                                    after_nested_ident += 1;
                                }
                                
                                // Check if identifier is followed by ! then (
                                if after_nested_ident < line_bytes.len() && line_bytes[after_nested_ident] == b'!' {
                                    // Found nested identifier!( pattern - mark as problematic
                                    found_inner_bang = true;
                                }
                            }
                        }
                        
                        check_pos += 1;
                    }
                    
                    if paren_count > 0 {
                        // Unclosed paren, skip this
                        pos = ident_end + 1;
                    }
                    continue;
                }
            }
            
            // Check for pattern: identifier(expression!)! (original pattern)
            // Check if we have a valid identifier followed by (
            if line_bytes[after_ident] == b'(' {
                // Found identifier( - now check if there's a nested ! pattern
                let paren_start = after_ident;
                let mut paren_count = 1;
                let mut check_pos = paren_start + 1;
                let mut found_inner_bang = false;
                
                // Scan through the function call arguments
                while check_pos < line_bytes.len() && paren_count > 0 {
                    let ch = line_bytes[check_pos] as char;
                    
                    if ch == '(' {
                        paren_count += 1;
                    } else if ch == ')' {
                        paren_count -= 1;
                        if paren_count == 0 {
                            // Found closing paren - check if there's a ! after it
                            let after_paren_start = check_pos + 1;
                            // Skip whitespace after closing paren
                            let mut after_paren_pos = after_paren_start;
                            while after_paren_pos < line_bytes.len() && (line_bytes[after_paren_pos] as char).is_whitespace() {
                                after_paren_pos += 1;
                            }
                            
                            if after_paren_pos < line_bytes.len() && line_bytes[after_paren_pos] == b'!' {
                                // Found pattern: identifier(...)!
                                // Skip if this function takes regular values (not thunks) as arguments
                                if !value_functions.iter().any(|&name| identifier == name) {
                                    // Check if there was a ! inside the arguments
                                    if found_inner_bang {
                                        // This is a nested ! pattern: identifier(expression!)! or identifier(identifier!(...))!
                                        // Report the outer ! position
                                        issues.push((line_num, after_paren_pos, 1));
                                    }
                                }
                            }
                            pos = check_pos + 1;
                            break;
                        }
                    } else if ch == '!' && paren_count == 1 {
                        // Found ! inside the function call arguments (at the top level)
                        // Check if it's followed by ) or whitespace then )
                        let mut after_bang = check_pos + 1;
                        while after_bang < line_bytes.len() && (line_bytes[after_bang] as char).is_whitespace() {
                            after_bang += 1;
                        }
                        if after_bang < line_bytes.len() && line_bytes[after_bang] == b')' {
                            // Found pattern like: identifier(expression!) - inner bang before closing paren
                            found_inner_bang = true;
                        }
                    } else if (ch.is_alphanumeric() || ch == '_') && paren_count == 1 {
                        // Check for nested identifier!(...) pattern inside arguments
                        let nested_ident_start = check_pos;
                        let mut nested_ident_end = nested_ident_start;
                        while nested_ident_end < line_bytes.len() {
                            let nested_ch = line_bytes[nested_ident_end] as char;
                            if nested_ch.is_alphanumeric() || nested_ch == '_' {
                                nested_ident_end += 1;
                            } else {
                                break;
                            }
                        }
                        
                        if nested_ident_end > nested_ident_start {
                            // Skip whitespace after identifier
                            let mut after_nested_ident = nested_ident_end;
                            while after_nested_ident < line_bytes.len() && (line_bytes[after_nested_ident] as char).is_whitespace() {
                                after_nested_ident += 1;
                            }
                            
                            // Check if identifier is followed by ! then (
                            if after_nested_ident < line_bytes.len() && line_bytes[after_nested_ident] == b'!' {
                                // Found nested identifier!( pattern - mark as problematic
                                found_inner_bang = true;
                            }
                        }
                    }
                    
                    check_pos += 1;
                }
                
                if paren_count > 0 {
                    // Unclosed paren, skip this
                    pos = ident_end + 1;
                }
            } else {
                pos += 1;
            }
        }
    }
    
    issues
}

