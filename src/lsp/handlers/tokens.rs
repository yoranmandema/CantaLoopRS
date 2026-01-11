//! Semantic tokens handler.

use tower_lsp::lsp_types::*;

use crate::lsp::server::CantaLoopServer;
use crate::lsp::mapping::spans::LineIndex;

// File logger for debugging token issues
fn log_to_file(msg: &str) {
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("lsp_debug.log")
    {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let _ = writeln!(file, "[{}] {}", timestamp, msg);
    }
}

/// Handle textDocument/semanticTokens/full.
pub async fn handle_semantic_tokens_full(
    server: &CantaLoopServer,
    params: SemanticTokensParams,
) -> tower_lsp::jsonrpc::Result<Option<SemanticTokensResult>> {
    let uri = &params.text_document.uri;
    
    eprintln!("[LSP] ========================================");
    eprintln!("[LSP] semanticTokens/full ENTERED");
    eprintln!("[LSP] URI: {}", uri);
    eprintln!("[LSP] ========================================");

    // Get source text and file ID
    eprintln!("[LSP] Step 1: Getting source manager lock...");
    let (file_id, source) = {
        let source_manager = server.source_manager.read().await;
        eprintln!("[LSP] Step 2: Source manager lock acquired");
        
        let file_id = match source_manager.get_file_id(uri) {
            Some(id) => {
                eprintln!("[LSP] Step 3: File ID found: {:?}", id);
                id
            },
            None => {
                eprintln!("[LSP] Step 3: File NOT in source manager");
                eprintln!("[LSP] Available files: {:?}", source_manager.list_files());
                return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                    result_id: None,
                    data: vec![],
                })));
            }
        };
        
        let text = source_manager.get_file_text(file_id)
            .unwrap_or("")
            .to_string();
        eprintln!("[LSP] Step 4: File text retrieved, length: {}", text.len());
        (file_id, text)
    };

    // Get compiler snapshot
    eprintln!("[LSP] Step 5: Getting snapshot...");
    let snapshot = match server.compiler_state.get_snapshot_for_file(file_id).await {
        Some(s) => {
            eprintln!("[LSP] Step 6: ✓ Snapshot found");
            s
        },
        None => {
            eprintln!("[LSP] Step 6: ✗ No snapshot available yet");
            eprintln!("[LSP] This means compilation hasn't finished");
            return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data: vec![],
            })));
        }
    };
    
    // Check if CST exists
    eprintln!("[LSP] Step 7: Checking CST...");
    if snapshot.cst(file_id).is_none() {
        eprintln!("[LSP] Step 7: ✗ No CST found (parse failed)");
        return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: vec![],
        })));
    }
    eprintln!("[LSP] Step 7: ✓ CST exists");

    // Generate tokens
    eprintln!("[LSP] Step 8: Generating semantic tokens...");
    let tokens = generate_semantic_tokens(file_id, &snapshot, &source);
    eprintln!("[LSP] Step 9: ✓ Generated {} tokens", tokens.len());
    
    // Debug info
    let has_symbols = snapshot.has_symbols();
    let has_hir = snapshot.hir().is_some();
    let diagnostics_count = snapshot.diagnostics(file_id).len();
    
    eprintln!("[LSP] Snapshot state:");
    eprintln!("[LSP]   - has_symbols: {}", has_symbols);
    eprintln!("[LSP]   - has_hir: {}", has_hir);
    eprintln!("[LSP]   - diagnostics: {}", diagnostics_count);
    eprintln!("[LSP]   - token_count: {}", tokens.len());
    
    eprintln!("[LSP] ========================================");
    eprintln!("[LSP] semanticTokens/full RETURNING {} tokens", tokens.len());
    eprintln!("[LSP] ========================================");

    Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data: tokens,
    })))
}

