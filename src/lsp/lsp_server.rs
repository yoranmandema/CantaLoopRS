use std::collections::HashMap;
use std::sync::Arc;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::core::engine::Engine;
use crate::core::semantic_analyser::{CompilerState, ValueKind, HirExpression};
use crate::stdlib;

/// Language Server Protocol server for CantaLoop.
/// 
/// Provides IDE features including:
/// - Real-time diagnostics (parse errors, type errors)
/// - Hover information (variable types, function signatures)
/// - Code completion
/// 
/// Uses the compiler's CompilerState as the single source of truth.
/// The LSP never invents language semantics - it only consumes compiler state.
pub struct CantaLoopLSPServer {
    client: Client,
    documents: Arc<tokio::sync::RwLock<HashMap<Url, String>>>,
    compiler_state_cache: Arc<tokio::sync::RwLock<HashMap<Url, CompilerState>>>,
    engine: Arc<Engine>, // Shared engine with built-in functions registered
}

impl CantaLoopLSPServer {
    pub fn new(client: Client) -> Self {
        // Create and initialize engine with standard library
        let mut engine = Engine::new();
        
        // Load all standard library modules
        stdlib::load_all_stdlib(&mut engine);

        Self {
            client,
            documents: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            compiler_state_cache: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            engine: Arc::new(engine),
        }
    }

