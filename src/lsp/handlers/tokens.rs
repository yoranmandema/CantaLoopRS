//! Semantic tokens handler.

use tower_lsp::lsp_types::*;

use crate::lsp::server::CantaLoopServer;
use crate::lsp::mapping::spans::LineIndex;
use crate::parse_cst_program;

// -----------------------------------------------------------------------------
// Semantic token type indices (must match initialize.rs legend order)
// -----------------------------------------------------------------------------
const TOKEN_FUNCTION: u32 = 0;
const TOKEN_VARIABLE: u32 = 1;
const TOKEN_PARAMETER: u32 = 2;
const TOKEN_KEYWORD: u32 = 3;
const TOKEN_OPERATOR: u32 = 4;
const TOKEN_STRING: u32 = 5;
const TOKEN_NUMBER: u32 = 6;
const TOKEN_COMMENT: u32 = 7;
const TOKEN_TYPE: u32 = 8;

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
            // IMPORTANT: Don't return empty tokens while compilation is in-flight (e.g. compiling native modules).
            // VSCode will "go dark" and may not request again promptly.
            // Instead, parse CST directly and return CST-only tokens.
            match parse_cst_program(&source) {
                Ok((cst, _docs)) => {
                    let tokens = generate_cst_only_semantic_tokens(&cst, &source);
                    return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                        result_id: None,
                        data: tokens,
                    })));
                }
                Err(_) => {
                    return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                        result_id: None,
                        data: vec![],
                    })));
                }
            }
        }
    };
    
    // Check if CST exists
    eprintln!("[LSP] Step 7: Checking CST...");
    if snapshot.cst(file_id).is_none() {
        eprintln!("[LSP] Step 7: ✗ No CST found (parse failed)");
        // Fall back to parsing CST from current buffer text, so tokens don't disappear.
        match parse_cst_program(&source) {
            Ok((cst, _docs)) => {
                let tokens = generate_cst_only_semantic_tokens(&cst, &source);
                return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                    result_id: None,
                    data: tokens,
                })));
            }
            Err(_) => {
                return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                    result_id: None,
                    data: vec![],
                })));
            }
        }
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
    
    // CRITICAL: Always generate CST-based tokens first (keywords, literals, operators, identifiers).
    // This ensures we have tokens even if semantic analysis failed / is partial.
    if let Some(cst) = snapshot.cst(file_id) {
        extract_cst_tokens(cst, source, &mut span_tokens);
        // Always extract identifiers as a fallback. Semantic spans will override via dedup/priority.
        extract_identifiers_from_cst(cst, &mut span_tokens);
    }
    
    // If we have semantic information, enhance tokens with symbol types
    // But never fail if this is missing - we already have CST tokens above
    if let (Some(symbols), Some(hir)) = (snapshot.symbol_table(), snapshot.hir()) {
    
        // Process all symbols from span_to_symbol map to enhance tokens with semantic types
        for (span, &symbol_id) in &symbols.span_to_symbol {
            // Get symbol info
            if let Some(info) = snapshot.symbol_info(symbol_id) {
                // Sanity check: if the span doesn't actually cover the symbol name in the current buffer,
                // skip it. Bad spans can completely break highlighting (VSCode will render nonsense).
                // Prefer CST-only tokens in these cases.
                let base_name = info.name.split('.').last().unwrap_or(info.name.as_str());
                if span.end <= source.len() {
                    let slice = &source[span.start..span.end];
                    if !slice.contains(base_name) {
                        continue;
                    }
                } else {
                    continue;
                }
                let token_type = match info.kind {
                    SymbolKind::Function => {
                        // Check if it's effectful or pure
                        // Look up function in HIR to check signature
                        let is_effectful = hir.functions.values()
                            .find(|f| f.name == info.name)
                            .map(|f| f.signature.is_effectful)
                            .unwrap_or(false);
                        
                        let _ = is_effectful; // reserved for future token modifiers
                        TOKEN_FUNCTION
                    }
                    SymbolKind::Variable => {
                        // Highlight callable variables (thunks/functions) as FUNCTION for better UX.
                        // This makes pipelines like `let effectfulPipeline = fetch(?) |> print(?);` read naturally.
                        match &info.ty {
                            crate::core::hir_lowering::ValueKind::Function(_) |
                            crate::core::hir_lowering::ValueKind::Thunk(_) |
                            crate::core::hir_lowering::ValueKind::Callable => TOKEN_FUNCTION,
                            _ => TOKEN_VARIABLE,
                        }
                    },
                    SymbolKind::Parameter => TOKEN_PARAMETER,
                    SymbolKind::Field => TOKEN_VARIABLE,
                    SymbolKind::Module | SymbolKind::Type => TOKEN_TYPE,
                };
                
                // Override or add semantic token type for this span
                // Remove any existing CST-based tokens that exactly match this span
                // Use a more precise check: remove tokens that have the exact same span
                span_tokens.retain(|(s, _, _)| {
                    // Keep tokens that don't exactly match this span
                    // This allows semantic tokens to override CST tokens for the same identifier
                    s.start != span.start || s.end != span.end
                });
                span_tokens.push((*span, token_type, 0));
            }
        }

        // If semantic analysis produced no usable spans (e.g., compilation aborted on an error),
        // fall back to CST identifier extraction so highlighting doesn't "go dark" mid-file.
        if symbols.span_to_symbol.is_empty() {
            if let Some(cst) = snapshot.cst(file_id) {
                extract_identifiers_from_cst(cst, &mut span_tokens);
            }
        }
    } else {
        // No semantic information available - use CST identifiers as a fallback (VARIABLE by default).
        // This is intentionally non-committal: it preserves readability without inventing semantics.
        if let Some(cst) = snapshot.cst(file_id) {
            extract_identifiers_from_cst(cst, &mut span_tokens);
        }
        log::warn!("semantic coverage low — falling back to CST-only tokens for file_id {:?}", file_id);
    }

    // Filter out invalid/empty tokens
    // Only filter truly invalid spans (end < start or zero-length)
    span_tokens.retain(|(span, _, _)| {
        let len = span.end.saturating_sub(span.start);
        len > 0 && span.start < span.end // Valid non-empty span
    });

    // Deduplicate overlapping tokens before conversion
    // Keep only one token per position - prefer semantic tokens (those with higher priority types)
    // Sort by start position, then by span size, then by explicit priority.
    fn token_priority(token_type: u32) -> u32 {
        match token_type {
            TOKEN_FUNCTION => 100,
            TOKEN_PARAMETER => 95,
            TOKEN_VARIABLE => 90,
            TOKEN_TYPE => 85,
            TOKEN_KEYWORD => 80,
            TOKEN_STRING => 70,
            TOKEN_NUMBER => 70,
            TOKEN_COMMENT => 60,
            TOKEN_OPERATOR => 50,
            _ => 0,
        }
    }

    span_tokens.sort_by(|a: &(HirSpan, u32, u32), b: &(HirSpan, u32, u32)| {
        // First sort by start position
        a.0.start.cmp(&b.0.start)
            // Then by explicit priority (higher wins)
            .then(token_priority(b.1).cmp(&token_priority(a.1)))
            // Finally by span size (smaller = more specific within the same category)
            .then(a.0.end.cmp(&b.0.end))
    });

    // Remove overlapping tokens - keep the most specific (smallest span) token
    // Two spans overlap if they share any byte position
    // Spans are half-open: [start, end), so they overlap if:
    //   !(span1.end <= span2.start || span1.start >= span2.end)
    let mut deduplicated = Vec::new();
    for token in span_tokens {
        let (span, token_type, modifiers) = token;
        let overlaps = deduplicated.iter().any(|(existing_span, _, _): &(HirSpan, u32, u32)| {
            // Check if this token overlaps with any existing token
            // Spans overlap if they share any byte position
            span.start < existing_span.end && existing_span.start < span.end
        });
        
        if !overlaps {
            deduplicated.push((span, token_type, modifiers));
        } else {
            // If it overlaps, check if this token is more specific (smaller span)
            // and should replace the existing one
            if let Some((idx, (existing_span, existing_type, _))) = deduplicated.iter()
                .enumerate()
                .find(|(_, (existing_span, _, _))| {
                    span.start < existing_span.end && existing_span.start < span.end
                })
            {
                let existing_len = existing_span.end - existing_span.start;
                let new_len = span.end - span.start;
                let existing_prio = token_priority(*existing_type);
                let new_prio = token_priority(token_type);
                // Replace if new token is higher priority; if equal priority, prefer smaller span.
                if new_prio > existing_prio || (new_prio == existing_prio && new_len < existing_len) {
                    deduplicated[idx] = (span, token_type, modifiers);
                }
            }
        }
    }

    // Convert spans to semantic tokens (relative deltas)
    convert_to_semantic_tokens(deduplicated, &line_index)
}