/// Generate semantic tokens from compiler snapshot.
/// 
/// CRITICAL: This function MUST always generate tokens if CST exists, even if:
/// - Symbol table is missing
/// - HIR is missing
/// - Semantic coverage is low
/// - Name resolution failed
/// 
/// VSCode expects tokens for syntax highlighting even when the program has errors.
/// We generate CST-based tokens (keywords, literals, identifiers) as a fallback.
fn generate_semantic_tokens(
    file_id: crate::core::source_manager::FileId,
    snapshot: &crate::core::lsp_api::CompilerSnapshot,
    source: &str,
) -> Vec<SemanticToken> {
    use crate::core::hir_lowering::{Span as HirSpan, SymbolKind};
    
    let line_index = LineIndex::new(source);
    
    // Build list of all spans with their token types
    let mut span_tokens: Vec<(HirSpan, u32, u32)> = Vec::new(); // (span, token_type, modifiers)
    
    // CRITICAL: Always generate CST-based tokens first (keywords, literals, operators, identifiers)
    // This ensures we have tokens even if semantic analysis failed
    if let Some(cst) = snapshot.cst(file_id) {
        extract_cst_tokens(cst, source, &mut span_tokens);
        // Also extract identifiers from CST for basic highlighting
        extract_identifiers_from_cst(cst, &mut span_tokens);
    }
    
    // If we have semantic information, enhance tokens with symbol types
    // But never fail if this is missing - we already have CST tokens above
    if let (Some(symbols), Some(hir)) = (snapshot.symbol_table(), snapshot.hir()) {
    
        // Process all symbols from span_to_symbol map to enhance tokens with semantic types
        for (span, &symbol_id) in &symbols.span_to_symbol {
            // Get symbol info
            if let Some(info) = snapshot.symbol_info(symbol_id) {
                let token_type = match info.kind {
                    SymbolKind::Function => {
                        // Check if it's effectful or pure
                        // Look up function in HIR to check signature
                        let is_effectful = hir.functions.values()
                            .find(|f| f.name == info.name)
                            .map(|f| f.signature.is_effectful)
                            .unwrap_or(false);
                        
                        if is_effectful {
                            0 // FUNCTION - we'll use this for now, could add effect modifier later
                        } else {
                            0 // FUNCTION
                        }
                    }
                    SymbolKind::Variable => 1, // VARIABLE
                    SymbolKind::Parameter => 2, // PARAMETER
                    SymbolKind::Module => 8, // TYPE (modules are like types)
                };
                
                // Override or add semantic token type for this span
                // Remove any existing CST-based token for this span and add semantic one
                span_tokens.retain(|(s, _, _)| *s != *span);
                span_tokens.push((*span, token_type, 0));
            }
        }
    } else {
        // No semantic information available - log warning but continue with CST tokens
        log::warn!("semantic coverage low — falling back to CST-only tokens for file_id {:?}", file_id);
    }
    
    // Deduplicate overlapping tokens before conversion
    // Keep only one token per position - prefer semantic tokens (those with higher priority types)
    // Sort by start position, then by span size (smallest first), then by type priority
    span_tokens.sort_by(|a: &(HirSpan, u32, u32), b: &(HirSpan, u32, u32)| {
        // First sort by start position
        a.0.start.cmp(&b.0.start)
            // Then by span size (smaller = more specific)
            .then(a.0.end.cmp(&b.0.end))
            // Finally by type priority (functions > variables > keywords)
            .then(b.1.cmp(&a.1))
    });
    
    // Remove overlapping tokens - keep the first (smallest, highest priority) token
    let mut deduplicated = Vec::new();
    for token in span_tokens {
        let overlaps = deduplicated.iter().any(|(existing_span, _, _): &(HirSpan, u32, u32)| {
            // Check if this token overlaps with any existing token
            !(token.0.end <= existing_span.start || token.0.start >= existing_span.end)
        });
        if !overlaps {
            deduplicated.push(token);
        }
    }
    
    // Convert spans to semantic tokens (relative deltas)
    convert_to_semantic_tokens(deduplicated, &line_index)
}