    fn format_value_kind(kind: &ValueKind) -> String {
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

    fn format_function_signature(func: &crate::core::semantic_analyser::Function) -> String {
        let params: Vec<String> = func.signature.params.iter()
            .map(|p| Self::format_value_kind(p))
            .collect();
        let return_type = Self::format_value_kind(&func.signature.return_type);
        format!("fn {}({}) -> {}", func.name, params.join(", "), return_type)
    }


    fn byte_position_to_line_col(text: &str, pos: usize) -> (usize, usize) {
        let text_before = &text[..pos];
        let line = text_before.matches('\n').count();
        let col = text_before
            .rfind('\n')
            .map(|last_nl| pos - last_nl - 1)
            .unwrap_or(pos);
        (line, col)
    }

    fn extract_identifier_at_position(text: &str, line: usize, col: usize) -> Option<(String, usize, usize)> {
        let lines: Vec<&str> = text.lines().collect();
        if line >= lines.len() {
            return None;
        }

        let line_text = lines[line];
        if col >= line_text.len() {
            return None;
        }

        // Find start of identifier
        let mut start = col;
        while start > 0 && (line_text.chars().nth(start - 1).map_or(false, |c| c.is_alphanumeric() || c == '_')) {
            start -= 1;
        }

        // Find end of identifier
        let mut end = col;
        while end < line_text.len() && (line_text.chars().nth(end).map_or(false, |c| c.is_alphanumeric() || c == '_')) {
            end += 1;
        }

        if start >= end {
            return None;
        }

        Some((line_text[start..end].to_string(), start, end))
    }

    fn create_diagnostic(range: Range, message: String) -> Diagnostic {
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

    fn create_range(line: usize, col: usize, length: usize) -> Range {
        Range {
            start: Position {
                line: line as u32,
                character: col as u32,
            },
            end: Position {
                line: line as u32,
                character: (col + length) as u32,
            },
        }
    }

    fn extract_variable_name_from_message(msg: &str) -> String {
        // Extract variable name from error message
        // Messages are typically: "Variable 'b' is not declared..." or "b is not a variable..."
        if let Some(start) = msg.find('\'') {
            // Extract from "Variable 'b' is not declared..."
            let after_quote = &msg[start + 1..];
            if let Some(end) = after_quote.find('\'') {
                after_quote[..end].to_string()
            } else if let Some(end) = after_quote.find(' ') {
                // Handle case where there's no closing quote
                after_quote[..end].to_string()
            } else {
                after_quote.to_string()
            }
        } else {
            // Extract from "b is not a variable..."
            if let Some(end) = msg.find(' ') {
                msg[..end].to_string()
            } else {
                msg.to_string()
            }
        }
    }

    fn find_variable_in_code(lines: &[&str], var_name: &str) -> Option<(usize, usize)> {
        // Search for the variable name in the code (as a word boundary to avoid partial matches)
        for (line_num, line) in lines.iter().enumerate() {
            // Find all occurrences and use the first one that's a valid identifier
            let mut search_pos = 0;
            while let Some(pos) = line[search_pos..].find(var_name) {
                let abs_pos = search_pos + pos;
                let after_pos = abs_pos + var_name.len();
                
                // Check if it's a valid identifier boundary (not part of a larger identifier)
                // Check character before (if exists)
                let before_ok = if abs_pos > 0 {
                    line[..abs_pos].chars().last().map_or(true, |c| !c.is_alphanumeric() && c != '_')
                } else {
                    true
                };
                
                // Check character after (if exists)
                let after_ok = if after_pos < line.len() {
                    line[after_pos..].chars().next().map_or(true, |c| !c.is_alphanumeric() && c != '_')
                } else {
                    true // At end of line, which is a valid boundary
                };
                
                if before_ok && after_ok {
                    return Some((line_num, abs_pos));
                }
                search_pos = after_pos;
            }
        }
        None
    }

    fn format_hir_error(e: &crate::core::semantic_analyser::HirError) -> String {
        match e {
            crate::core::semantic_analyser::HirError::UnknownVariable(msg) => msg.clone(),
            crate::core::semantic_analyser::HirError::VariableAlreadyDeclared(msg) => msg.clone(),
            crate::core::semantic_analyser::HirError::TypeMismatch { variable, expected, actual } => {
                format!(
                    "Type mismatch for variable '{}': expected {}, got {}",
                    variable,
                    Self::format_value_kind(expected),
                    Self::format_value_kind(actual)
                )
            }
            crate::core::semantic_analyser::HirError::BinaryOpTypeError { operator, lhs_type, rhs_type, expected } => {
                format!(
                    "{} operation requires {}, but got {} and {}",
                    operator,
                    expected,
                    Self::format_value_kind(lhs_type),
                    Self::format_value_kind(rhs_type)
                )
            }
            crate::core::semantic_analyser::HirError::TypeError(msg) => msg.clone(),
            crate::core::semantic_analyser::HirError::NotImplemented => "Not implemented".to_string(),
        }
    }

    fn find_error_location(text: &str, error: &crate::core::semantic_analyser::HirError) -> (usize, usize) {
        let lines: Vec<&str> = text.lines().collect();
        
        match error {
            crate::core::semantic_analyser::HirError::BinaryOpTypeError { operator, .. } => {
                for (line_num, line) in lines.iter().enumerate() {
                    if let Some(pos) = line.find(operator) {
                        return (line_num, pos);
                    }
                }
            }
            crate::core::semantic_analyser::HirError::TypeMismatch { variable, .. } => {
                for (line_num, line) in lines.iter().enumerate() {
                    if let Some(pos) = line.find(variable) {
                        return (line_num, pos);
                    }
                }
            }
            crate::core::semantic_analyser::HirError::UnknownVariable(msg) => {
                let var_name = Self::extract_variable_name_from_message(msg);
                if let Some((line_num, col)) = Self::find_variable_in_code(&lines, &var_name) {
                    return (line_num, col);
                }
            }
            crate::core::semantic_analyser::HirError::VariableAlreadyDeclared(msg) => {
                let var_name = Self::extract_variable_name_from_message(msg);
                if let Some((line_num, col)) = Self::find_variable_in_code(&lines, &var_name) {
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

    fn improve_parse_error_message(text: &str, line: usize, error_msg: &str) -> String {
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

    fn create_hover_content(markdown: String, range: Range) -> Hover {
        Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: markdown,
            }),
            range: Some(range),
        }
    }

    /// Check if a position in a line is inside a comment
    fn is_in_comment(line: &str, byte_pos: usize) -> bool {
        // Check for line comment (//)
        if let Some(comment_pos) = line.find("//") {
            if byte_pos > comment_pos {
                return true;
            }
        }
        // TODO: Handle block comments (/* ... */) - would need multi-line tracking
        false
    }

    fn tokens_to_absolute(tokens: &[SemanticToken]) -> Vec<(u32, u32, u32, u32, u32)> {
        let mut absolute_tokens = Vec::new();
        let mut current_line = 0;
        let mut current_col = 0;
        
        for token in tokens {
            current_line += token.delta_line;
            if token.delta_line > 0 {
                current_col = token.delta_start;
            } else {
                current_col += token.delta_start;
            }
            absolute_tokens.push((
                current_line,
                current_col,
                token.length,
                token.token_type,
                token.token_modifiers_bitset,
            ));
        }
        absolute_tokens
    }

    fn absolute_to_delta_tokens(absolute_tokens: Vec<(u32, u32, u32, u32, u32)>) -> Vec<SemanticToken> {
        let mut sorted_tokens = Vec::new();
        let mut last_line = 0;
        let mut last_col = 0;
        
        for (line, col, length, token_type, modifiers) in absolute_tokens {
            let delta_line = if sorted_tokens.is_empty() {
                line
            } else {
                line - last_line
            };
            let delta_start = if sorted_tokens.is_empty() || delta_line > 0 {
                col
            } else {
                col - last_col
            };
            
            sorted_tokens.push(SemanticToken {
                delta_line,
                delta_start,
                length,
                token_type,
                token_modifiers_bitset: modifiers,
            });
            
            last_line = line;
            last_col = col;
        }
        sorted_tokens
    }

    fn extract_type_tokens(line: &str, line_num: u32, start_pos: usize, end_pos: usize) -> Vec<(u32, u32, u32, u32, u32)> {
        let mut tokens = Vec::new();
        let type_type = 2; // TYPE
        let operator_type = 3; // OPERATOR
        let type_names = ["num", "number", "string", "str", "boolean", "bool", "any", "void"];
        
        let type_text = &line[start_pos..end_pos];
        let mut byte_pos = 0;
        let type_bytes = type_text.as_bytes();
        
        while byte_pos < type_bytes.len() {
            // Skip whitespace
            while byte_pos < type_bytes.len() && (type_bytes[byte_pos] as char).is_whitespace() {
                byte_pos += 1;
            }
            if byte_pos >= type_bytes.len() {
                break;
            }
            
            // Check for -> or ~> operators
            if byte_pos + 1 < type_bytes.len() {
                if &type_bytes[byte_pos..byte_pos + 2] == b"->" || &type_bytes[byte_pos..byte_pos + 2] == b"~>" {
                    // Highlight the operator as an operator token
                    tokens.push((
                        line_num,
                        (start_pos + byte_pos) as u32,
                        2,
                        operator_type,
                        0,
                    ));
                    byte_pos += 2;
                    // Skip whitespace after operator
                    while byte_pos < type_bytes.len() && (type_bytes[byte_pos] as char).is_whitespace() {
                        byte_pos += 1;
                    }
                    continue;
                }
            }
            
            // Extract type name (atom type)
            let type_start = byte_pos;
            let mut type_end = type_start;
            while type_end < type_bytes.len() {
                let ch = type_bytes[type_end] as char;
                if ch.is_alphanumeric() || ch == '_' {
                    type_end += 1;
                } else {
                    break;
                }
            }
            
            if type_end > type_start {
                let type_name = &type_text[type_start..type_end];
                if type_names.iter().any(|&tn| type_name == tn) {
                    tokens.push((
                        line_num,
                        (start_pos + type_start) as u32,
                        type_name.len() as u32,
                        type_type,
                        0,
                    ));
                }
                byte_pos = type_end;
            } else {
                byte_pos += 1;
            }
        }
        
        tokens
    }


    async fn rebuild_compiler_state(&self, uri: &Url, text: &str) {
        // Use the compiler to build state - single source of truth
        match self.engine.compile_for_lsp(text) {
            Ok(state) => {
                let mut cache = self.compiler_state_cache.write().await;
                cache.insert(uri.clone(), state);
                self.client
                    .log_message(MessageType::INFO, format!("Compiler state built successfully for {}", uri))
                    .await;
            }
            Err(e) => {
                // If compilation fails, remove from cache
                let mut cache = self.compiler_state_cache.write().await;
                cache.remove(uri);
                self.client
                    .log_message(MessageType::WARNING, format!("Compilation failed: {:?}", e))
                    .await;
            }
        }
    }

    fn find_unused_variables(state: &CompilerState) -> Vec<(String, usize, usize)> {
        // Returns: Vec of (variable_name, line, col) for each unused variable
        let mut unused = Vec::new();
        
        // Use compiler state's HIR
        let hir = &state.hir;
        
        // Collect all declared variables (from let statements)
        // Check both top-level blocks and function bodies
        let mut declared_vars: std::collections::HashMap<u32, String> = std::collections::HashMap::new();
        
        // Check top-level blocks
        for block in &hir.blocks {
            for stmt in &block.statements {
                if let crate::core::semantic_analyser::HirStmt::Assign { slot, .. } = stmt {
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
                if let crate::core::semantic_analyser::HirStmt::Assign { slot, .. } = stmt {
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
        // Check both top-level blocks and function bodies
        let mut used_vars: std::collections::HashSet<u32> = std::collections::HashSet::new();
        
        // Check top-level blocks
        for block in &hir.blocks {
            Self::collect_used_vars_from_block(block, &hir, &mut used_vars);
        }
        
        // Check function bodies
        for (_, func) in &hir.functions {
            Self::collect_used_vars_from_block(&func.definition.body, &hir, &mut used_vars);
        }
        
        // Find unused variables
        for (var_id, var_name) in &declared_vars {
            if !used_vars.contains(var_id) {
                // Find the location of this variable in the source text
                // Note: We don't have the source text here, so we'll return the variable name
                // The caller can map it to a location if needed
                // For now, we'll use a placeholder (0, 0)
                unused.push((var_name.clone(), 0, 0));
            }
        }
        
        unused
    }
    
    fn collect_used_vars_from_block(
        block: &crate::core::semantic_analyser::HirBlock,
        hir: &crate::core::semantic_analyser::HirAst,
        used_vars: &mut std::collections::HashSet<u32>,
    ) {
        for stmt in &block.statements {
            Self::collect_used_vars_from_stmt(stmt, hir, used_vars);
        }
    }
    
    fn collect_used_vars_from_stmt(
        stmt: &crate::core::semantic_analyser::HirStmt,
        hir: &crate::core::semantic_analyser::HirAst,
        used_vars: &mut std::collections::HashSet<u32>,
    ) {
        match stmt {
            crate::core::semantic_analyser::HirStmt::Assign { value, .. } => {
                Self::collect_used_vars_from_expr(value, hir, used_vars);
            }
            crate::core::semantic_analyser::HirStmt::AssignIncrement { value, .. } => {
                Self::collect_used_vars_from_expr(value, hir, used_vars);
            }
            crate::core::semantic_analyser::HirStmt::AssignDecrement { value, .. } => {
                Self::collect_used_vars_from_expr(value, hir, used_vars);
            }
            crate::core::semantic_analyser::HirStmt::If { arms, else_block } => {
                for (condition, block) in arms {
                    Self::collect_used_vars_from_expr(condition, hir, used_vars);
                    Self::collect_used_vars_from_block(block, hir, used_vars);
                }
                Self::collect_used_vars_from_block(else_block, hir, used_vars);
            }
            crate::core::semantic_analyser::HirStmt::Match { expression, cases } => {
                Self::collect_used_vars_from_expr(expression, hir, used_vars);
                for (pattern, block) in cases {
                    if let Some(pattern_expr) = pattern {
                        Self::collect_used_vars_from_expr(pattern_expr, hir, used_vars);
                    }
                    Self::collect_used_vars_from_block(block, hir, used_vars);
                }
            }
            crate::core::semantic_analyser::HirStmt::Return { value } => {
                Self::collect_used_vars_from_expr(value, hir, used_vars);
            }
            crate::core::semantic_analyser::HirStmt::Loop { body, .. } => {
                Self::collect_used_vars_from_block(body, hir, used_vars);
            }
            crate::core::semantic_analyser::HirStmt::Break { value } => {
                if let Some(expr) = value {
                    Self::collect_used_vars_from_expr(expr, hir, used_vars);
                }
            }
            crate::core::semantic_analyser::HirStmt::Continue => {
                // Continue doesn't use any variables
            }
            crate::core::semantic_analyser::HirStmt::Expression(expr) => {
                Self::collect_used_vars_from_expr(expr, hir, used_vars);
            }
            crate::core::semantic_analyser::HirStmt::Nop => {
                // No-op statement (used for use statements which are compile-time only)
                // Do nothing
            }
        }
    }
    
    fn collect_used_vars_from_expr(
        expr: &HirExpression,
        hir: &crate::core::semantic_analyser::HirAst,
        used_vars: &mut std::collections::HashSet<u32>,
    ) {
        match expr {
            HirExpression::Identifier(var_id) => {
                used_vars.insert(*var_id);
            }
            HirExpression::Binary { lhs, rhs, .. } => {
                Self::collect_used_vars_from_expr(lhs, hir, used_vars);
                Self::collect_used_vars_from_expr(rhs, hir, used_vars);
            }
            HirExpression::Unary { operand, .. } => {
                Self::collect_used_vars_from_expr(operand, hir, used_vars);
            }
            HirExpression::FunctionCall { args, .. } => {
                for arg in args {
                    Self::collect_used_vars_from_expr(arg, hir, used_vars);
                }
            }
            HirExpression::PostfixInvoke { operand, args } => {
                Self::collect_used_vars_from_expr(operand, hir, used_vars);
                if let Some(arg_list) = args {
                    for arg in arg_list {
                        Self::collect_used_vars_from_expr(arg, hir, used_vars);
                    }
                }
            }
            HirExpression::ComposeThunk { first, second } => {
                Self::collect_used_vars_from_expr(first, hir, used_vars);
                Self::collect_used_vars_from_expr(second, hir, used_vars);
            }
            _ => {}
        }
    }
    
    fn find_variable_location(text: &str, var_name: &str) -> Option<(usize, usize)> {
        // Find the location of a variable declaration in the source text
        let lines: Vec<&str> = text.lines().collect();
        for (line_num, line) in lines.iter().enumerate() {
            // Look for "let var_name" pattern
            if let Some(pos) = line.find(&format!("let {}", var_name)) {
                // Check if it's actually a let statement (not part of another word)
                let before = if pos > 0 { line.chars().nth(pos - 1) } else { None };
                let after_pos = pos + 4 + var_name.len(); // "let " + var_name
                let after = if after_pos < line.len() { line.chars().nth(after_pos) } else { None };
                
                // Check that it's not part of another identifier
                if (before.is_none() || !before.unwrap().is_alphanumeric() && before.unwrap() != '_')
                    && (after.is_none() || !after.unwrap().is_alphanumeric() && after.unwrap() != '_') {
                    return Some((line_num, pos + 4)); // Return position after "let "
                }
            }
        }
        None
    }

    fn find_nested_invoke_patterns(text: &str) -> Vec<(usize, usize, usize)> {
        // Find patterns like: identifier(expression!)! or identifier!(expression!)!
        // Returns: Vec of (line, col, length) for each problematic pattern
        // Note: Functions that take regular values (not thunks) as arguments are excluded
        // e.g., print(expression!)! is valid because print takes a value, not a thunk
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

    async fn update_diagnostics(&self, uri: Url) {
        let documents = self.documents.read().await;
        let text = match documents.get(&uri) {
            Some(text) => text.clone(),
            None => return,
        };
        drop(documents);

        let mut diagnostics = Vec::new();

        // Use compiler state - single source of truth
        match self.engine.compile_for_lsp(&text) {
            Ok(state) => {
                // Add diagnostics from compiler state
                for error in &state.diagnostics {
                    let error_msg = Self::format_hir_error(error);
                    let (found_line, found_col) = Self::find_error_location(&text, error);
                    
                    // Check if this is a nested invoke pattern error - these should be warnings, not errors
                    let is_nested_invoke_error = error_msg.contains("Confusing nested invoke pattern") ||
                                                 error_msg.contains("nested invoke pattern");
                    
                    let diagnostic = if is_nested_invoke_error {
                        // Convert to warning for nested invoke patterns since code is still runnable
                        Diagnostic {
                            range: Self::create_range(found_line, found_col, 1),
                            severity: Some(DiagnosticSeverity::WARNING),
                            code: Some(NumberOrString::String("nested_invoke".to_string())),
                            code_description: None,
                            source: Some("CantaLoop".to_string()),
                            message: error_msg,
                            related_information: None,
                            tags: Some(vec![DiagnosticTag::UNNECESSARY]),
                            data: None,
                        }
                    } else {
                        // Regular semantic errors remain as errors
                        Self::create_diagnostic(
                            Self::create_range(found_line, found_col, 1),
                            error_msg,
                        )
                    };
                    diagnostics.push(diagnostic);
                }
                
                // Check for unused variables using compiler state
                let unused_vars = Self::find_unused_variables(&state);
                for (var_name, line_num, col) in unused_vars {
                    if line_num > 0 || col > 0 { // Only add if we have location info
                        let diagnostic = Diagnostic {
                            range: Self::create_range(line_num, col, var_name.len()),
                            severity: Some(DiagnosticSeverity::WARNING),
                            code: Some(NumberOrString::String("unused_variable".to_string())),
                            code_description: None,
                            source: Some("CantaLoop".to_string()),
                            message: format!("Variable '{}' is declared but never used", var_name),
                            related_information: None,
                            tags: Some(vec![DiagnosticTag::UNNECESSARY]),
                            data: None,
                        };
                        diagnostics.push(diagnostic);
                    }
                }
            }
            Err(e) => {
                // Parse errors
                let (line, col) = match e.location {
                    pest::error::InputLocation::Pos(pos) => Self::byte_position_to_line_col(&text, pos),
                    pest::error::InputLocation::Span((start, _end)) => Self::byte_position_to_line_col(&text, start),
                };

                // Improve error message for missing type annotations
                let error_msg = format!("{}", e);
                let improved_msg = Self::improve_parse_error_message(&text, line, &error_msg);
                let diagnostic = Self::create_diagnostic(
                    Self::create_range(line, col, 1),
                    improved_msg,
                );
                diagnostics.push(diagnostic);
            }
        }

        // Check for nested ! invoke patterns (e.g., mul2(add10(i)!)! or mul2!(add10!(i))!)
        let nested_invoke_issues = Self::find_nested_invoke_patterns(&text);
        for (line_num, col, length) in nested_invoke_issues {
            let diagnostic = Diagnostic {
                range: Self::create_range(line_num, col, length),
                severity: Some(DiagnosticSeverity::WARNING),
                code: Some(NumberOrString::String("nested_invoke".to_string())),
                code_description: None,
                source: Some("CantaLoop".to_string()),
                message: "Nested invoke operator (!) detected. Patterns like `mul2(add10(i)!)!` or `mul2!(add10!(i))!` can be confusing and may create unnecessary intermediate thunks. Consider extracting the inner invocation: `let temp = add10(i)!; mul2(temp)!`".to_string(),
                related_information: None,
                tags: Some(vec![DiagnosticTag::UNNECESSARY]),
                data: None,
            };
            diagnostics.push(diagnostic);
        }


        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for CantaLoopLSPServer {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "CantaLoop LSP".to_string(),
                version: Some("0.1.0".to_string()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::FULL),
                        will_save: None,
                        will_save_wait_until: None,
                        save: None,
                    },
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![".", "(", "p", "r", "i", "n", "t"].iter().map(|s| s.to_string()).collect()),
                    resolve_provider: Some(false),
                    ..Default::default()
                }),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            work_done_progress_options: Default::default(),
                            legend: SemanticTokensLegend {
                                token_types: vec![
                                    SemanticTokenType::FUNCTION,
                                    SemanticTokenType::VARIABLE,
                                    SemanticTokenType::TYPE,
                                    SemanticTokenType::OPERATOR,
                                    SemanticTokenType::KEYWORD,
                                ],
                                token_modifiers: vec![
                                    SemanticTokenModifier::DECLARATION,
                                    SemanticTokenModifier::READONLY,
                                    SemanticTokenModifier::DEPRECATED, // Reuse deprecated as "thunk" indicator
                                ],
                            },
                            range: Some(true),
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                        },
                    ),
                ),
                ..Default::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "CantaLoop LSP initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text.clone();

        let mut documents = self.documents.write().await;
        documents.insert(uri.clone(), text.clone());
        drop(documents);

        self.rebuild_compiler_state(&uri, &text).await;
        self.update_diagnostics(uri).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let mut documents = self.documents.write().await;
        
        if let Some(text) = params.content_changes.into_iter().next() {
            let text_clone = text.text.clone();
            documents.insert(uri.clone(), text.text);
            drop(documents);
            self.rebuild_compiler_state(&uri, &text_clone).await;
        } else {
            drop(documents);
        }

        self.update_diagnostics(uri).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        let mut documents = self.documents.write().await;
        documents.remove(&uri);
        drop(documents);
        
        let mut cache = self.compiler_state_cache.write().await;
        cache.remove(&uri);
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        self.client
            .log_message(MessageType::INFO, "Hover method called")
            .await;
        
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;

        let documents = self.documents.read().await;
        let text = match documents.get(&uri) {
            Some(text) => text,
            None => return Ok(None),
        };

        // Simple implementation: find identifier at position
        let identifier_info = match Self::extract_identifier_at_position(text, pos.line as usize, pos.character as usize) {
            Some((id, start, end)) => (id, start, end),
            None => return Ok(None),
        };
        let (identifier, start, end) = identifier_info;
        drop(documents);

        // Log for debugging
        self.client
            .log_message(MessageType::INFO, format!("Hover requested for identifier: '{}' at line {}", identifier, pos.line))
            .await;

        // Use compiler state - single source of truth
        let cache = self.compiler_state_cache.read().await;
        let has_state = cache.contains_key(&uri);
        drop(cache);
        
        if !has_state {
            // Compiler state not found, try to rebuild it
            self.client
                .log_message(MessageType::INFO, format!("Compiler state not found for URI, attempting to rebuild: {}", uri))
                .await;
            let documents = self.documents.read().await;
            if let Some(text) = documents.get(&uri) {
                let text_clone = text.clone();
                drop(documents);
                self.rebuild_compiler_state(&uri, &text_clone).await;
            }
        }
        
        let cache = self.compiler_state_cache.read().await;
        if let Some(state) = cache.get(&uri) {
            self.client
                .log_message(MessageType::INFO, format!("Compiler state found for URI, searching for '{}'", identifier))
                .await;
            
            // Use symbol table to find symbol
            let symbols = state.symbols.find_by_name(&identifier);
            if let Some(symbol) = symbols.first() {
                let type_str = Self::format_value_kind(&symbol.ty);
                let hover_content = match symbol.kind {
                    crate::core::semantic_analyser::SymbolKind::Function => {
                        // For functions, try to get the full signature from HIR
                        if let Some((func_id, _)) = state.hir.functions.iter()
                            .find(|(_, f)| f.name == identifier) {
                            if let Some(func) = state.hir.functions.get(func_id) {
                                let signature = Self::format_function_signature(func);
                                format!("```cantaloop\n{}\n```", signature)
                            } else {
                                format!("```cantaloop\n{}\n```\nType: `{}`", identifier, type_str)
                            }
                        } else {
                            format!("```cantaloop\n{}\n```\nType: `{}`", identifier, type_str)
                        }
                    }
                    _ => {
                        format!("```cantaloop\n{}\n```\nType: `{}`", identifier, type_str)
                    }
                };
                let range = Self::create_range(pos.line as usize, start, end - start);
                return Ok(Some(Self::create_hover_content(hover_content, range)));
            }
            
            self.client
                .log_message(MessageType::INFO, format!("Identifier '{}' not found in symbol table", identifier))
                .await;
        } else {
            self.client
                .log_message(MessageType::WARNING, format!("No compiler state found for URI: {}", uri))
                .await;
        }

        Ok(None)
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;

        self.client
            .log_message(MessageType::INFO, format!("Completion requested for URI: {} at line {}", uri, pos.line))
            .await;

        let documents = self.documents.read().await;
        let text = match documents.get(&uri) {
            Some(text) => text,
            None => {
                self.client
                    .log_message(MessageType::WARNING, format!("No document found for URI: {}", uri))
                    .await;
                return Ok(None);
            }
        };

        let lines: Vec<&str> = text.lines().collect();
        if pos.line as usize >= lines.len() {
            return Ok(None);
        }

        let line = &lines[pos.line as usize];
        let char_pos = pos.character as usize;
        
        // Get the text before cursor for prefix matching
        let prefix = if char_pos <= line.len() {
            &line[..char_pos]
        } else {
            line
        };

        // Build completion list
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
        let keywords = vec!["fn", "if", "else", "return", "let", "true", "false", "loop", "while", "for", "in", "break", "continue", "use"];
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
        // Check if we're in a context where a type annotation is expected
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
            // If prefix ends with a type name and space, suggest function/thunk types
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
        let cache = self.compiler_state_cache.read().await;
        if let Some(state) = cache.get(&uri) {
            // Use symbol table - single source of truth
            for symbol in state.symbols.get_all() {
                if prefix.is_empty() || symbol.name.starts_with(prefix.trim()) {
                    let type_str = Self::format_value_kind(&symbol.ty);
                    let kind = match symbol.kind {
                        crate::core::semantic_analyser::SymbolKind::Function => CompletionItemKind::FUNCTION,
                        crate::core::semantic_analyser::SymbolKind::Variable | 
                        crate::core::semantic_analyser::SymbolKind::Parameter => CompletionItemKind::VARIABLE,
                        crate::core::semantic_analyser::SymbolKind::Module => CompletionItemKind::MODULE,
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

        self.client
            .log_message(MessageType::INFO, format!("Returning {} completion items", items.len()))
            .await;

        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        self.client
            .log_message(MessageType::INFO, "Semantic tokens requested")
            .await;
        
        let uri = params.text_document.uri;
        let documents = self.documents.read().await;
        let text = match documents.get(&uri) {
            Some(text) => text.clone(),
            None => {
                self.client
                    .log_message(MessageType::WARNING, "No document found for semantic tokens")
                    .await;
                return Ok(None);
            }
        };
        drop(documents);
        
        // Ensure compiler state is built before generating semantic tokens
        let cache = self.compiler_state_cache.read().await;
        let has_state = cache.contains_key(&uri);
        drop(cache);
        
        if !has_state {
            self.client
                .log_message(MessageType::INFO, "Compiler state not found, rebuilding for semantic tokens")
                .await;
            self.rebuild_compiler_state(&uri, &text).await;
        }

        // Find thunks: function calls that are NOT followed by !
        // Also mark variables that hold thunks
        let mut tokens = Vec::new();
        let lines: Vec<&str> = text.lines().collect();
        
        // Keywords that should not be marked as thunks
        let keywords = ["fn", "if", "else", "elseif", "match", "return", "let", "and", "or", "true", "false", "loop", "while", "for", "break", "continue", "in", "use"];
        
        // Token type indices (from legend)
        let function_type = 0; // FUNCTION
        let variable_type = 1; // VARIABLE
        let _type_type = 2; // TYPE
        let operator_type = 3; // OPERATOR
        let keyword_type = 4; // KEYWORD
        // Token modifier bits (from legend: declaration=0, readonly=1, deprecated=2)
        // We use deprecated modifier to mark thunks
        let thunk_modifier = 1 << 2; // bit 2 for deprecated (repurposed as thunk)
        
        // Type names that should be highlighted
        let _type_names = ["num", "number", "string", "str", "boolean", "bool", "any", "void"];
        
        let mut last_line = 0;
        let mut last_start = 0;
        
        // Get compiler state - use it as the source of truth for semantics
        let cache = self.compiler_state_cache.read().await;
        let state = match cache.get(&uri) {
            Some(s) => s,
            None => {
                drop(cache);
                return Ok(None);
            }
        };

        // Build maps from compiler state for fast lookup
        // Map symbol names to their types and kinds
        let mut symbol_info: HashMap<String, (ValueKind, crate::core::semantic_analyser::SymbolKind)> = HashMap::new();
        for symbol in state.symbols.get_all() {
            symbol_info.insert(symbol.name.clone(), (symbol.ty.clone(), symbol.kind.clone()));
        }

        drop(cache);
        
        // Now scan text to find positions, but use compiler state for classification
        for (line_num, line) in lines.iter().enumerate() {
            let line_bytes = line.as_bytes();
            let mut byte_pos = 0;
            
            // Find identifiers followed by (
            while byte_pos < line_bytes.len() {
                // Skip whitespace
                while byte_pos < line_bytes.len() && (line_bytes[byte_pos] as char).is_whitespace() {
                    byte_pos += 1;
                }
                if byte_pos >= line_bytes.len() {
                    break;
                }
                
                // Check if this is a function declaration (fn identifier)
                if byte_pos + 2 < line_bytes.len() && &line_bytes[byte_pos..byte_pos + 2] == b"fn" {
                    // Skip function declaration
                    byte_pos += 2;
                    while byte_pos < line_bytes.len() && (line_bytes[byte_pos] as char).is_whitespace() {
                        byte_pos += 1;
                    }
                    // Skip function name
                    while byte_pos < line_bytes.len() {
                        let ch = line_bytes[byte_pos] as char;
                        if ch.is_alphanumeric() || ch == '_' {
                            byte_pos += 1;
                        } else {
                            break;
                        }
                    }
                    continue;
                }
                
                // Check if we have an identifier
                let ident_start = byte_pos;
                let mut ident_end = ident_start;
                while ident_end < line_bytes.len() {
                    let ch = line_bytes[ident_end] as char;
                    if ch.is_alphanumeric() || ch == '_' {
                        ident_end += 1;
                    } else {
                        break;
                    }
                }
                
                if ident_end > ident_start {
                    // Skip if this identifier is inside a comment
                    if Self::is_in_comment(line, ident_start) {
                        byte_pos = ident_end + 1;
                        continue;
                    }
                    
                    let identifier = &line[ident_start..ident_end];
                    
                    // Highlight keywords
                    if keywords.contains(&identifier) {
                        let delta_line = if tokens.is_empty() {
                            line_num as u32
                        } else {
                            (line_num as i32 - last_line as i32) as u32
                        };
                        let delta_start = if tokens.is_empty() || delta_line > 0 {
                            ident_start as u32
                        } else {
                            (ident_start as i32 - last_start as i32) as u32
                        };
                        
                        tokens.push(SemanticToken {
                            delta_line,
                            delta_start,
                            length: identifier.len() as u32,
                            token_type: keyword_type,
                            token_modifiers_bitset: 0,
                        });
                        
                        last_line = line_num;
                        last_start = ident_start;
                        byte_pos = ident_end + 1;
                        continue;
                    }
                    
                    // Use compiler state to classify this identifier
                    // Text parsing only finds position - compiler state determines semantics
                    if let Some((ty, kind)) = symbol_info.get(identifier) {
                        let delta_line = if tokens.is_empty() {
                            line_num as u32
                        } else {
                            (line_num as i32 - last_line as i32) as u32
                        };
                        let delta_start = if tokens.is_empty() || delta_line > 0 {
                            ident_start as u32
                        } else {
                            (ident_start as i32 - last_start as i32) as u32
                        };
                        
                        let token_type = match kind {
                            crate::core::semantic_analyser::SymbolKind::Function => function_type,
                            crate::core::semantic_analyser::SymbolKind::Variable | 
                            crate::core::semantic_analyser::SymbolKind::Parameter => variable_type,
                            _ => variable_type,
                        };
                        
                        // Mark thunks with the thunk modifier
                        let modifiers = if matches!(ty, ValueKind::Thunk(_)) {
                            thunk_modifier
                        } else {
                            0
                        };
                        
                        tokens.push(SemanticToken {
                            delta_line,
                            delta_start,
                            length: identifier.len() as u32,
                            token_type,
                            token_modifiers_bitset: modifiers,
                        });
                        
                        last_line = line_num;
                        last_start = ident_start;
                    }
                    
                    byte_pos = ident_end + 1;
                } else {
                    // Check for operators: .. and !
                    // Check for .. operator (range operator)
                    if byte_pos + 1 < line_bytes.len() && &line_bytes[byte_pos..byte_pos + 2] == b".." {
                        let delta_line = if tokens.is_empty() {
                            line_num as u32
                        } else {
                            (line_num as i32 - last_line as i32) as u32
                        };
                        let delta_start = if tokens.is_empty() || delta_line > 0 {
                            byte_pos as u32
                        } else {
                            (byte_pos as i32 - last_start as i32) as u32
                        };
                        
                        tokens.push(SemanticToken {
                            delta_line,
                            delta_start,
                            length: 2,
                            token_type: operator_type,
                            token_modifiers_bitset: 0,
                        });
                        
                        last_line = line_num;
                        last_start = byte_pos;
                        byte_pos += 2;
                        continue;
                    }
                    
                    // Check for ! operator (not/invoke operator)
                    if line_bytes[byte_pos] == b'!' {
                        // Only highlight ! if it's not part of != (which should be handled separately)
                        let is_not_equal = byte_pos + 1 < line_bytes.len() && line_bytes[byte_pos + 1] == b'=';
                        if !is_not_equal {
                            let delta_line = if tokens.is_empty() {
                                line_num as u32
                            } else {
                                (line_num as i32 - last_line as i32) as u32
                            };
                            let delta_start = if tokens.is_empty() || delta_line > 0 {
                                byte_pos as u32
                            } else {
                                (byte_pos as i32 - last_start as i32) as u32
                            };
                            
                            tokens.push(SemanticToken {
                                delta_line,
                                delta_start,
                                length: 1,
                                token_type: operator_type,
                                token_modifiers_bitset: 0,
                            });
                            
                            last_line = line_num;
                            last_start = byte_pos;
                            byte_pos += 1;
                            continue;
                        }
                    }
                    
                    byte_pos += 1;
                }
            }
        }
        
        // Third pass: mark type annotations
        let mut type_tokens_absolute: Vec<(u32, u32, u32, u32, u32)> = Vec::new();
        
        for (line_num, line) in lines.iter().enumerate() {
            let line_bytes = line.as_bytes();
            let mut byte_pos = 0;
            
            while byte_pos < line_bytes.len() {
                // Skip whitespace
                while byte_pos < line_bytes.len() && (line_bytes[byte_pos] as char).is_whitespace() {
                    byte_pos += 1;
                }
                if byte_pos >= line_bytes.len() {
                    break;
                }
                
                // Look for type annotations: identifier : type
                // Or return type: -> type
                
                // Check for return type annotation (-> type)
                if byte_pos + 1 < line_bytes.len() && &line_bytes[byte_pos..byte_pos + 2] == b"->" {
                    // Skip if this is inside a comment
                    if Self::is_in_comment(line, byte_pos) {
                        byte_pos += 1;
                        continue;
                    }
                    let arrow_start = byte_pos;
                    byte_pos += 2;
                    // Skip whitespace after ->
                    while byte_pos < line_bytes.len() && (line_bytes[byte_pos] as char).is_whitespace() {
                        byte_pos += 1;
                    }
                    // Extract full type annotation (may include -> or ~> operators)
                    let type_start = byte_pos;
                    let mut type_end = type_start;
                    while type_end < line_bytes.len() {
                        let ch = line_bytes[type_end] as char;
                        // Allow alphanumeric, whitespace, ->, ~>, and _ for type annotations
                        if ch.is_alphanumeric() || ch == '_' || ch.is_whitespace() || 
                           (ch == '-' && type_end + 1 < line_bytes.len() && line_bytes[type_end + 1] == b'>') ||
                           (ch == '~' && type_end + 1 < line_bytes.len() && line_bytes[type_end + 1] == b'>') {
                            if ch == '-' || ch == '~' {
                                type_end += 2; // Skip both chars of -> or ~>
                            } else {
                                type_end += 1;
                            }
                        } else if ch == ',' || ch == ')' || ch == '{' || ch == '=' {
                            // Stop at these characters
                            break;
                        } else {
                            break;
                        }
                    }
                    
                    if type_end > type_start {
                        // Extract type tokens from the type annotation
                        let type_tokens = Self::extract_type_tokens(line, line_num as u32, arrow_start, type_end);
                        type_tokens_absolute.extend(type_tokens);
                    }
                    byte_pos = type_end;
                    continue;
                }
                
                // Check for colon (type annotation: identifier : type)
                if byte_pos < line_bytes.len() && line_bytes[byte_pos] == b':' {
                    // Look backwards to find the identifier before :
                    let mut before_colon = byte_pos;
                    // Skip whitespace before :
                    while before_colon > 0 && (line_bytes[before_colon - 1] as char).is_whitespace() {
                        before_colon -= 1;
                    }
                    
                    // Check if there's an identifier before the colon
                    let ident_end = before_colon;
                    let mut ident_start = ident_end;
                    while ident_start > 0 {
                        let ch = line_bytes[ident_start - 1] as char;
                        if ch.is_alphanumeric() || ch == '_' {
                            ident_start -= 1;
                        } else {
                            break;
                        }
                    }
                    
                    // Only proceed if we found an identifier before the colon
                    // and it's not a keyword
                    if ident_start < ident_end {
                        // Skip if this is inside a comment
                        if Self::is_in_comment(line, ident_start) {
                            byte_pos += 1;
                            continue;
                        }
                        let before_ident = &line[..ident_start].trim_end();
                        // Skip if this is part of a string or comment
                        if !before_ident.ends_with('"') && !before_ident.ends_with("//") {
                            byte_pos += 1; // Skip the colon
                            // Skip whitespace after colon
                            while byte_pos < line_bytes.len() && (line_bytes[byte_pos] as char).is_whitespace() {
                                byte_pos += 1;
                            }
                            
                            // Extract full type annotation after colon (may include -> or ~> operators)
                            let type_start = byte_pos;
                            let mut type_end = type_start;
                            while type_end < line_bytes.len() {
                                let ch = line_bytes[type_end] as char;
                                // Allow alphanumeric, whitespace, ->, ~>, and _ for type annotations
                                if ch.is_alphanumeric() || ch == '_' || ch.is_whitespace() || 
                                   (ch == '-' && type_end + 1 < line_bytes.len() && line_bytes[type_end + 1] == b'>') ||
                                   (ch == '~' && type_end + 1 < line_bytes.len() && line_bytes[type_end + 1] == b'>') {
                                    if ch == '-' || ch == '~' {
                                        type_end += 2; // Skip both chars of -> or ~>
                                    } else {
                                        type_end += 1;
                                    }
                                } else if ch == ',' || ch == ')' || ch == '{' || ch == '=' {
                                    // Stop at these characters
                                    break;
                                } else {
                                    // Unknown character, stop
                                    break;
                                }
                            }
                            
                            if type_end > type_start {
                                // Extract type tokens from the type annotation
                                let type_tokens = Self::extract_type_tokens(line, line_num as u32, type_start, type_end);
                                type_tokens_absolute.extend(type_tokens);
                            }
                            // Continue from after the type
                            byte_pos = type_end;
                            continue;
                        }
                    }
                }
                
                byte_pos += 1;
            }
        }

        // Sort tokens by line and column to ensure correct order
        // Convert delta encoding to absolute positions, sort, then convert back
        let mut absolute_tokens = Self::tokens_to_absolute(&tokens);
        
        // Add type tokens (already in absolute positions)
        absolute_tokens.extend(type_tokens_absolute);
        
        // Sort all tokens by line and column
        absolute_tokens.sort_by(|a, b| {
            a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1))
        });
        
        // Convert back to delta encoding
        let sorted_tokens = Self::absolute_to_delta_tokens(absolute_tokens);

        self.client
            .log_message(MessageType::INFO, format!("Returning {} semantic tokens", sorted_tokens.len()))
            .await;
        
        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: sorted_tokens,
        })))
    }
}