fn generate_cst_only_semantic_tokens(
    cst: &crate::core::cst::CstProgram,
    source: &str,
) -> Vec<SemanticToken> {
    use crate::core::hir_lowering::Span as HirSpan;

    let line_index = LineIndex::new(source);
    let mut span_tokens: Vec<(HirSpan, u32, u32)> = Vec::new();

    extract_cst_tokens(cst, source, &mut span_tokens);
    extract_identifiers_from_cst(cst, &mut span_tokens);

    // Filter out invalid/empty tokens
    span_tokens.retain(|(span, _, _)| {
        let len = span.end.saturating_sub(span.start);
        len > 0 && span.start < span.end
    });

    fn token_priority(token_type: u32) -> u32 {
        match token_type {
            TOKEN_FUNCTION => 100,
            TOKEN_PARAMETER => 95,
            TOKEN_VARIABLE => 90,
            TOKEN_TYPE => 85,
            TOKEN_KEYWORD => 80,
            TOKEN_STRING => 70,
            TOKEN_NUMBER => 70,
            TOKEN_COMMENT => 60,
            TOKEN_OPERATOR => 50,
            _ => 0,
        }
    }

    span_tokens.sort_by(|a: &(HirSpan, u32, u32), b: &(HirSpan, u32, u32)| {
        a.0.start
            .cmp(&b.0.start)
            .then(token_priority(b.1).cmp(&token_priority(a.1)))
            .then(a.0.end.cmp(&b.0.end))
    });

    let mut deduplicated = Vec::new();
    for (span, token_type, modifiers) in span_tokens {
        let overlaps = deduplicated.iter().any(|(existing_span, _, _): &(HirSpan, u32, u32)| {
            span.start < existing_span.end && existing_span.start < span.end
        });

        if !overlaps {
            deduplicated.push((span, token_type, modifiers));
        } else if let Some((idx, (existing_span, existing_type, _))) = deduplicated
            .iter()
            .enumerate()
            .find(|(_, (existing_span, _, _))| span.start < existing_span.end && existing_span.start < span.end)
        {
            let existing_len = existing_span.end - existing_span.start;
            let new_len = span.end - span.start;
            let existing_prio = token_priority(*existing_type);
            let new_prio = token_priority(token_type);
            if new_prio > existing_prio || (new_prio == existing_prio && new_len < existing_len) {
                deduplicated[idx] = (span, token_type, modifiers);
            }
        }
    }

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

    fn push_type_idents_from_span(tokens: &mut Vec<(HirSpan, u32, u32)>, source: &str, span: &crate::core::cst::Span) {
        // Highlight each identifier-like segment inside a type annotation span as TYPE.
        // Examples:
        // - ": num"            -> "num"
        // - "num"              -> "num"
        // - "[num]"            -> "num"
        // - "math.Point"       -> "math", "Point" (good enough until member-access typing for types is added)
        let start = span.start as usize;
        let end = span.end as usize;
        let Some(text) = source.get(start..end) else { return };

        let bytes = text.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            let b = bytes[i];
            let is_ident_start = (b as char).is_ascii_alphabetic() || b == b'_';
            if !is_ident_start {
                i += 1;
                continue;
            }
            let ident_start = i;
            i += 1;
            while i < bytes.len() {
                let b = bytes[i];
                let is_ident_cont = (b as char).is_ascii_alphanumeric() || b == b'_';
                if !is_ident_cont {
                    break;
                }
                i += 1;
            }
            let ident_end = i;
            let abs_start = start + ident_start;
            let abs_end = start + ident_end;
            tokens.push((HirSpan::new(abs_start, abs_end), TOKEN_TYPE, 0));
        }
    }
    
    fn walk_expr(expr: &crate::core::cst::Spanned<CstExpr>, tokens: &mut Vec<(HirSpan, u32, u32)>, source: &str) {
        fn is_module_like_ident(name: &str) -> bool {
            // Heuristic for CST-only highlighting:
            // - Treat well-known modules/namespaces as TYPE (e.g. `string.len`, `math.floor`)
            // - Treat UpperCamelCase identifiers as TYPE (likely user-defined types)
            // - Otherwise default to VARIABLE (prevents `state.iter` from coloring `state` as a type)
            matches!(name,
                "array" | "math" | "string" | "std" | "functional" | "bevy" | "matrix"
            ) || name.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false)
        }

        match &expr.node {
            CstExpr::Literal(lit) => {
                // Use the literal's own span, not the expression span
                let span = HirSpan::new(lit.span.start as usize, lit.span.end as usize);
                match &lit.node {
                    crate::core::cst::CstLiteral::String(_) => tokens.push((span, TOKEN_STRING, 0)),
                    crate::core::cst::CstLiteral::Number(_) => tokens.push((span, TOKEN_NUMBER, 0)),
                    crate::core::cst::CstLiteral::Boolean(_) => {
                        // Booleans are keywords in CantaLoop
                        tokens.push((span, TOKEN_KEYWORD, 0))
                    }
                }
            }
            CstExpr::Infix { lhs, op, rhs, .. } => {
                walk_expr(lhs, tokens, source);
                // Extract operator span
                let op_span = HirSpan::new(op.span.start as usize, op.span.end as usize);
                tokens.push((op_span, TOKEN_OPERATOR, 0));
                walk_expr(rhs, tokens, source);
            }
            CstExpr::Prefix { op, rhs, .. } => {
                // Extract operator span
                let op_span = HirSpan::new(op.span.start as usize, op.span.end as usize);
                tokens.push((op_span, TOKEN_OPERATOR, 0));
                walk_expr(rhs, tokens, source);
            }
            CstExpr::Postfix { lhs, op, .. } => {
                walk_expr(lhs, tokens, source);
                // Extract operator span
                let op_span = HirSpan::new(op.span.start as usize, op.span.end as usize);
                tokens.push((op_span, TOKEN_OPERATOR, 0));
            }
            CstExpr::FunctionCall { callee, arguments, .. } => {
                walk_expr(callee, tokens, source);
                for arg in arguments {
                    match &arg.node {
                        crate::core::cst::CstCallArgument::Expr(expr) => {
                            walk_expr(expr, tokens, source);
                        }
                        crate::core::cst::CstCallArgument::Hole(_) => {}
                    }
                }
                // Don't add parentheses - they're punctuation, not semantic tokens
            }
            CstExpr::Group { inner, .. } => {
                walk_expr(inner, tokens, source);
                // Don't add parentheses - they're punctuation
            }
            CstExpr::Array { elements, .. } => {
                for elem in elements {
                    walk_expr(elem, tokens, source);
                }
                // Don't add brackets - they're punctuation
            }
            CstExpr::ArrayIndex { array, indices, .. } => {
                walk_expr(array, tokens, source);
                for idx in indices {
                    match &idx.node {
                        crate::core::cst::CstIndexSpec::Single(expr) => {
                            walk_expr(expr, tokens, source);
                        }
                        crate::core::cst::CstIndexSpec::Range { start, end, .. } => {
                            if let Some(start) = start {
                                walk_expr(start, tokens, source);
                            }
                            if let Some(end) = end {
                                walk_expr(end, tokens, source);
                            }
                        }
                        crate::core::cst::CstIndexSpec::InclusiveRange { start, end, .. } => {
                            if let Some(start) = start {
                                walk_expr(start, tokens, source);
                            }
                            if let Some(end) = end {
                                walk_expr(end, tokens, source);
                            }
                        }
                    }
                }
                // Don't add brackets - they're punctuation
            }
            CstExpr::Compose { lhs, op, rhs, .. } => {
                walk_expr(lhs, tokens, source);
                // Extract compose operator (|>)
                let op_span = HirSpan::new(op.span.start as usize, op.span.end as usize);
                tokens.push((op_span, TOKEN_OPERATOR, 0));
                walk_expr(rhs, tokens, source);
            }
            CstExpr::FieldAccess { object, field, .. } => {
                walk_expr(object, tokens, source);
                // Fallback: fields are not always represented as symbols, so default to VARIABLE.
                let span = HirSpan::new(field.span.start as usize, field.span.end as usize);
                tokens.push((span, TOKEN_VARIABLE, 0));
            }
            CstExpr::MemberAccess { object, members, .. } => {
                // In member-access, the base identifier is typically a module/namespace path.
                // Provide a best-effort TYPE token for it, even without semantic resolution.
                if let CstExpr::Identifier(name) = &object.node {
                    let span = HirSpan::new(name.span.start as usize, name.span.end as usize);
                    let tt = if is_module_like_ident(&name.node) { TOKEN_TYPE } else { TOKEN_VARIABLE };
                    tokens.push((span, tt, 0));
                } else {
                    walk_expr(object, tokens, source);
                }

                // Fallback: member segments default to VARIABLE unless semantic resolution overrides them.
                for m in members {
                    let span = HirSpan::new(m.span.start as usize, m.span.end as usize);
                    tokens.push((span, TOKEN_VARIABLE, 0));
                }
            }
            CstExpr::PartialCall { func, args, .. } => {
                walk_expr(func, tokens, source);
                for arg in args {
                    match &arg.node {
                        crate::core::cst::CstCallArgument::Expr(expr) => {
                            walk_expr(expr, tokens, source);
                        }
                        crate::core::cst::CstCallArgument::Hole(_) => {}
                    }
                }
            }
            CstExpr::Closure { fn_keyword, arguments, return_type_arrow, arrow, body, .. } => {
                // `fn` keyword
                tokens.push((HirSpan::new(fn_keyword.start as usize, fn_keyword.end as usize), TOKEN_KEYWORD, 0));

                // Return-type arrow (`->` / `~>`) and body arrow (`=>`) are operators.
                if let Some(rta) = return_type_arrow {
                    tokens.push((HirSpan::new(rta.node.arrow.start as usize, rta.node.arrow.end as usize), TOKEN_OPERATOR, 0));
                }
                if let Some(arr) = arrow {
                    tokens.push((HirSpan::new(arr.start as usize, arr.end as usize), TOKEN_OPERATOR, 0));
                }

                // Type annotations in closure args/return.
                for arg in arguments {
                    if let Some(ty) = &arg.node.type_annotation {
                        push_type_idents_from_span(tokens, source, &ty.span);
                    }
                }
                if let Some(rta) = return_type_arrow {
                    push_type_idents_from_span(tokens, source, &rta.node.type_annotation.span);
                }
                match body {
                    crate::core::cst::CstClosureBody::Expression(expr) => {
                        walk_expr(expr, tokens, source);
                    }
                    crate::core::cst::CstClosureBody::Block(b) => {
                        // CRITICAL: closure bodies contain most real-world code; we must walk them to
                        // produce CST fallback tokens (and to reach nested closures/type annotations).
                        walk_block(&b.node, tokens, source);
                    }
                }
            }
            CstExpr::Loop { init_vars, body, .. } => {
                for (_, _, expr) in init_vars {
                    walk_expr(expr, tokens, source);
                }
                walk_block(&body.node, tokens, source);
            }
            CstExpr::StructInit { fields, .. } => {
                // Highlight the struct name itself as a TYPE, even without semantic resolution.
                // (We don't currently record struct types in the symbol table.)
                if let crate::core::cst::CstExpr::StructInit { struct_name, .. } = &expr.node {
                    let span = HirSpan::new(struct_name.span.start as usize, struct_name.span.end as usize);
                    tokens.push((span, TOKEN_TYPE, 0));
                }
                for _field in fields {
                    // Field values will be handled by expression walking
                }
            }
            CstExpr::Identifier(_) => {
                // Identifiers will be handled by semantic tokens (from symbol table)
                // Don't add them here as CST tokens
            }
        }
    }

    fn walk_block(block: &crate::core::cst::CstBlock, tokens: &mut Vec<(HirSpan, u32, u32)>, source: &str) {
        for stmt in &block.statements {
            walk_stmt(&stmt.node, tokens, source);
        }
    }

    fn walk_stmt(stmt: &CstStatement, tokens: &mut Vec<(HirSpan, u32, u32)>, source: &str) {
        match stmt {
            CstStatement::Let { pub_keyword, let_keyword, type_annotation, expression, .. } => {
                if let Some(pub_span) = pub_keyword {
                    tokens.push((HirSpan::new(pub_span.start as usize, pub_span.end as usize), TOKEN_KEYWORD, 0));
                }
                tokens.push((HirSpan::new(let_keyword.start as usize, let_keyword.end as usize), TOKEN_KEYWORD, 0));
                if let Some(ty) = type_annotation {
                    push_type_idents_from_span(tokens, source, &ty.span);
                }
                walk_expr(expression, tokens, source);
            }
            CstStatement::Const { pub_keyword, const_keyword, expression, .. } => {
                if let Some(pub_span) = pub_keyword {
                    tokens.push((HirSpan::new(pub_span.start as usize, pub_span.end as usize), TOKEN_KEYWORD, 0));
                }
                tokens.push((HirSpan::new(const_keyword.start as usize, const_keyword.end as usize), TOKEN_KEYWORD, 0));
                walk_expr(expression, tokens, source);
            }
            CstStatement::FunctionDeclaration { pub_keyword, fn_keyword, arguments, return_type_arrow, body, .. } => {
                if let Some(pub_span) = pub_keyword {
                    tokens.push((HirSpan::new(pub_span.start as usize, pub_span.end as usize), TOKEN_KEYWORD, 0));
                }
                tokens.push((HirSpan::new(fn_keyword.start as usize, fn_keyword.end as usize), TOKEN_KEYWORD, 0));
                for arg in arguments {
                    push_type_idents_from_span(tokens, source, &arg.node.type_annotation.span);
                }
                if let Some(rta) = return_type_arrow {
                    push_type_idents_from_span(tokens, source, &rta.node.type_annotation.span);
                }
                walk_block(&body.node, tokens, source);
            }
            CstStatement::Struct { pub_keyword, struct_keyword, fields, name, .. } => {
                if let Some(pub_span) = pub_keyword {
                    tokens.push((HirSpan::new(pub_span.start as usize, pub_span.end as usize), TOKEN_KEYWORD, 0));
                }
                tokens.push((HirSpan::new(struct_keyword.start as usize, struct_keyword.end as usize), TOKEN_KEYWORD, 0));
                tokens.push((HirSpan::new(name.span.start as usize, name.span.end as usize), TOKEN_TYPE, 0));
                for f in fields {
                    push_type_idents_from_span(tokens, source, &f.node.type_annotation.span);
                }
            }
            CstStatement::Return { return_keyword, expression } => {
                tokens.push((HirSpan::new(return_keyword.start as usize, return_keyword.end as usize), TOKEN_KEYWORD, 0));
                walk_expr(expression, tokens, source);
            }
            CstStatement::If { if_keyword, arms, else_keywords, else_keyword, else_block } => {
                tokens.push((HirSpan::new(if_keyword.start as usize, if_keyword.end as usize), TOKEN_KEYWORD, 0));
                for else_kw in else_keywords {
                    tokens.push((HirSpan::new(else_kw.start as usize, else_kw.end as usize), TOKEN_KEYWORD, 0));
                }
                if let Some(else_kw) = else_keyword {
                    tokens.push((HirSpan::new(else_kw.start as usize, else_kw.end as usize), TOKEN_KEYWORD, 0));
                }
                for (cond, body) in arms {
                    walk_expr(cond, tokens, source);
                    walk_block(&body.node, tokens, source);
                }
                if let Some(eb) = else_block {
                    walk_block(&eb.node, tokens, source);
                }
            }
            CstStatement::While { while_keyword, condition, body } => {
                tokens.push((HirSpan::new(while_keyword.start as usize, while_keyword.end as usize), TOKEN_KEYWORD, 0));
                walk_expr(condition, tokens, source);
                walk_block(&body.node, tokens, source);
            }
            CstStatement::Loop { loop_keyword, init_vars, body } => {
                tokens.push((HirSpan::new(loop_keyword.start as usize, loop_keyword.end as usize), TOKEN_KEYWORD, 0));
                for (_, _, expr) in init_vars {
                    walk_expr(expr, tokens, source);
                }
                walk_block(&body.node, tokens, source);
            }
            CstStatement::For { for_keyword, in_keyword, start, end, body, .. } => {
                tokens.push((HirSpan::new(for_keyword.start as usize, for_keyword.end as usize), TOKEN_KEYWORD, 0));
                tokens.push((HirSpan::new(in_keyword.start as usize, in_keyword.end as usize), TOKEN_KEYWORD, 0));
                walk_expr(start, tokens, source);
                walk_expr(end, tokens, source);
                walk_block(&body.node, tokens, source);
            }
            CstStatement::Break { break_keyword, expression } => {
                tokens.push((HirSpan::new(break_keyword.start as usize, break_keyword.end as usize), TOKEN_KEYWORD, 0));
                if let Some(expr) = expression {
                    walk_expr(expr, tokens, source);
                }
            }
            CstStatement::Continue { continue_keyword } => {
                tokens.push((HirSpan::new(continue_keyword.start as usize, continue_keyword.end as usize), TOKEN_KEYWORD, 0));
            }
            CstStatement::Match { match_keyword, expression, cases } => {
                tokens.push((HirSpan::new(match_keyword.start as usize, match_keyword.end as usize), TOKEN_KEYWORD, 0));
                walk_expr(expression, tokens, source);
                for (pat, body) in cases {
                    if let Some(p) = pat {
                        walk_expr(p, tokens, source);
                    }
                    walk_block(&body.node, tokens, source);
                }
            }
            CstStatement::Use { selector, path, use_keyword, from_keyword, .. } => {
                // Keep existing detailed logging, but also make it recursive-safe via stmt walker.
                eprintln!("[TOKENS] Processing Use statement:");
                log_to_file("Processing Use statement:");
                let use_text = &source.get(use_keyword.start as usize..use_keyword.end as usize).unwrap_or("<invalid>");
                eprintln!("[TOKENS]   use_keyword: {}..{} = '{}'", use_keyword.start, use_keyword.end, use_text);
                log_to_file(&format!("  use_keyword: {}..{} = '{}'", use_keyword.start, use_keyword.end, use_text));
                let from_text = &source.get(from_keyword.start as usize..from_keyword.end as usize).unwrap_or("<invalid>");
                eprintln!("[TOKENS]   from_keyword: {}..{} = '{}'", from_keyword.start, from_keyword.end, from_text);
                log_to_file(&format!("  from_keyword: {}..{} = '{}'", from_keyword.start, from_keyword.end, from_text));

                tokens.push((HirSpan::new(use_keyword.start as usize, use_keyword.end as usize), TOKEN_KEYWORD, 0));
                tokens.push((HirSpan::new(from_keyword.start as usize, from_keyword.end as usize), TOKEN_KEYWORD, 0));

                match &selector.node {
                    crate::core::cst::CstImportSelector::Single(name) => {
                        let selector_text = &source.get(name.span.start as usize..name.span.end as usize).unwrap_or("<invalid>");
                        eprintln!("[TOKENS]   selector: {}..{} = '{}'", name.span.start, name.span.end, selector_text);
                        log_to_file(&format!("  selector: {}..{} = '{}'", name.span.start, name.span.end, selector_text));
                        let span = HirSpan::new(name.span.start as usize, name.span.end as usize);
                        tokens.push((span, TOKEN_VARIABLE, 0));
                    }
                    crate::core::cst::CstImportSelector::Multiple(names) => {
                        for name in names {
                            eprintln!("[TOKENS]   selector (multi): {}..{} = '{}'",
                                name.span.start, name.span.end,
                                &source.get(name.span.start as usize..name.span.end as usize).unwrap_or("<invalid>"));
                            let span = HirSpan::new(name.span.start as usize, name.span.end as usize);
                            tokens.push((span, TOKEN_VARIABLE, 0));
                        }
                    }
                    crate::core::cst::CstImportSelector::Wildcard(_) => {}
                }

                for path_segment in path {
                    let path_text = &source.get(path_segment.span.start as usize..path_segment.span.end as usize).unwrap_or("<invalid>");
                    eprintln!("[TOKENS]   path: {}..{} = '{}'", path_segment.span.start, path_segment.span.end, path_text);
                    log_to_file(&format!("  path: {}..{} = '{}'", path_segment.span.start, path_segment.span.end, path_text));
                    let span = HirSpan::new(path_segment.span.start as usize, path_segment.span.end as usize);
                    tokens.push((span, TOKEN_TYPE, 0));
                }
            }
            CstStatement::Expression(expr) => {
                walk_expr(expr, tokens, source);
            }
            _ => {}
        }
    }
    
    // Extract ALL types of comments from source text
    // Look for // (line comments), /// (doc comments), /* */ (block comments), and /** */ (doc block comments)
    let bytes = source.as_bytes();
    let mut i = 0;
    let mut in_string = false;
    let mut escape_next = false;
    while i < bytes.len() {
        if in_string {
            if escape_next {
                escape_next = false;
                i += 1;
                continue;
            }
            if bytes[i] == b'\\' {
                escape_next = true;
                i += 1;
                continue;
            }
            if bytes[i] == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        } else if bytes[i] == b'"' {
            in_string = true;
            i += 1;
            continue;
        }

        if i + 1 < bytes.len() && bytes[i] == b'/' {
            if bytes[i + 1] == b'/' {
                // Line comment: // or ///
                let start = i;
                let mut end = i + 2;
                
                // Find end of line (but don't include the newline)
                while end < bytes.len() && bytes[end] != b'\n' && bytes[end] != b'\r' {
                    end += 1;
                }
                
                // CRITICAL: Don't include newline in comment span
                // LSP semantic tokens should not include newlines
                // The comment ends at the newline character, not including it
                
                tokens.push((HirSpan::new(start, end), TOKEN_COMMENT, 0));
                
                // Skip past the newline for next iteration
                if end < bytes.len() {
                    if bytes[end] == b'\r' && end + 1 < bytes.len() && bytes[end + 1] == b'\n' {
                        i = end + 2; // Skip \r\n
                    } else {
                        i = end + 1; // Skip \n
                    }
                } else {
                    i = end;
                }
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
                    tokens.push((HirSpan::new(start, end), TOKEN_COMMENT, 0));
                }
                i = end;
                continue;
            }
        }
        i += 1;
    }
    
    // Walk through CST and extract token types (including nested blocks).
    for block in &cst.blocks {
        walk_block(&block.node, tokens, source);
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
    
    fn is_module_like_ident(name: &str) -> bool {
        matches!(name,
            "array" | "math" | "string" | "std" | "functional" | "bevy" | "matrix"
        ) || name.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false)
    }

    fn walk_expr_for_identifiers(
        expr: &crate::core::cst::Spanned<CstExpr>,
        tokens: &mut Vec<(HirSpan, u32, u32)>,
    ) {
        match &expr.node {
            CstExpr::Identifier(_) => {
                // Extract identifier span - use VARIABLE type as default
                // Semantic tokens will override this if available
                let span = HirSpan::new(expr.span.start as usize, expr.span.end as usize);
                tokens.push((span, TOKEN_VARIABLE, 0)); // default identifier token (semantic overrides when available)
            }
            CstExpr::Infix { lhs, rhs, .. } => {
                walk_expr_for_identifiers(lhs, tokens);
                walk_expr_for_identifiers(rhs, tokens);
            }
            CstExpr::Compose { lhs, rhs, .. } => {
                // Pipelines/desugaring should not make highlighting go dark.
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
                if let CstExpr::FunctionCall { arguments, .. } = &expr.node {
                    for a in arguments {
                        if let crate::core::cst::CstCallArgument::Expr(e) = &a.node {
                            walk_expr_for_identifiers(e, tokens);
                        }
                    }
                }
            }
            CstExpr::PartialCall { func, .. } => {
                walk_expr_for_identifiers(func, tokens);
                if let CstExpr::PartialCall { args, .. } = &expr.node {
                    for a in args {
                        if let crate::core::cst::CstCallArgument::Expr(e) = &a.node {
                            walk_expr_for_identifiers(e, tokens);
                        }
                    }
                }
            }
            CstExpr::FieldAccess { object, field, .. } => {
                walk_expr_for_identifiers(object, tokens);
                let span = HirSpan::new(field.span.start as usize, field.span.end as usize);
                tokens.push((span, TOKEN_VARIABLE, 0));
            }
            CstExpr::MemberAccess { object, members, .. } => {
                if let crate::core::cst::CstExpr::Identifier(name) = &object.node {
                    let span = HirSpan::new(name.span.start as usize, name.span.end as usize);
                    let tt = if is_module_like_ident(&name.node) { TOKEN_TYPE } else { TOKEN_VARIABLE };
                    tokens.push((span, tt, 0));
                } else {
                    walk_expr_for_identifiers(object, tokens);
                }
                for m in members {
                    let span = HirSpan::new(m.span.start as usize, m.span.end as usize);
                    tokens.push((span, TOKEN_VARIABLE, 0));
                }
            }
            CstExpr::StructInit { struct_name, fields, .. } => {
                // Struct name as TYPE
                let span = HirSpan::new(struct_name.span.start as usize, struct_name.span.end as usize);
                tokens.push((span, TOKEN_TYPE, 0));
                for f in fields {
                    let name_span = HirSpan::new(f.node.name.span.start as usize, f.node.name.span.end as usize);
                    tokens.push((name_span, TOKEN_VARIABLE, 0));
                    walk_expr_for_identifiers(&f.node.value, tokens);
                }
            }
            CstExpr::Array { elements, .. } => {
                for e in elements {
                    walk_expr_for_identifiers(e, tokens);
                }
            }
            CstExpr::ArrayIndex { array, indices, .. } => {
                walk_expr_for_identifiers(array, tokens);
                for idx in indices {
                    match &idx.node {
                        crate::core::cst::CstIndexSpec::Single(e) => walk_expr_for_identifiers(e, tokens),
                        crate::core::cst::CstIndexSpec::Range { start, end, .. }
                        | crate::core::cst::CstIndexSpec::InclusiveRange { start, end, .. } => {
                            if let Some(s) = start {
                                walk_expr_for_identifiers(s, tokens);
                            }
                            if let Some(e) = end {
                                walk_expr_for_identifiers(e, tokens);
                            }
                        }
                    }
                }
            }
            CstExpr::Group { inner, .. } => walk_expr_for_identifiers(inner, tokens),
            CstExpr::Closure { arguments, body, .. } => {
                for a in arguments {
                    if !a.node.is_placeholder {
                        let s = HirSpan::new(a.node.identifier.span.start as usize, a.node.identifier.span.end as usize);
                        tokens.push((s, TOKEN_PARAMETER, 0));
                    }
                }
                match body {
                    crate::core::cst::CstClosureBody::Expression(e) => walk_expr_for_identifiers(e, tokens),
                    crate::core::cst::CstClosureBody::Block(b) => {
                        fn walk_block_for_identifiers(
                            block: &crate::core::cst::CstBlock,
                            tokens: &mut Vec<(HirSpan, u32, u32)>,
                            walk_expr_for_identifiers: &impl Fn(&crate::core::cst::Spanned<CstExpr>, &mut Vec<(HirSpan, u32, u32)>),
                        ) {
                            for st in &block.statements {
                                walk_stmt_for_identifiers(&st.node, tokens, walk_expr_for_identifiers);
                            }
                        }

                        fn walk_stmt_for_identifiers(
                            stmt: &CstStatement,
                            tokens: &mut Vec<(HirSpan, u32, u32)>,
                            walk_expr_for_identifiers: &impl Fn(&crate::core::cst::Spanned<CstExpr>, &mut Vec<(HirSpan, u32, u32)>),
                        ) {
                            match stmt {
                                CstStatement::Let { identifier, expression, .. } => {
                                    let s = HirSpan::new(identifier.span.start as usize, identifier.span.end as usize);
                                    tokens.push((s, TOKEN_VARIABLE, 0));
                                    walk_expr_for_identifiers(expression, tokens);
                                }
                                CstStatement::Const { identifier, expression, .. } => {
                                    let s = HirSpan::new(identifier.span.start as usize, identifier.span.end as usize);
                                    tokens.push((s, TOKEN_VARIABLE, 0));
                                    walk_expr_for_identifiers(expression, tokens);
                                }
                                CstStatement::Assign { identifier, expression, .. }
                                | CstStatement::AssignIncrement { identifier, expression, .. }
                                | CstStatement::AssignDecrement { identifier, expression, .. } => {
                                    let s = HirSpan::new(identifier.span.start as usize, identifier.span.end as usize);
                                    tokens.push((s, TOKEN_VARIABLE, 0));
                                    walk_expr_for_identifiers(expression, tokens);
                                }
                                CstStatement::Expression(e) => walk_expr_for_identifiers(e, tokens),
                                CstStatement::Return { expression, .. } => walk_expr_for_identifiers(expression, tokens),
                                CstStatement::If { arms, else_block, .. } => {
                                    for (cond, blk) in arms {
                                        walk_expr_for_identifiers(cond, tokens);
                                        walk_block_for_identifiers(&blk.node, tokens, walk_expr_for_identifiers);
                                    }
                                    if let Some(eb) = else_block {
                                        walk_block_for_identifiers(&eb.node, tokens, walk_expr_for_identifiers);
                                    }
                                }
                                CstStatement::While { condition, body, .. } => {
                                    walk_expr_for_identifiers(condition, tokens);
                                    walk_block_for_identifiers(&body.node, tokens, walk_expr_for_identifiers);
                                }
                                CstStatement::Loop { init_vars, body, .. } => {
                                    for (var, _, expr) in init_vars {
                                        let s = HirSpan::new(var.span.start as usize, var.span.end as usize);
                                        tokens.push((s, TOKEN_VARIABLE, 0));
                                        walk_expr_for_identifiers(expr, tokens);
                                    }
                                    walk_block_for_identifiers(&body.node, tokens, walk_expr_for_identifiers);
                                }
                                CstStatement::Match { expression, cases, .. } => {
                                    walk_expr_for_identifiers(expression, tokens);
                                    for (pat, blk) in cases {
                                        if let Some(p) = pat {
                                            walk_expr_for_identifiers(p, tokens);
                                        }
                                        walk_block_for_identifiers(&blk.node, tokens, walk_expr_for_identifiers);
                                    }
                                }
                                _ => {}
                            }
                        }

                        walk_block_for_identifiers(&b.node, tokens, &|e, t| walk_expr_for_identifiers(e, t));
                    }
                }
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
                    tokens.push((span, TOKEN_VARIABLE, 0));
                    walk_expr_for_identifiers(expression, tokens);
                }
                CstStatement::FunctionDeclaration { identifier, .. } => {
                    // Extract function name - use FUNCTION type
                    let span = HirSpan::new(identifier.span.start as usize, identifier.span.end as usize);
                    tokens.push((span, TOKEN_FUNCTION, 0));
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
        let (start_line, start_col) = line_index.byte_to_line_col(span.start);
        let (end_line, end_col) = line_index.byte_to_line_col(span.end);

        // LSP semantic tokens must be single-line unless multiline support is explicitly advertised.
        // Split multi-line spans into one token per line.
        let mut segments: Vec<(u32, u32, u32)> = Vec::new(); // (line, col, length)
        if start_line == end_line {
            let length = end_col.saturating_sub(start_col);
            if length > 0 {
                segments.push((start_line, start_col, length));
            }
        } else {
            // First line: from start_col to end-of-line (excluding the newline char)
            let first_line_end_byte = line_index.line_col_to_byte(start_line + 1, 0);
            let first_line_last_col = line_index
                .byte_to_line_col(first_line_end_byte.saturating_sub(1))
                .1;
            let first_len = (first_line_last_col + 1).saturating_sub(start_col);
            if first_len > 0 {
                segments.push((start_line, start_col, first_len));
            }

            // Middle full lines
            let mut line = start_line + 1;
            while line < end_line {
                let line_end_byte = line_index.line_col_to_byte(line + 1, 0);
                let last_col = line_index.byte_to_line_col(line_end_byte.saturating_sub(1)).1;
                let len = last_col + 1;
                if len > 0 {
                    segments.push((line, 0, len));
                }
                line += 1;
            }

            // Last line: from 0 to end_col
            if end_col > 0 {
                segments.push((end_line, 0, end_col));
            }
        }

        for (line, col, length) in segments {
            // Calculate deltas (first token uses absolute position, rest are relative)
            let delta_line = if tokens.is_empty() {
                line // First token: absolute line
            } else {
                line.saturating_sub(last_line) // Subsequent tokens: relative
            };

            let delta_start = if tokens.is_empty() {
                col // First token: absolute column
            } else if line == last_line {
                col.saturating_sub(last_col) // Same line: relative to previous token START
            } else {
                col // New line: absolute column (relative to start of line)
            };

            eprintln!("[CONVERT] Token at line={}, col={}, len={}, delta_line={}, delta_start={}, type={}",
                line, col, length, delta_line, delta_start, token_type);
            log_to_file(&format!("Token at line={}, col={}, len={}, delta_line={}, delta_start={}, type={}",
                line, col, length, delta_line, delta_start, token_type));

            tokens.push(SemanticToken {
                delta_line,
                delta_start,
                length,
                token_type,
                token_modifiers_bitset: modifiers,
            });

            last_line = line;
            last_col = col;
        }
    }

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::compiler_state::CompilerState;
    use crate::core::source_manager::SourceManager;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::RwLock;

    static FILE_COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn byte_to_line_col(source: &str, byte: usize) -> (u32, u32) {
        let mut line: u32 = 0;
        let mut col: u32 = 0;
        let mut i = 0usize;
        for ch in source.chars() {
            if i >= byte {
                break;
            }
            if ch == '\n' {
                line += 1;
                col = 0;
            } else {
                col += 1;
            }
            i += ch.len_utf8();
        }
        (line, col)
    }

    fn decode_semantic_tokens(tokens: &[SemanticToken]) -> Vec<(u32, u32, u32, u32)> {
        let mut out = Vec::new();
        let mut line: u32 = 0;
        let mut col: u32 = 0;
        for t in tokens {
            line = line.saturating_add(t.delta_line);
            if t.delta_line == 0 {
                col = col.saturating_add(t.delta_start);
            } else {
                col = t.delta_start;
            }
            out.push((line, col, t.length, t.token_type));
            // IMPORTANT: LSP semantic tokens encode `delta_start` relative to the previous token's START,
            // not its end. So we must NOT advance `col` by `length` here.
        }
        out
    }

    async fn compile_source(source: &str) -> (crate::core::source_manager::FileId, Arc<crate::core::lsp_api::CompilerSnapshot>) {
        let source_manager = Arc::new(RwLock::new(SourceManager::new()));
        let compiler_state = CompilerState::new(source_manager.clone());

        // Use a unique file per test to avoid cross-test interference when tests run in parallel.
        let n = FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let uri = tower_lsp::lsp_types::Url::from_file_path(std::env::temp_dir().join(format!("test_semantic_tokens_{n}.cl")))
            .unwrap();

        let file_id = {
            let mut sm = source_manager.write().await;
            sm.update_file(&uri, source.to_string(), 1)
        };

        compiler_state.mark_as_root(file_id).await.unwrap();
        compiler_state.compile_changed_files(vec![file_id]).await.unwrap();

        let snapshot = compiler_state.get_snapshot().await.expect("snapshot should exist after compilation");
        (file_id, Arc::new(snapshot))
    }

    fn assert_has_token(decoded: &[(u32, u32, u32, u32)], line: u32, col: u32, len: u32, token_type: u32) {
        let ok = decoded.iter().any(|(l, c, ln, ty)| *l == line && *c == col && *ln == len && *ty == token_type);
        assert!(
            ok,
            "Expected token (line={}, col={}, len={}, type={}) not found. Got: {:?}",
            line,
            col,
            len,
            token_type,
            decoded
        );
    }

    #[tokio::test]
    async fn semantic_tokens_use_imports_and_modules() {
        let source = "use print from std;\nuse map from functional;\nprint(\"hi\")!;\n";
        let (file_id, snapshot) = compile_source(source).await;

        let tokens = super::generate_semantic_tokens(file_id, &snapshot, source);
        let decoded = decode_semantic_tokens(&tokens);

        // `use` keyword (line 0)
        let use_pos = source.find("use ").unwrap();
        let (use_line, use_col) = byte_to_line_col(source, use_pos);
        assert_has_token(&decoded, use_line, use_col, 3, TOKEN_KEYWORD);

        // `from` keyword (line 0)
        let from_pos = source.find(" from ").unwrap() + 1;
        let (from_line, from_col) = byte_to_line_col(source, from_pos);
        assert_has_token(&decoded, from_line, from_col, 4, TOKEN_KEYWORD);

        // imported `print` in first use statement should be function
        let print_pos = source.find("print from std").unwrap();
        let (print_line, print_col) = byte_to_line_col(source, print_pos);
        assert_has_token(&decoded, print_line, print_col, 5, TOKEN_FUNCTION);

        // module `std` should be type
        let std_pos = source.find("std;").unwrap();
        let (std_line, std_col) = byte_to_line_col(source, std_pos);
        assert_has_token(&decoded, std_line, std_col, 3, TOKEN_TYPE);

        // imported `map` should be function
        let map_pos = source.find("map from functional").unwrap();
        let (map_line, map_col) = byte_to_line_col(source, map_pos);
        assert_has_token(&decoded, map_line, map_col, 3, TOKEN_FUNCTION);

        // module `functional` should be type
        let func_pos = source.find("functional;").unwrap();
        let (func_line, func_col) = byte_to_line_col(source, func_pos);
        assert_has_token(&decoded, func_line, func_col, 10, TOKEN_TYPE);
    }

    #[tokio::test]
    async fn semantic_tokens_member_access_modules_and_members() {
        // Uses global stdlib module access syntax (module.member) without `use`.
        // Expect resolution-driven typing when available:
        // - module segment: TYPE
        // - member function: FUNCTION (resolution-driven, with VARIABLE fallback via CST)
        let source = "let x = math.floor;\n";
        let (file_id, snapshot) = compile_source(source).await;
        let tokens = super::generate_semantic_tokens(file_id, &snapshot, source);
        let decoded = decode_semantic_tokens(&tokens);

        let math_pos = source.find("math").unwrap();
        let (math_line, math_col) = byte_to_line_col(source, math_pos);
        assert_has_token(&decoded, math_line, math_col, 4, TOKEN_TYPE);

        let floor_pos = source.find("floor").unwrap();
        let (floor_line, floor_col) = byte_to_line_col(source, floor_pos);
        assert_has_token(&decoded, floor_line, floor_col, 5, TOKEN_FUNCTION);
    }

    #[tokio::test]
    async fn semantic_tokens_builtin_types_in_closure_annotations() {
        // Regression for "only the 'u' in num is highlighted" / broken annotation highlighting.
        let source = "let mapped = xs |> map(fn (x: num) -> num => add(x, 2));\n";
        let (file_id, snapshot) = compile_source(source).await;

        let tokens = super::generate_semantic_tokens(file_id, &snapshot, source);
        let decoded = decode_semantic_tokens(&tokens);

        // First `num`
        let num1_pos = source.find(": num").unwrap() + 2; // points at 'n'
        let (num1_line, num1_col) = byte_to_line_col(source, num1_pos);
        assert_has_token(&decoded, num1_line, num1_col, 3, TOKEN_TYPE);

        // Return `num`
        let num2_pos = source.find("-> num").unwrap() + 3; // points at 'n'
        let (num2_line, num2_col) = byte_to_line_col(source, num2_pos);
        assert_has_token(&decoded, num2_line, num2_col, 3, TOKEN_TYPE);
    }

    #[tokio::test]
    async fn mandelbrot_min_and_gradient_are_tokenized_even_with_diagnostics() {
        let source = std::fs::read_to_string("tests/fixtures/mandelbrot/main.cl").expect("read mandelbrot fixture");
        let (file_id, snapshot) = compile_source(&source).await;

        let tokens = super::generate_semantic_tokens(file_id, &snapshot, &source);
        let decoded = decode_semantic_tokens(&tokens);

        // `min` inside `(max - min)` should at least be tokenized (prefer PARAMETER, but VARIABLE is acceptable fallback).
        let needle = "(max - min)";
        let off = source.find(needle).expect("find (max - min)");
        let min_off = off + needle.find("min").unwrap();
        let (min_line, min_col) = byte_to_line_col(&source, min_off);
        let has_min = decoded.iter().any(|(l, c, ln, ty)| *l == min_line && *c == min_col && *ln == 3 && (*ty == TOKEN_PARAMETER || *ty == TOKEN_VARIABLE));
        assert!(has_min, "expected token for inner `min` in (max - min), got: {:?}", decoded);

        // `gradient[` usage is after line 69 and must be tokenized as VARIABLE at least.
        let grad_off = source.find("gradient[").expect("find gradient[");
        let (gline, gcol) = byte_to_line_col(&source, grad_off);
        let has_grad = decoded.iter().any(|(l, c, ln, _ty)| *l == gline && *c == gcol && *ln == 8);
        assert!(has_grad, "expected token for `gradient` usage after line 69, got: {:?}", decoded);
    }

    #[tokio::test]
    async fn mandelbrot_state_type_is_tokenized_after_declaration() {
        let source = std::fs::read_to_string("tests/fixtures/mandelbrot/main.cl").expect("read mandelbrot fixture");
        let (file_id, snapshot) = compile_source(&source).await;

        // Sanity: the compiled CST should contain the fold initializer StructInit.
        {
            use crate::core::cst::{CstCallArgument, CstClosureBody, CstExpr, CstStatement};

            let cst = snapshot.cst(file_id).expect("snapshot CST");
            let off = source.find("State { zx: 0").expect("find fold initializer State {");
            let fold_off = source.find("fold(").expect("find fold(");

            fn expr_contains_off(expr: &crate::core::cst::Spanned<CstExpr>, off: usize) -> bool {
                expr.span.start as usize <= off && off < expr.span.end as usize
            }

            fn walk_expr(src: &str, expr: &crate::core::cst::Spanned<CstExpr>, off: usize) -> bool {
                if !expr_contains_off(expr, off) {
                    return false;
                }
                match &expr.node {
                    CstExpr::StructInit { struct_name, .. } => {
                        let name = &src[struct_name.span.start as usize..struct_name.span.end as usize];
                        return name == "State";
                    }
                    CstExpr::FunctionCall { callee, arguments, .. } => {
                        if walk_expr(src, callee, off) {
                            return true;
                        }
                        for a in arguments {
                            if let CstCallArgument::Expr(e) = &a.node {
                                if walk_expr(src, e, off) {
                                    return true;
                                }
                            }
                        }
                    }
                    CstExpr::Infix { lhs, rhs, .. } | CstExpr::Compose { lhs, rhs, .. } => {
                        if walk_expr(src, lhs, off) {
                            return true;
                        }
                        if walk_expr(src, rhs, off) {
                            return true;
                        }
                    }
                    CstExpr::Prefix { rhs, .. } => return walk_expr(src, rhs, off),
                    CstExpr::Postfix { lhs, .. } => return walk_expr(src, lhs, off),
                    CstExpr::Group { inner, .. } => return walk_expr(src, inner, off),
                    CstExpr::Closure { body, .. } => {
                        if let CstClosureBody::Block(b) = body {
                            for st in &b.node.statements {
                                match &st.node {
                                    CstStatement::Let { expression, .. }
                                    | CstStatement::Const { expression, .. }
                                    | CstStatement::Assign { expression, .. } => {
                                        if walk_expr(src, expression, off) {
                                            return true;
                                        }
                                    }
                                    CstStatement::Expression(e) => {
                                        if walk_expr(src, e, off) {
                                            return true;
                                        }
                                    }
                                    CstStatement::Return { expression, .. } => {
                                        if walk_expr(src, expression, off) {
                                            return true;
                                        }
                                    }
                                    CstStatement::If { arms, else_block, .. } => {
                                        for (cond, blk) in arms {
                                            if walk_expr(src, cond, off) {
                                                return true;
                                            }
                                            for s2 in &blk.node.statements {
                                                if let CstStatement::Expression(e2) = &s2.node {
                                                    if walk_expr(src, e2, off) {
                                                        return true;
                                                    }
                                                }
                                            }
                                        }
                                        if let Some(eb) = else_block {
                                            for s2 in &eb.node.statements {
                                                if let CstStatement::Expression(e2) = &s2.node {
                                                    if walk_expr(src, e2, off) {
                                                        return true;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    _ => {}
                }
                false
            }

            // Also assert: `fold(` in this region is a call with 2 args and arg0 is StructInit.
            fn walk_find_fold(expr: &crate::core::cst::Spanned<CstExpr>, off: usize) -> Option<usize> {
                if !expr_contains_off(expr, off) {
                    return None;
                }
                match &expr.node {
                    CstExpr::FunctionCall { callee, arguments, .. } => {
                        if let CstExpr::Identifier(name) = &callee.node {
                            if name.node == "fold" {
                                return Some(arguments.len());
                            }
                        }
                        walk_find_fold(callee, off).or_else(|| {
                            for a in arguments {
                                if let CstCallArgument::Expr(e) = &a.node {
                                    if let Some(v) = walk_find_fold(e, off) {
                                        return Some(v);
                                    }
                                }
                            }
                            None
                        })
                    }
                    CstExpr::Infix { lhs, rhs, .. } | CstExpr::Compose { lhs, rhs, .. } => {
                        walk_find_fold(lhs, off).or_else(|| walk_find_fold(rhs, off))
                    }
                    CstExpr::Prefix { rhs, .. } => walk_find_fold(rhs, off),
                    CstExpr::Postfix { lhs, .. } => walk_find_fold(lhs, off),
                    CstExpr::Group { inner, .. } => walk_find_fold(inner, off),
                    CstExpr::Closure { body, .. } => {
                        if let CstClosureBody::Block(b) = body {
                            for st in &b.node.statements {
                                match &st.node {
                                    CstStatement::Let { expression, .. }
                                    | CstStatement::Const { expression, .. }
                                    | CstStatement::Assign { expression, .. } => {
                                        if let Some(v) = walk_find_fold(expression, off) {
                                            return Some(v);
                                        }
                                    }
                                    CstStatement::Expression(e) => {
                                        if let Some(v) = walk_find_fold(e, off) {
                                            return Some(v);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        None
                    }
                    _ => None,
                }
            }

            let mut found = false;
            for block in &cst.blocks {
                for stmt in &block.node.statements {
                    match &stmt.node {
                        CstStatement::Let { expression, .. } => {
                            if walk_expr(&source, expression, off) {
                                found = true;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
            }
            assert!(
                found,
                "expected compiled CST to contain StructInit `State {{ ... }}` fold initializer"
            );

            // `fold(` should be a call in this region and have at least 2 args.
            let mut fold_arity: Option<usize> = None;
            for block in &cst.blocks {
                for stmt in &block.node.statements {
                    if let CstStatement::Let { expression, .. } = &stmt.node {
                        if let Some(n) = walk_find_fold(expression, fold_off) {
                            fold_arity = Some(n);
                            break;
                        }
                    }
                }
            }
            assert_eq!(fold_arity, Some(2), "expected fold(...) call with 2 args");
        }

        // Narrow the failure surface: ensure the CST-identifier extractor itself emits a TYPE token
        // for the fold initializer `State { ... }`.
        {
            use crate::core::hir_lowering::Span as HirSpan;
            let cst = snapshot.cst(file_id).expect("snapshot CST");
            let mut spans: Vec<(HirSpan, u32, u32)> = Vec::new();
            super::extract_identifiers_from_cst(cst, &mut spans);
            let structinit_off = source.find("State { zx: 0").expect("find struct init State {");
            let want_span = (
                structinit_off,
                structinit_off + "State".len(),
            );
            let has_state_type = spans.iter().any(|(sp, ty, _)| {
                *ty == TOKEN_TYPE && sp.start == want_span.0 && sp.end == want_span.1
            });
            assert!(
                has_state_type,
                "expected extract_identifiers_from_cst to emit TOKEN_TYPE for fold initializer State"
            );
        }

        let tokens = super::generate_semantic_tokens(file_id, &snapshot, &source);
        let decoded = decode_semantic_tokens(&tokens);

        // In the fold initializer: `State { zx: 0, ... }`
        let structinit_off = source.find("State { zx: 0").expect("find struct init State {");
        let (sline, scol) = byte_to_line_col(&source, structinit_off);
        assert_has_token(&decoded, sline, scol, 5, TOKEN_TYPE);

        // In the closure return type: `fn (state) -> State => {`
        let ret_off = source.find("-> State").expect("find -> State") + 3; // point at 'S'
        let (rline, rcol) = byte_to_line_col(&source, ret_off);
        assert_has_token(&decoded, rline, rcol, 5, TOKEN_TYPE);
    }

    #[tokio::test]
    async fn mandelbrot_state_iter_access_does_not_color_state_as_type() {
        let source = std::fs::read_to_string("tests/fixtures/mandelbrot/main.cl").expect("read mandelbrot fixture");
        let (file_id, snapshot) = compile_source(&source).await;

        let tokens = super::generate_semantic_tokens(file_id, &snapshot, &source);
        let decoded = decode_semantic_tokens(&tokens);

        // Regression: in `iter:  state.iter + 1`, `state` must NOT be colored as TOKEN_TYPE.
        let needle = "iter:  state.iter + 1";
        let off = source.find(needle).expect("find iter:  state.iter + 1");
        let state_off = off + needle.find("state").unwrap();
        let (line, col) = byte_to_line_col(&source, state_off);

        // We accept VARIABLE/PARAMETER (semantic may classify as parameter), but not TYPE.
        let ty_at_state = decoded
            .iter()
            .find(|(l, c, ln, _ty)| *l == line && *c == col && *ln == 5)
            .map(|t| t.3);
        assert_ne!(
            ty_at_state,
            Some(TOKEN_TYPE),
            "expected `state` in `state.iter` to not be TOKEN_TYPE; decoded={:?}",
            decoded
        );
    }
}