/// Extract tokens from CST (keywords, operators, literals, doc comments).
fn extract_cst_tokens(
    cst: &crate::core::cst::CstProgram,
    source: &str,
    tokens: &mut Vec<(crate::core::hir_lowering::Span, u32, u32)>,
) {
    use crate::core::cst::{CstExpr, CstStatement};
    use crate::core::hir_lowering::Span as HirSpan;
    
    fn walk_expr(expr: &crate::core::cst::Spanned<CstExpr>, tokens: &mut Vec<(HirSpan, u32, u32)>) {
        match &expr.node {
            CstExpr::Literal(lit) => {
                let span = HirSpan::new(expr.span.start as usize, expr.span.end as usize);
                match &lit.node {
                    crate::core::cst::CstLiteral::String(_) => tokens.push((span, 5, 0)), // STRING
                    crate::core::cst::CstLiteral::Number(_) => tokens.push((span, 6, 0)), // NUMBER
                    crate::core::cst::CstLiteral::Boolean(_) => {
                        // Booleans are keywords in CantaLoop
                        tokens.push((span, 3, 0)) // KEYWORD
                    }
                }
            }
            CstExpr::Infix { .. } | CstExpr::Prefix { .. } | CstExpr::Postfix { .. } => {
                // Operators are handled by their operator tokens
                // Could extract operator spans here
            }
            _ => {}
        }
    }
    
    // Extract ALL types of comments from source text
    // Look for // (line comments), /// (doc comments), /* */ (block comments), and /** */ (doc block comments)
    let bytes = source.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'/' {
            if bytes[i + 1] == b'/' {
                // Line comment: // or ///
                let start = i;
                let mut end = i + 2;
                
                // Find end of line
                while end < bytes.len() && bytes[end] != b'\n' && bytes[end] != b'\r' {
                    end += 1;
                }
                
                // Include newline in comment span
                if end < bytes.len() {
                    if bytes[end] == b'\r' && end + 1 < bytes.len() && bytes[end + 1] == b'\n' {
                        end += 2; // \r\n
                    } else {
                        end += 1; // \n
                    }
                }
                
                tokens.push((HirSpan::new(start, end), 7, 0)); // COMMENT token type = 7
                i = end;
                continue;
            } else if bytes[i + 1] == b'*' {
                // Block comment: /* or /**
                let start = i;
                let mut end = i + 2;
                
                // Find closing */
                let mut found_close = false;
                while end + 1 < bytes.len() {
                    if bytes[end] == b'*' && bytes[end + 1] == b'/' {
                        end += 2;
                        found_close = true;
                        break;
                    }
                    end += 1;
                }
                
                if found_close {
                    tokens.push((HirSpan::new(start, end), 7, 0)); // COMMENT
                }
                i = end;
                continue;
            }
        }
        i += 1;
    }
    
    // Walk through CST and extract token types
    for block in &cst.blocks {
        for stmt in &block.node.statements {
            match &stmt.node {
                CstStatement::Let { pub_keyword, let_keyword, .. } => {
                    // Add pub keyword if present
                    if let Some(pub_span) = pub_keyword {
                        tokens.push((HirSpan::new(pub_span.start as usize, pub_span.end as usize), 3, 0)); // KEYWORD
                    }
                    // Add let keyword
                    tokens.push((HirSpan::new(let_keyword.start as usize, let_keyword.end as usize), 3, 0)); // KEYWORD
                }
                CstStatement::Const { pub_keyword, const_keyword, .. } => {
                    if let Some(pub_span) = pub_keyword {
                        tokens.push((HirSpan::new(pub_span.start as usize, pub_span.end as usize), 3, 0)); // KEYWORD
                    }
                    tokens.push((HirSpan::new(const_keyword.start as usize, const_keyword.end as usize), 3, 0)); // KEYWORD
                }
                CstStatement::FunctionDeclaration { pub_keyword, fn_keyword, .. } => {
                    if let Some(pub_span) = pub_keyword {
                        tokens.push((HirSpan::new(pub_span.start as usize, pub_span.end as usize), 3, 0)); // KEYWORD
                    }
                    tokens.push((HirSpan::new(fn_keyword.start as usize, fn_keyword.end as usize), 3, 0)); // KEYWORD
                }
                CstStatement::Struct { pub_keyword, struct_keyword, .. } => {
                    if let Some(pub_span) = pub_keyword {
                        tokens.push((HirSpan::new(pub_span.start as usize, pub_span.end as usize), 3, 0)); // KEYWORD
                    }
                    tokens.push((HirSpan::new(struct_keyword.start as usize, struct_keyword.end as usize), 3, 0)); // KEYWORD
                }
                CstStatement::Return { .. } => {
                    // return keyword - handled separately if needed
                }
                CstStatement::Loop { loop_keyword, .. } => {
                    tokens.push((HirSpan::new(loop_keyword.start as usize, loop_keyword.end as usize), 3, 0)); // KEYWORD
                }
                CstStatement::While { while_keyword, .. } => {
                    tokens.push((HirSpan::new(while_keyword.start as usize, while_keyword.end as usize), 3, 0)); // KEYWORD
                }
                CstStatement::For { for_keyword, in_keyword, .. } => {
                    tokens.push((HirSpan::new(for_keyword.start as usize, for_keyword.end as usize), 3, 0)); // KEYWORD
                    tokens.push((HirSpan::new(in_keyword.start as usize, in_keyword.end as usize), 3, 0)); // KEYWORD
                }
                CstStatement::Break { .. } | CstStatement::Continue { .. } => {
                    // Control flow keywords - handled if spans available
                }
                CstStatement::Use { selector, path, use_keyword, from_keyword, .. } => {
                    eprintln!("[TOKENS] Processing Use statement:");
                    log_to_file("Processing Use statement:");
                    let use_text = &source.get(use_keyword.start as usize..use_keyword.end as usize).unwrap_or("<invalid>");
                    eprintln!("[TOKENS]   use_keyword: {}..{} = '{}'", use_keyword.start, use_keyword.end, use_text);
                    log_to_file(&format!("  use_keyword: {}..{} = '{}'", use_keyword.start, use_keyword.end, use_text));
                    let from_text = &source.get(from_keyword.start as usize..from_keyword.end as usize).unwrap_or("<invalid>");
                    eprintln!("[TOKENS]   from_keyword: {}..{} = '{}'", from_keyword.start, from_keyword.end, from_text);
                    log_to_file(&format!("  from_keyword: {}..{} = '{}'", from_keyword.start, from_keyword.end, from_text));

                    // Extract use keyword
                    tokens.push((HirSpan::new(use_keyword.start as usize, use_keyword.end as usize), 3, 0)); // KEYWORD

                    // Extract from keyword
                    tokens.push((HirSpan::new(from_keyword.start as usize, from_keyword.end as usize), 3, 0)); // KEYWORD

                    // Extract identifiers from selector (the names being imported)
                    // These will be enhanced by semantic tokens if symbols are available
                    match &selector.node {
                        crate::core::cst::CstImportSelector::Single(name) => {
                            let selector_text = &source.get(name.span.start as usize..name.span.end as usize).unwrap_or("<invalid>");
                            eprintln!("[TOKENS]   selector: {}..{} = '{}'", name.span.start, name.span.end, selector_text);
                            log_to_file(&format!("  selector: {}..{} = '{}'", name.span.start, name.span.end, selector_text));
                            let span = HirSpan::new(name.span.start as usize, name.span.end as usize);
                            tokens.push((span, 1, 0)); // VARIABLE (will be enhanced to FUNCTION if it's a function)
                        }
                        crate::core::cst::CstImportSelector::Multiple(names) => {
                            for name in names {
                                eprintln!("[TOKENS]   selector (multi): {}..{} = '{}'",
                                    name.span.start, name.span.end,
                                    &source.get(name.span.start as usize..name.span.end as usize).unwrap_or("<invalid>"));
                                let span = HirSpan::new(name.span.start as usize, name.span.end as usize);
                                tokens.push((span, 1, 0)); // VARIABLE
                            }
                        }
                        crate::core::cst::CstImportSelector::Wildcard(_) => {
                            // Wildcard import doesn't have specific identifiers
                        }
                    }

                    // Extract module path segments (e.g., "std" in "use print from std")
                    // These should be highlighted as namespaces/modules
                    for path_segment in path {
                        let path_text = &source.get(path_segment.span.start as usize..path_segment.span.end as usize).unwrap_or("<invalid>");
                        eprintln!("[TOKENS]   path: {}..{} = '{}'", path_segment.span.start, path_segment.span.end, path_text);
                        log_to_file(&format!("  path: {}..{} = '{}'", path_segment.span.start, path_segment.span.end, path_text));
                        let span = HirSpan::new(path_segment.span.start as usize, path_segment.span.end as usize);
                        tokens.push((span, 4, 0)); // NAMESPACE/MODULE
                    }
                }
                CstStatement::Mod { .. } => {
                    // mod keyword
                }
                CstStatement::If { if_keyword, else_keywords, else_keyword, .. } => {
                    tokens.push((HirSpan::new(if_keyword.start as usize, if_keyword.end as usize), 3, 0)); // KEYWORD
                    for else_kw in else_keywords {
                        tokens.push((HirSpan::new(else_kw.start as usize, else_kw.end as usize), 3, 0)); // KEYWORD
                    }
                    if let Some(else_kw) = else_keyword {
                        tokens.push((HirSpan::new(else_kw.start as usize, else_kw.end as usize), 3, 0)); // KEYWORD
                    }
                }
                CstStatement::Match { match_keyword, .. } => {
                    tokens.push((HirSpan::new(match_keyword.start as usize, match_keyword.end as usize), 3, 0)); // KEYWORD
                }
                CstStatement::Expression(expr) => {
                    walk_expr(expr, tokens);
                }
                _ => {}
            }
        }
    }
}

/// Extract identifiers from CST for basic highlighting when semantic info is unavailable.
/// 
/// This ensures we can highlight identifiers even if:
/// - Symbol table is missing
/// - Name resolution failed
/// - Coverage is too low
fn extract_identifiers_from_cst(
    cst: &crate::core::cst::CstProgram,
    tokens: &mut Vec<(crate::core::hir_lowering::Span, u32, u32)>,
) {
    use crate::core::cst::{CstExpr, CstStatement};
    use crate::core::hir_lowering::Span as HirSpan;
    
    fn walk_expr_for_identifiers(
        expr: &crate::core::cst::Spanned<CstExpr>,
        tokens: &mut Vec<(HirSpan, u32, u32)>,
    ) {
        match &expr.node {
            CstExpr::Identifier(_) => {
                // Extract identifier span - use VARIABLE type as default
                // Semantic tokens will override this if available
                let span = HirSpan::new(expr.span.start as usize, expr.span.end as usize);
                tokens.push((span, 1, 0)); // VARIABLE (will be overridden by semantic tokens if available)
            }
            CstExpr::Infix { lhs, rhs, .. } => {
                walk_expr_for_identifiers(lhs, tokens);
                walk_expr_for_identifiers(rhs, tokens);
            }
            CstExpr::Prefix { rhs, .. } => {
                walk_expr_for_identifiers(rhs, tokens);
            }
            CstExpr::Postfix { lhs, .. } => {
                walk_expr_for_identifiers(lhs, tokens);
            }
            CstExpr::FunctionCall { callee, .. } => {
                walk_expr_for_identifiers(callee, tokens);
                // TODO: Extract identifiers from arguments if needed
            }
            CstExpr::PartialCall { func, .. } => {
                walk_expr_for_identifiers(func, tokens);
            }
            CstExpr::MemberAccess { object, .. } => {
                walk_expr_for_identifiers(object, tokens);
            }
            _ => {}
        }
    }
    
    // Walk through CST and extract identifier nodes
    for block in &cst.blocks {
        for stmt in &block.node.statements {
            match &stmt.node {
                CstStatement::Let { identifier, expression, .. } |
                CstStatement::Const { identifier, expression, .. } => {
                    // Extract identifier name - use VARIABLE type
                    let span = HirSpan::new(identifier.span.start as usize, identifier.span.end as usize);
                    tokens.push((span, 1, 0)); // VARIABLE
                    walk_expr_for_identifiers(expression, tokens);
                }
                CstStatement::FunctionDeclaration { identifier, .. } => {
                    // Extract function name - use FUNCTION type
                    let span = HirSpan::new(identifier.span.start as usize, identifier.span.end as usize);
                    tokens.push((span, 0, 0)); // FUNCTION
                }
                // Use statements are handled by extract_cst_tokens() - skip here to avoid duplicates
                CstStatement::Use { .. } => {}
                CstStatement::Expression(expr) => {
                    walk_expr_for_identifiers(expr, tokens);
                }
                _ => {}
            }
        }
    }
}

/// Convert absolute spans to relative semantic tokens.
fn convert_to_semantic_tokens(
    span_tokens: Vec<(crate::core::hir_lowering::Span, u32, u32)>,
    line_index: &LineIndex,
) -> Vec<SemanticToken> {
    // Sort tokens by position
    let mut sorted = span_tokens;
    sorted.sort_by_key(|(span, _, _)| (span.start, span.end));

    let mut tokens = Vec::new();
    let mut last_line = 0u32;
    let mut last_col = 0u32;

    for (span, token_type, modifiers) in sorted {
        let (line, col) = line_index.byte_to_line_col(span.start);
        let (end_line, end_col) = line_index.byte_to_line_col(span.end);

        // Calculate deltas (first token uses absolute position, rest are relative)
        let delta_line = if tokens.is_empty() {
            line // First token: absolute line
        } else {
            line.saturating_sub(last_line) // Subsequent tokens: relative
        };

        let delta_start = if tokens.is_empty() {
            col // First token: absolute column
        } else if line == last_line {
            col.saturating_sub(last_col) // Same line: relative column
        } else {
            col // New line: absolute column (relative to start of line)
        };

        let length = if line == end_line {
            end_col.saturating_sub(col)
        } else {
            // Multi-line token - use length from start to end of first line
            // Get the last column on the first line
            let first_line_end_col = end_col.max(col);
            first_line_end_col.saturating_sub(col)
        };

        eprintln!("[CONVERT] Token at byte {}..{} -> line={}, col={}, delta_line={}, delta_start={}, length={}, type={}",
            span.start, span.end, line, col, delta_line, delta_start, length, token_type);
        log_to_file(&format!("Token at byte {}..{} -> line={}, col={}, delta_line={}, delta_start={}, length={}, type={}",
            span.start, span.end, line, col, delta_line, delta_start, length, token_type));

        tokens.push(SemanticToken {
            delta_line,
            delta_start,
            length,
            token_type,
            token_modifiers_bitset: modifiers,
        });

        last_line = line;
        // CRITICAL FIX: When calculating the next token's delta_start on the same line,
        // it should be relative to the END of this token, not the start.
        // The LSP spec requires: next_delta_start = next_col - (this_col + this_length)
        last_col = col + length;
    }

    tokens
}
