use pest::iterators::Pair;
use pest::Parser as PestParser;
use std::fs::OpenOptions;
use std::io::Write;

use crate::core::parser::{Rule, CantaLoopParser};
use crate::core::cst::pratt::parse_expression;
use crate::core::cst::{CstProgram, CstBlock, CstStatement, CstExpr, CstLiteral, CstUnaryOp, CstBinaryOp, CstPostfixOp, CstComposeOp, CstCallArgument, CstImportSelector, CstIndexSpec, CstClosureBody, CstArgument, ReturnTypeArrow, Span, Spanned, DocBlock, CstId, CstIdGenerator};

const DEBUG_LOG_PATH: &str = ".cursor/debug.log";

/// Safely create a pest::Span, returning an error if the span is invalid.
/// This prevents panics in the LSP server when dealing with malformed input.
fn safe_pest_span<'i>(
    input: &'i str,
    start: usize,
    end: usize,
    context_span: pest::Span<'i>,
) -> Result<pest::Span<'i>, pest::error::Error<Rule>> {
    pest::Span::new(input, start, end).ok_or_else(|| {
        pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError {
                message: format!(
                    "Invalid span: start={}, end={}, input_len={}",
                    start,
                    end,
                    input.len()
                ),
            },
            context_span,
        )
    })
}

// Helper function to write debug logs
fn debug_log(hypothesis_id: &str, location: &str, message: &str, data: serde_json::Value) {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(std::time::Duration::ZERO)
        .as_millis();
    let log_entry = serde_json::json!({
        "sessionId": "debug-session",
        "runId": "run1",
        "hypothesisId": hypothesis_id,
        "location": location,
        "message": message,
        "data": data,
        "timestamp": timestamp
    });
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(DEBUG_LOG_PATH) {
        let _ = writeln!(file, "{}", log_entry);
    }
}

/// Build a CST program from a Pest parse tree.
/// Returns the CST and a map of documentation blocks keyed by declaration identifier span.
pub fn build_cst_program(pair: Pair<Rule>) -> Result<(CstProgram, std::collections::HashMap<Span, DocBlock>), pest::error::Error<Rule>> {
    let mut id_gen = CstIdGenerator::new();
    let mut blocks: Vec<Spanned<CstBlock>> = Vec::new();
    let mut all_docs: std::collections::HashMap<Span, DocBlock> = std::collections::HashMap::new();

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::block => {
                let (block, block_docs) = build_cst_block(inner, &mut id_gen)?;
                all_docs.extend(block_docs);
                blocks.push(block);
            }
            Rule::EOI => {}
            _ => unreachable!("unexpected rule: {:?}", inner.as_rule()),
        }
    }

    Ok((CstProgram { blocks }, all_docs))
}

/// Build a CST block from a Pest parse tree.
/// Returns the block and a map of documentation blocks keyed by declaration identifier span.
/// 
/// CRITICAL: This function MUST create a brand new CstBlock every time it's called.
/// Never reuse, clone, or share blocks between different functions/closures.
fn build_cst_block(pair: Pair<Rule>, id_gen: &mut CstIdGenerator) -> Result<(Spanned<CstBlock>, std::collections::HashMap<Span, DocBlock>), pest::error::Error<Rule>> {
    let id = id_gen.next();
    let span = Span::from_pest_span(pair.as_span());
    // CRITICAL: Create a fresh Vec - never reuse or clone from elsewhere
    let mut statements: Vec<Spanned<CstStatement>> = Vec::new();
    let mut pending_docs: Vec<DocBlock> = Vec::new();
    let mut block_docs: std::collections::HashMap<Span, DocBlock> = std::collections::HashMap::new();

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::doc_comment => {
                // Collect doc comment
                let doc_span = Span::from_pest_span(inner.as_span());
                let doc_text = inner.as_str();
                // Extract text after "///", removing leading whitespace
                let doc_content = if doc_text.len() >= 3 {
                    doc_text[3..].trim_start().to_string()
                } else {
                    String::new()
                };
                if !doc_content.is_empty() {
                    pending_docs.push(DocBlock::new(doc_span, doc_content));
                }
            },
            Rule::statement_with_semicolon | Rule::statement_without_semicolon | Rule::statement => {
                let stmt = build_cst_statement(inner, id_gen)?;
                // Attach docs to declaration statements
                let _identifier_span = attach_docs_to_statement(&stmt, &mut pending_docs, &mut block_docs)?;
                statements.push(stmt);
            },
            _ => {
                // Ignore semicolons and whitespace
                // If we encounter non-statement tokens (other than whitespace/comments), discard pending docs
                if !matches!(inner.as_rule(), Rule::WHITESPACE | Rule::EOI | Rule::line_comment | Rule::block_comment) {
                    pending_docs.clear();
                }
            }
        }
    }

    // CRITICAL: Create a brand new CstBlock struct - never reuse or clone
    // Create the CstBlock inline to ensure it's truly a new allocation
    // CRITICAL: Create the CstBlock first, then wrap it in Spanned
    // This ensures both are fresh allocations
    let cst_block = CstBlock { statements };
    let spanned_block = Spanned::new(id, span, cst_block);
    
    // Log the Spanned wrapper address, not just the inner node
    let spanned_wrapper_ptr = std::ptr::addr_of!(spanned_block);
    let block_node_ptr = std::ptr::addr_of!(spanned_block.node);
    let stmt_count = spanned_block.node.statements.len();
    eprintln!(
        "[CST Builder] build_cst_block created: id={:?}, spanned_wrapper_ptr={:p}, block_node_ptr={:p}, stmt_count={}",
        id,
        spanned_wrapper_ptr,
        block_node_ptr,
        stmt_count
    );
    
    // CRITICAL: Verify this is a unique block by checking statements vec capacity
    // If we see the same capacity across different calls, we're reusing memory
    eprintln!(
        "[CST Builder] Block details: statements_vec_ptr={:p}, capacity={}",
        spanned_block.node.statements.as_ptr(),
        spanned_block.node.statements.capacity()
    );
    
    Ok((spanned_block, block_docs))
}

/// Attach pending documentation to a statement if it's a declaration.
/// Returns the identifier span if docs were attached, None otherwise.
fn attach_docs_to_statement(
    stmt: &Spanned<CstStatement>,
    pending_docs: &mut Vec<DocBlock>,
    docs_map: &mut std::collections::HashMap<Span, DocBlock>
) -> Result<Option<Span>, pest::error::Error<Rule>> {
    // Check if this is a declaration (fn, type/mod, const, let, struct)
    let (is_declaration, identifier_span) = match &stmt.node {
        CstStatement::FunctionDeclaration { identifier, .. } => (true, Some(identifier.span)),
        CstStatement::Const { identifier, .. } => (true, Some(identifier.span)),
        CstStatement::Let { identifier, .. } => (true, Some(identifier.span)),
        CstStatement::Struct { name, .. } => (true, Some(name.span)),
        CstStatement::Mod { identifier, .. } => (true, Some(identifier.span)),
        _ => (false, None),
    };
    if is_declaration && !pending_docs.is_empty() {
        // Merge consecutive doc blocks into a single DocBlock
        // Safe: we checked !pending_docs.is_empty(), so first() and last() will return Some
        let first_span = pending_docs.first()
            .map(|doc| doc.span)
            .unwrap_or_else(|| Span::new(0, 0)); // Fallback synthetic span
        let last_span = pending_docs.last()
            .map(|doc| doc.span)
            .unwrap_or(first_span); // Fallback to first if somehow empty
        let merged_span = Span::new(first_span.start, last_span.end);
        let merged_text = pending_docs.iter()
            .map(|doc| doc.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let merged_doc = DocBlock::new(merged_span, merged_text);
        // Store docs keyed by identifier span
        if let Some(id_span) = identifier_span {
            docs_map.insert(id_span, merged_doc);
        }
        pending_docs.clear();
        Ok(identifier_span)
    } else {
        // Not a declaration or no docs - discard pending docs
        pending_docs.clear();
        Ok(None)
    }
}

/// Build a CST statement from a Pest parse tree.
fn build_cst_statement(pair: Pair<Rule>, id_gen: &mut CstIdGenerator) -> Result<Spanned<CstStatement>, pest::error::Error<Rule>> {
    // Extract the actual statement from wrapper rules
    let statement_inner = extract_statement_inner(pair);
    let span = Span::from_pest_span(statement_inner.as_span());

    let stmt = match statement_inner.as_rule() {
        Rule::mod_statement => {
            build_cst_mod_statement(statement_inner, id_gen)?
        }
        Rule::let_statement => {
            build_cst_let_statement(statement_inner, id_gen)?
        }
        Rule::const_statement => {
            build_cst_const_statement(statement_inner, id_gen)?
        }
        Rule::assign_statement => {
            build_cst_assign_statement(statement_inner, id_gen)?
        }
        Rule::assign_increment_statement => {
            build_cst_assign_increment_statement(statement_inner, id_gen)?
        }
        Rule::assign_decrement_statement => {
            build_cst_assign_decrement_statement(statement_inner, id_gen)?
        }
        Rule::if_statement => {
            build_cst_if_statement(statement_inner, id_gen)?
        }
        Rule::pattern_match_statement => {
            build_cst_match_statement(statement_inner, id_gen)?
        }
        Rule::function_statement => {
            build_cst_function_declaration(statement_inner, id_gen)?
        }
        Rule::return_statement => {
            build_cst_return_statement(statement_inner, id_gen)?
        }
        Rule::loop_statement => {
            build_cst_loop_statement(statement_inner, id_gen)?
        }
        Rule::while_statement => {
            build_cst_while_statement(statement_inner, id_gen)?
        }
        Rule::for_statement => {
            build_cst_for_statement(statement_inner, id_gen)?
        }
        Rule::break_statement => {
            build_cst_break_statement(statement_inner, id_gen)?
        }
        Rule::continue_statement => {
            let span = statement_inner.as_span();
            let text = statement_inner.as_str();
            let start_offset = span.start();
            let continue_start = text.find("continue").unwrap_or(0) + start_offset;
            let continue_keyword = Span::from_usize(continue_start, continue_start + 8);
            CstStatement::Continue {
                continue_keyword,
            }
        }
        Rule::expression_statement => {
            let expr = build_cst_expression_from_pair(statement_inner, id_gen)?;
            CstStatement::Expression(expr)
        }
        Rule::use_statement => {
            build_cst_use_statement(statement_inner, id_gen)?
        }
        Rule::struct_statement => {
            build_cst_struct_statement(statement_inner, id_gen)?
        }
        _ => unreachable!("unexpected rule in build_cst_statement: {:?}", statement_inner.as_rule()),
    };

    let id = id_gen.next();
    let spanned_stmt = Spanned::new(id, span, stmt);
    
    // Log function declarations to track body blocks
    if let CstStatement::FunctionDeclaration { body, .. } = &spanned_stmt.node {
        eprintln!(
            "[CST Builder] Storing FunctionDeclaration in Spanned: stmt_id={:?}, body_id={:?}, body_spanned_ptr={:p}, body_node_ptr={:p}, body_statements_vec_ptr={:p}",
            id,
            body.id,
            body,  // Address of the Spanned<CstBlock> inside the enum
            &body.node,  // Address of the CstBlock inside the Spanned
            body.node.statements.as_ptr()  // Address of the Vec
        );
    }
    
    Ok(spanned_stmt)
}

// Helper function to recursively extract the actual statement from wrapper rules
fn extract_statement_inner(mut pair: Pair<Rule>) -> Pair<Rule> {
    loop {
        let rule = pair.as_rule();
        if matches!(rule, Rule::statement_with_semicolon | Rule::statement_without_semicolon | Rule::statement | Rule::non_assign_statement) {
            // Clone before consuming to allow recovery on missing inner element
            let pair_clone = pair.clone();
            let mut inner_iter = pair.into_inner();
            // Recover from missing inner element by cloning the original pair
            // This prevents panics on malformed input
            pair = match inner_iter.next() {
                Some(inner_pair) => inner_pair,
                None => return pair_clone, // Return cloned pair as fallback
            };
        } else {
            return pair;
        }
    }
}

/// Find the span of a keyword in text.
fn find_keyword_span(text: &str, keyword: &str, pair_span: pest::Span) -> Result<Option<Span>, pest::error::Error<Rule>> {
    let start_offset = pair_span.start();
    if let Some(pos) = text.find(keyword) {
        let start = start_offset + pos;
        let end = start + keyword.len();
        Ok(Some(Span::from_usize(start, end)))
    } else {
        Ok(None)
    }
}

/// Find the span of an operator in text.
fn find_operator_span(text: &str, op: &str, pair_span: pest::Span) -> Result<Option<Span>, pest::error::Error<Rule>> {
    let start_offset = pair_span.start();
    if let Some(pos) = text.find(op) {
        let start = start_offset + pos;
        let end = start + op.len();
        Ok(Some(Span::from_usize(start, end)))
    } else {
        Ok(None)
    }
}

/// Find a keyword as a word boundary (not part of a larger word).
fn find_word_boundary(text: &str, keyword: &str) -> Option<usize> {
    for i in 0..text.len() {
        if text[i..].starts_with(keyword) {
            // Check if it's a word boundary
            let after_keyword = if i + keyword.len() < text.len() {
                text.chars().nth(i + keyword.len())
            } else {
                None
            };
            if after_keyword.map_or(true, |c| !c.is_alphanumeric()) {
                return Some(i);
            }
        }
    }
    None
}

/// Find opening brace position in text.
fn find_opening_brace(text: &str, span: pest::Span, context: &str) -> Result<usize, pest::error::Error<Rule>> {
    text.find('{').ok_or_else(|| {
        pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: format!("{} missing opening brace", context) },
            span
        )
    })
}

/// Find matching closing brace, handling nested braces.
/// Extract braced content from text.
fn extract_cst_braced_content<'a>(text: &'a str, brace_start: usize, span: pest::Span) -> Result<&'a str, pest::error::Error<Rule>> {
    let brace_end = find_matching_brace(text, brace_start)
        .ok_or_else(|| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "Missing matching closing brace".to_string() },
            span
        ))?;
    Ok(&text[brace_start + 1..brace_end - 1])
}

fn find_matching_brace(text: &str, brace_start: usize) -> Option<usize> {
    let mut brace_count = 0;
    let chars: Vec<char> = text[brace_start..].chars().collect();
    for (i, &ch) in chars.iter().enumerate() {
        if ch == '{' {
            brace_count += 1;
        } else if ch == '}' {
            brace_count -= 1;
            if brace_count == 0 {
                return Some(brace_start + text[brace_start..][..i].len() + 1);
            }
        }
    }
    None
}

// Statement builders - these will be implemented similar to AST builder but with spans
fn build_cst_mod_statement(pair: Pair<Rule>, id_gen: &mut CstIdGenerator) -> Result<CstStatement, pest::error::Error<Rule>> {
    let span = pair.as_span();
    let mut identifier = None;
    for inner in pair.into_inner() {
        if inner.as_rule() == Rule::identifier {
            let id_span = Span::from_pest_span(inner.as_span());
            let ident_id = id_gen.next();
            identifier = Some(Spanned::new(ident_id, id_span, inner.as_str().to_string()));
            break;
        }
    }
    let identifier = identifier.ok_or_else(|| {
        pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "Mod statement missing identifier".to_string() },
            span,
        )
    })?;
    Ok(CstStatement::Mod { identifier })
}

fn build_cst_let_statement(pair: Pair<Rule>, id_gen: &mut CstIdGenerator) -> Result<CstStatement, pest::error::Error<Rule>> {
    let span = pair.as_span();
    let text = pair.as_str();
    let start_offset = span.start();
    // Check for pub visibility
    let pub_keyword = if text.trim_start().starts_with("pub") {
        Some(Span::from_usize(start_offset, start_offset + 3))
    } else {
        None
    };
    // Find "let" keyword
    let let_start = if pub_keyword.is_some() {
        text.find("let").unwrap_or(0) + start_offset
    } else {
        text.find("let").unwrap_or(0) + start_offset
    };
    let let_keyword = Span::from_usize(let_start, let_start + 3);
    // Extract identifier after "let "
    let let_keyword_end = if pub_keyword.is_some() { "pub let " } else { "let " };
    let id_start = text.find(let_keyword_end).unwrap_or(0) + let_keyword_end.len();
    let id_end = text[id_start..].find(|c: char| c.is_whitespace() || c == ':' || c == '=')
        .map(|pos| id_start + pos)
        .unwrap_or(text.len());
    let identifier = Spanned::new(id_gen.next(), 
        Span::from_usize(start_offset + id_start, start_offset + id_end),
        text[id_start..id_end].trim().to_string()
    );
    // Find '=' position
    let eq_pos = text.find('=').unwrap_or(0) + start_offset;
    let eq = Span::from_usize(eq_pos, eq_pos + 1);
    // Extract type annotation if present
    let type_annotation = if let Some(colon_pos) = text.find(':') {
        if colon_pos < eq_pos - start_offset {
            let type_start = colon_pos + 1;
            let type_end = eq_pos - start_offset;
            let type_text = text[type_start..type_end].trim();
            Some(Spanned::new(
                id_gen.next(),
                Span::from_usize(start_offset + type_start, start_offset + type_end),
                type_text.to_string()
            ))
        } else {
            None
        }
    } else {
        None
    };
    // Parse expression after "="
    let expr_start = eq_pos + 1 - start_offset;
    let expr_text = text[expr_start..].trim().strip_suffix(';').unwrap_or(&text[expr_start..]).trim();
    // For now, use the original span - expression parser will parse expr_text as substring
    // Spans will be approximate but functional
    let expression = build_cst_expression_from_text(expr_text, span, id_gen)?;
    Ok(CstStatement::Let {
        pub_keyword,
        let_keyword,
        identifier,
        type_annotation,
        eq,
        expression,
    })
}

fn build_cst_const_statement(pair: Pair<Rule>, id_gen: &mut CstIdGenerator) -> Result<CstStatement, pest::error::Error<Rule>> {
    let span = pair.as_span();
    let text = pair.as_str();
    let start_offset = span.start();
    // Check for pub visibility
    let pub_keyword = if text.trim_start().starts_with("pub") {
        Some(Span::from_usize(start_offset, start_offset + 3))
    } else {
        None
    };
    // Find "const" keyword
    let const_keyword_text = if pub_keyword.is_some() { "pub const " } else { "const " };
    let const_start = text.find("const").unwrap_or(0) + start_offset;
    let const_keyword = Span::from_usize(const_start, const_start + 5);
    // Extract identifier
    let id_start = const_keyword_text.len();
    let id_end = text[id_start..].find(|c: char| c.is_whitespace() || c == '=')
        .map(|pos| id_start + pos)
        .unwrap_or(text.len());
    let identifier = Spanned::new(id_gen.next(), 
        Span::from_usize(start_offset + id_start, start_offset + id_end),
        text[id_start..id_end].trim().to_string()
    );
    // Find '=' position
    let eq_pos = text.find('=').unwrap_or(0) + start_offset;
    let eq = Span::from_usize(eq_pos, eq_pos + 1);
    // Parse expression after "="
    let expr_start = eq_pos + 1 - start_offset;
    let expr_text = text[expr_start..].trim().strip_suffix(';').unwrap_or(&text[expr_start..]).trim();
    let expression = build_cst_expression_from_text(expr_text, span, id_gen)?;
    Ok(CstStatement::Const {
        pub_keyword,
        const_keyword,
        identifier,
        eq,
        expression,
    })
}

fn build_cst_assign_statement(pair: Pair<Rule>, id_gen: &mut CstIdGenerator) -> Result<CstStatement, pest::error::Error<Rule>> {
    let span = pair.as_span();
    let text = pair.as_str();
    let start_offset = span.start();
    // Find '=' position
    let eq_pos = text.find('=').unwrap_or(0);
    let eq = Span::from_usize(start_offset + eq_pos, start_offset + eq_pos + 1);
    // Extract identifier
    let id_end = eq_pos;
    let identifier = Spanned::new(id_gen.next(), 
        Span::from_usize(start_offset, start_offset + id_end),
        text[..id_end].trim().to_string()
    );
    // Parse expression after "="
    let expr_start = eq_pos + 1;
    let expr_text = text[expr_start..].trim();
    // Use original span - expression parser will handle the offset
    let expression = build_cst_expression_from_text(expr_text, span, id_gen)?;
    Ok(CstStatement::Assign {
        identifier,
        eq,
        expression,
    })
}

fn build_cst_assign_increment_statement(pair: Pair<Rule>, id_gen: &mut CstIdGenerator) -> Result<CstStatement, pest::error::Error<Rule>> {
    let span = pair.as_span();
    let text = pair.as_str();
    let start_offset = span.start();
    // Find "+=" operator
    let op_pos = text.find("+=").unwrap_or(0);
    let op = Span::from_usize(start_offset + op_pos, start_offset + op_pos + 2);
    // Extract identifier
    let identifier = Spanned::new(id_gen.next(), 
        Span::from_usize(start_offset, start_offset + op_pos),
        text[..op_pos].trim().to_string()
    );
    // Parse expression after "+="
    let expr_start = op_pos + 2;
    let expr_text = text[expr_start..].trim();
    let expression = build_cst_expression_from_text(expr_text, span, id_gen)?;
    Ok(CstStatement::AssignIncrement {
        identifier,
        op,
        expression,
    })
}

fn build_cst_assign_decrement_statement(pair: Pair<Rule>, id_gen: &mut CstIdGenerator) -> Result<CstStatement, pest::error::Error<Rule>> {
    let span = pair.as_span();
    let text = pair.as_str();
    let start_offset = span.start();
    // Find "-=" operator
    let op_pos = text.find("-=").unwrap_or(0);
    let op = Span::from_usize(start_offset + op_pos, start_offset + op_pos + 2);
    // Extract identifier
    let identifier = Spanned::new(id_gen.next(), 
        Span::from_usize(start_offset, start_offset + op_pos),
        text[..op_pos].trim().to_string()
    );
    // Parse expression after "-="
    let expr_start = op_pos + 2;
    let expr_text = text[expr_start..].trim();
    let expression = build_cst_expression_from_text(expr_text, span, id_gen)?;
    Ok(CstStatement::AssignDecrement {
        identifier,
        op,
        expression,
    })
}

fn build_cst_if_statement(pair: Pair<Rule>, id_gen: &mut CstIdGenerator) -> Result<CstStatement, pest::error::Error<Rule>> {
    let span = pair.as_span();
    let text = pair.as_str();
    let start_offset = span.start();
    // Find first "if" keyword
    let if_start = text.find("if ").unwrap_or(0) + start_offset;
    let if_keyword = Span::from_usize(if_start, if_start + 2);
    // Collect all braced_blocks from the parse tree (need to clone spans)
    let mut blocks = Vec::new();
    let inner = pair.clone().into_inner();
    for item in inner {
        if item.as_rule() == Rule::braced_block {
            blocks.push(item);
        }
    }
    // Extract expressions and blocks manually from text
    // Format: "if" expr block [ "elseif" expr block ]* [ "else" block ]?
    let mut arms = Vec::new();
    let mut else_keywords = Vec::new();
    let mut else_keyword = None;
    let mut else_block = None;
    // Get block start positions relative to the if_statement span
    let block_starts: Vec<usize> = blocks.iter()
        .map(|b| b.as_span().start() - span.start())
        .collect();
    // Find the first "if " keyword
    let first_if_pos = text.find("if ").ok_or_else(|| pest::error::Error::new_from_span(
        pest::error::ErrorVariant::CustomError { message: "If statement missing 'if' keyword".to_string() },
        span
    ))?;
    let mut prev_block_end = first_if_pos + 3; // After "if "
    let mut else_block_index = None;
    // Process each block
    for (i, &block_start) in block_starts.iter().enumerate() {
        // Text between previous block end and this block start
        let between = &text[prev_block_end..block_start];
        let trimmed = between.trim_start();
        // Check for "else " (must be followed by whitespace or "{")
        if trimmed.starts_with("else") {
            let after_else = &trimmed[4..];
            if after_else.is_empty() || after_else.starts_with(' ') || after_else.starts_with('{') || after_else.starts_with('\t') || after_else.starts_with('\n') {
                // This is the else block
                let else_start = start_offset + prev_block_end + (between.len() - trimmed.len());
                else_keyword = Some(Span::from_usize(else_start, else_start + 4));
                else_block_index = Some(i);
                break;
            }
        }
        // Check for "elseif "
        if trimmed.starts_with("elseif ") {
            let elseif_start = start_offset + prev_block_end + (between.len() - trimmed.len());
            else_keywords.push(Span::from_usize(elseif_start, elseif_start + 6));
            // Extract expression after "elseif "
            let expr_start = prev_block_end + (between.len() - trimmed.len()) + 7; // "elseif "
            let expr_text = text[expr_start..block_start].trim();
            if expr_text.is_empty() {
                return Err(pest::error::Error::new_from_span(
                    pest::error::ErrorVariant::CustomError { message: "Empty expression in elseif statement".to_string() },
                    span
                ));
            }
            let expr = build_cst_expression_from_text(expr_text, span, id_gen)?;
            // Get corresponding block
            if i < blocks.len() {
                let block_pair = blocks[i].clone().into_inner()
                    .find(|p| p.as_rule() == Rule::block)
                    .ok_or_else(|| pest::error::Error::new_from_span(
                        pest::error::ErrorVariant::CustomError { message: "If block missing inner block".to_string() },
                        span
                    ))?;
                let (block, _) = build_cst_block(block_pair, id_gen)?;
                arms.push((expr, block));
            }
        } else {
            // First block: extract expression after "if "
            let expr_text = text[prev_block_end..block_start].trim();
            if expr_text.is_empty() {
                return Err(pest::error::Error::new_from_span(
                    pest::error::ErrorVariant::CustomError { message: "Empty expression in if statement".to_string() },
                    span
                ));
            }
            let expr = build_cst_expression_from_text(expr_text, span, id_gen)?;
            // Get corresponding block
            if i < blocks.len() {
                let block_pair = blocks[i].clone().into_inner()
                    .find(|p| p.as_rule() == Rule::block)
                    .ok_or_else(|| pest::error::Error::new_from_span(
                        pest::error::ErrorVariant::CustomError { message: "If block missing inner block".to_string() },
                        span
                    ))?;
                let (block, _) = build_cst_block(block_pair, id_gen)?;
                arms.push((expr, block));
            }
        }
        // Move to end of this block for next iteration
        prev_block_end = blocks[i].as_span().end() - span.start();
    }
    // Handle else block if found
    if let Some(else_idx) = else_block_index {
        if else_idx < blocks.len() {
            let block_pair = blocks[else_idx].clone().into_inner()
                .find(|p| p.as_rule() == Rule::block)
                .ok_or_else(|| pest::error::Error::new_from_span(
                    pest::error::ErrorVariant::CustomError { message: "Else block missing inner block".to_string() },
                    span
                ))?;
            let (block, _) = build_cst_block(block_pair, id_gen)?;
            else_block = Some(block);
        }
    }
    if arms.is_empty() {
        return Err(pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "No expressions found in if statement".to_string() },
            span
        ));
    }
    Ok(CstStatement::If {
        if_keyword,
        arms,
        else_keywords,
        else_keyword,
        else_block,
    })
}

fn build_cst_match_statement(pair: Pair<Rule>, id_gen: &mut CstIdGenerator) -> Result<CstStatement, pest::error::Error<Rule>> {
    let span = pair.as_span();
    let text = pair.as_str();
    let start_offset = span.start();
    // Find "match" keyword
    let match_start = text.find("match").unwrap_or(0) + start_offset;
    let match_keyword = Span::from_usize(match_start, match_start + 5);
    // Find the opening brace
    let brace_start = text.find('{')
        .ok_or_else(|| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "Match statement missing opening brace".to_string() },
            span
        ))?;
    // Extract expression text between "match " and "{"
    let expr_start = 6; // After "match "
    let expr_text = text[expr_start..brace_start].trim();
    if expr_text.is_empty() {
        return Err(pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "Match statement missing expression".to_string() },
            span
        ));
    }
    let expression = build_cst_expression_from_text(expr_text, span, id_gen)?;
    // Extract the content between the braces (the pattern cases)
    let mut brace_count = 0;
    let mut found_start = false;
    let mut match_brace_end = None;
    for (i, ch) in text[brace_start..].char_indices() {
        if ch == '{' {
            brace_count += 1;
            found_start = true;
        } else if ch == '}' {
            brace_count -= 1;
            if found_start && brace_count == 0 {
                match_brace_end = Some(brace_start + i);
                break;
            }
        }
    }
    let match_brace_end = match_brace_end.ok_or_else(|| pest::error::Error::new_from_span(
        pest::error::ErrorVariant::CustomError { message: "Match statement missing closing brace".to_string() },
        span
    ))?;
    let cases_text = &text[brace_start + 1..match_brace_end];
    // Parse each pattern case manually
    let mut cases = Vec::new();
    let mut pos = 0;
    while pos < cases_text.len() {
        // Skip whitespace at the start
        let case_start = cases_text[pos..].find(|c: char| !c.is_whitespace())
            .map(|i| pos + i)
            .unwrap_or(cases_text.len());
        if case_start >= cases_text.len() {
            break; // No more cases
        }
        // Find the opening brace for this case's block
        let case_brace_start = cases_text[case_start..].find('{')
            .map(|i| case_start + i)
            .ok_or_else(|| pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError { message: "Pattern case missing opening brace".to_string() },
                span
            ))?;
        // Extract the pattern_value text (everything before the brace, trimmed)
        let pattern_text = cases_text[case_start..case_brace_start].trim();
        // Find the matching closing brace for this case's block
        let mut brace_count = 0;
        let mut found_start = false;
        let mut case_brace_end = None;
        for (i, ch) in cases_text[case_brace_start..].char_indices() {
            if ch == '{' {
                brace_count += 1;
                found_start = true;
            } else if ch == '}' {
                brace_count -= 1;
                if found_start && brace_count == 0 {
                    case_brace_end = Some(case_brace_start + i);
                    break;
                }
            }
        }
        let case_brace_end = case_brace_end.ok_or_else(|| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "Pattern case missing closing brace".to_string() },
            span
        ))?;
        // Extract the block text
        let block_text = &cases_text[case_brace_start..case_brace_end + 1];
        // Parse the pattern_value text to determine the pattern
        let pattern = if pattern_text.trim() == "_" {
            // Wildcard case
            None
        } else {
            // Regular expression pattern
            Some(build_cst_expression_from_text(pattern_text, span, id_gen)?)
        };
        // CRITICAL FIX: Clone the string to ensure independent parse
        let isolated_block_text = block_text.clone();
        let mut block_parse_result = CantaLoopParser::parse(Rule::braced_block, &isolated_block_text)
            .map_err(|e| pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError { 
                    message: format!("Failed to parse pattern case block: {:?}", e)
                },
                span
            ))?;
        
        let block = if let Some(block_pair) = block_parse_result.next() {
            let mut block_inner_iter = block_pair.into_inner();
            let inner_block = block_inner_iter
                .find(|p| p.as_rule() == Rule::block)
                .ok_or_else(|| pest::error::Error::new_from_span(
                    pest::error::ErrorVariant::CustomError { message: "Pattern case block missing block".to_string() },
                    span
                ))?;
            let (block, _) = build_cst_block(inner_block, id_gen)?;
            block
        } else {
            Spanned::new(id_gen.next(), Span::from_pest_span(span), CstBlock { statements: Vec::new() })
        };
        cases.push((pattern, block));
        // Move to after this case
        pos = case_brace_end + 1;
    }
    if cases.is_empty() {
        return Err(pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "Match statement must have at least one case".to_string() },
            span
        ));
    }
    Ok(CstStatement::Match {
        match_keyword,
        expression,
        cases,
    })
}

/// Parse function arguments from text with spans.
fn parse_cst_function_arguments(text: &str, parent_span: pest::Span, id_gen: &mut CstIdGenerator) -> Result<Vec<Spanned<CstArgument>>, pest::error::Error<Rule>> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    // Parse using the grammar rule
    let mut args_pairs = CantaLoopParser::parse(Rule::function_args, text)
        .map_err(|e| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { 
                message: format!("Failed to parse function arguments: {}", e)
            },
            parent_span
        ))?;
    let mut arguments = Vec::new();
    if let Some(args_pair) = args_pairs.next() {
        for arg_pair in args_pair.into_inner() {
            if arg_pair.as_rule() == Rule::argument {
                let arg_span = arg_pair.as_span();
                let arg_text = arg_pair.as_str();
                let start_offset = arg_span.start();
                // Find colon and parse identifier and type
                let colon_pos = arg_text.find(':')
                    .ok_or_else(|| pest::error::Error::new_from_span(
                        pest::error::ErrorVariant::CustomError { message: "Function argument missing colon".to_string() },
                        arg_span
                    ))?;
                let identifier_text = arg_text[..colon_pos].trim();
                let type_text = arg_text[colon_pos + 1..].trim();
                let identifier = Spanned::new(id_gen.next(), 
                    Span::from_usize(start_offset, start_offset + identifier_text.len()),
                    identifier_text.to_string()
                );
                let colon = Span::from_usize(start_offset + colon_pos, start_offset + colon_pos + 1);
                let type_annotation = Spanned::new(id_gen.next(), 
                    Span::from_usize(start_offset + colon_pos + 1, start_offset + arg_text.len()),
                    type_text.to_string()
                );
                arguments.push(Spanned::new(id_gen.next(), 
                    Span::from_pest_span(arg_span),
                    CstArgument {
                        identifier,
                        colon,
                        type_annotation,
                    }
                ));
            }
        }
    }
    Ok(arguments)
}

fn build_cst_function_declaration(pair: Pair<Rule>, id_gen: &mut CstIdGenerator) -> Result<CstStatement, pest::error::Error<Rule>> {
    let span = pair.as_span();
    let text = pair.as_str();
    let start_offset = span.start();
    // Check for pub visibility
    let pub_keyword = if text.trim_start().starts_with("pub") {
        Some(Span::from_usize(start_offset, start_offset + 3))
    } else {
        None
    };
    // Find "fn" keyword
    let fn_start = text.find("fn").unwrap_or(0) + start_offset;
    let fn_keyword = Span::from_usize(fn_start, fn_start + 2);
    // Extract identifier after "fn " or "pub fn "
    let fn_keyword_text = if pub_keyword.is_some() { "pub fn " } else { "fn " };
    let id_start = fn_keyword_text.len();
    let id_end = text[id_start..].find(|c: char| c.is_whitespace() || c == '(')
        .map(|pos| id_start + pos)
        .unwrap_or(text.len());
    let identifier = Spanned::new(id_gen.next(), 
        Span::from_usize(start_offset + id_start, start_offset + id_end),
        text[id_start..id_end].trim().to_string()
    );
    // Find closing paren
    let paren_end = text.find(')')
        .ok_or_else(|| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "Function missing closing paren".to_string() },
            span
        ))? + 1;
    // Extract arguments (between parens)
    let paren_start = text.find('(').unwrap_or(0) + 1;
    let args_text = text[paren_start..paren_end - 1].trim();
    // Parse function arguments
    let arguments = if args_text.is_empty() {
        Vec::new()
    } else {
        parse_cst_function_arguments(args_text, span, id_gen)?
    };
    // Find return type and body
    let after_paren = &text[paren_end..];
    // Check for return type arrow (-> or ~>)
    let return_type_arrow = extract_cst_return_type_arrow(after_paren, start_offset + paren_end, span, id_gen)?;
    // #region agent log
    debug_log("C", "cst/builder.rs:parse_cst_function", "Built function declaration", serde_json::json!({
        "function_name": identifier.node,
        "has_return_type_arrow": return_type_arrow.is_some(),
        "return_type_arrow_span": return_type_arrow.as_ref().map(|rta| serde_json::json!({
            "start": rta.node.arrow.start,
            "end": rta.node.arrow.end,
            "is_effectful": rta.node.is_effectful
        }))
    }));
    // #endregion
    // Find opening brace
    let brace_start = after_paren.find('{')
        .ok_or_else(|| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "Function missing opening brace".to_string() },
            span
        ))?;
    // Extract block content
    let mut brace_count = 0;
    let mut found_start = false;
    let mut body_brace_end = None;
    for (i, ch) in after_paren[brace_start..].char_indices() {
        if ch == '{' {
            brace_count += 1;
            found_start = true;
        } else if ch == '}' {
            brace_count -= 1;
            if found_start && brace_count == 0 {
                body_brace_end = Some(brace_start + i);
                break;
            }
        }
    }
    let body_brace_end = body_brace_end.ok_or_else(|| pest::error::Error::new_from_span(
        pest::error::ErrorVariant::CustomError { message: "Function missing closing brace".to_string() },
        span
    ))?;
    let body_content = &after_paren[brace_start + 1..body_brace_end];
    
    // CRITICAL FIX: Create a fresh String for each function body
    // This ensures each parse gets its own memory allocation
    let body_text = format!("{{{}}}", body_content);
    
    eprintln!(
        "[CST Builder] Parsing function '{}' body: text_len={}, text_preview={:?}",
        identifier.node,
        body_text.len(),
        if body_text.len() > 50 { &body_text[..50] } else { &body_text }
    );
    
    // CRITICAL FIX: Parse into a NEW string buffer for isolation
    // Create a completely independent parse for this function body
    let body = {
        // Force a new allocation by cloning the string
        let isolated_body_text = body_text.clone();
        
        let mut body_parse_result = CantaLoopParser::parse(Rule::braced_block, &isolated_body_text)
            .map_err(|e| pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError { 
                    message: format!("Failed to parse function body: {:?}", e)
                },
                span
            ))?;
        
        if let Some(block_pair) = body_parse_result.next() {
            // Extract inner block WITHOUT cloning the pair
            // This ensures we get a fresh allocation from the NEW parse
            let mut block_inner_iter = block_pair.into_inner();
            let inner_block = block_inner_iter
                .find(|p| p.as_rule() == Rule::block)
                .ok_or_else(|| pest::error::Error::new_from_span(
                    pest::error::ErrorVariant::CustomError { message: "Function body missing block".to_string() },
                    span
                ))?;
            
            // build_cst_block creates a NEW CstBlock with its own Vec<Statement>
            let (block, _) = build_cst_block(inner_block, id_gen)?;
            
            // CRITICAL: Log the Spanned wrapper address, not just the inner node
            let spanned_wrapper_ptr = std::ptr::addr_of!(block);
            eprintln!(
                "[CST Builder] Function '{}' body block created: id={:?}, spanned_wrapper_ptr={:p}, block_node_ptr={:p}, stmt_count={}, statements_vec_ptr={:p}",
                identifier.node,
                block.id,
                spanned_wrapper_ptr,
                &block.node,
                block.node.statements.len(),
                block.node.statements.as_ptr()
            );
            
            block
        } else {
            let empty_block = Spanned::new(
                id_gen.next(), 
                Span::from_pest_span(span), 
                CstBlock { statements: Vec::new() }
            );
            eprintln!(
                "[CST Builder] Function '{}' body block (empty): id={:?}, ptr={:p}",
                identifier.node,
                empty_block.id,
                &empty_block.node
            );
            empty_block
        }
    };
    // CRITICAL: Log the actual Spanned wrapper address using addr_of!
    let body_spanned_wrapper_ptr = std::ptr::addr_of!(body);
    eprintln!(
        "[CST Builder] Creating FunctionDeclaration '{}': body_id={:?}, body_spanned_wrapper_ptr={:p}, body_node_ptr={:p}, body_statements_vec_ptr={:p}",
        identifier.node,
        body.id,
        body_spanned_wrapper_ptr,
        &body.node,
        body.node.statements.as_ptr()
    );
    Ok(CstStatement::FunctionDeclaration {
        pub_keyword,
        fn_keyword,
        identifier,
        arguments,
        return_type_arrow,
        body,
    })
}

fn build_cst_return_statement(pair: Pair<Rule>, id_gen: &mut CstIdGenerator) -> Result<CstStatement, pest::error::Error<Rule>> {
    let span = pair.as_span();
    let text = pair.as_str();
    let start_offset = span.start();
    // Find "return" keyword
    let return_start = text.find("return").unwrap_or(0) + start_offset;
    let return_keyword = Span::from_usize(return_start, return_start + 6);
    // Extract expression after "return "
    let expr_start = 7; // After "return "
    let expr_text = text[expr_start..].trim();
    // Use original span - expression parser will handle the offset
    let expression = build_cst_expression_from_text(expr_text, span, id_gen)?;
    Ok(CstStatement::Return {
        return_keyword,
        expression,
    })
}

fn build_cst_loop_statement(pair: Pair<Rule>, id_gen: &mut CstIdGenerator) -> Result<CstStatement, pest::error::Error<Rule>> {
    let span = pair.as_span();
    let text = pair.as_str();
    let start_offset = span.start();
    // Find "loop" keyword
    let loop_start = text.find("loop").unwrap_or(0) + start_offset;
    let loop_keyword = Span::from_usize(loop_start, loop_start + 4);
    // Find opening brace
    let brace_start = text.find('{')
        .ok_or_else(|| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "Loop statement missing opening brace".to_string() },
            span
        ))?;
    // Parse optional init vars (everything between "loop" and "{")
    let init_text = text[4..brace_start].trim();
    let init_vars = if init_text.is_empty() {
        Vec::new()
    } else {
        // Parse loop init vars: identifier = expression, identifier = expression, ...
        let mut vars = Vec::new();
        let parts: Vec<&str> = init_text.split(',').collect();
        for part in parts {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            if let Some(eq_pos) = part.find('=') {
                let var_name = part[..eq_pos].trim();
                let expr_text = part[eq_pos + 1..].trim();
                // Calculate spans for var and expr
                let var_start = text.find(var_name).unwrap_or(0) + start_offset;
                let var_span = Spanned::new(id_gen.next(), 
                    Span::from_usize(var_start, var_start + var_name.len()),
                    var_name.to_string()
                );
                let eq_span = Span::from_usize(var_start + var_name.len() + 1, var_start + var_name.len() + 2);
                let expr = build_cst_expression_from_text(expr_text, span, id_gen)?;
                vars.push((var_span, eq_span, expr));
            }
        }
        vars
    };
    // Parse body block
    let mut inner = pair.into_inner();
    let braced_block = inner.find(|p| p.as_rule() == Rule::braced_block)
        .ok_or_else(|| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "Loop body missing block".to_string() },
            span
        ))?;
    let block_pair = braced_block.into_inner()
        .find(|p| p.as_rule() == Rule::block)
        .ok_or_else(|| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "Loop body missing inner block".to_string() },
            span
        ))?;
    let (body, _) = build_cst_block(block_pair, id_gen)?;
    Ok(CstStatement::Loop {
        loop_keyword,
        init_vars,
        body,
    })
}

fn build_cst_while_statement(pair: Pair<Rule>, id_gen: &mut CstIdGenerator) -> Result<CstStatement, pest::error::Error<Rule>> {
    let span = pair.as_span();
    let text = pair.as_str();
    let start_offset = span.start();
    // Find "while" keyword
    let while_start = text.find("while").unwrap_or(0) + start_offset;
    let while_keyword = Span::from_usize(while_start, while_start + 5);
    // Find opening brace
    let brace_start = text.find('{')
        .ok_or_else(|| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "While statement missing opening brace".to_string() },
            span
        ))?;
    // Extract condition (between "while " and "{")
    let condition_text = text[5..brace_start].trim();
    let condition = build_cst_expression_from_text(condition_text, span, id_gen)?;
    // Parse body block
    let mut inner = pair.into_inner();
    let braced_block = inner.find(|p| p.as_rule() == Rule::braced_block)
        .ok_or_else(|| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "While body missing block".to_string() },
            span
        ))?;
    let block_pair = braced_block.into_inner()
        .find(|p| p.as_rule() == Rule::block)
        .ok_or_else(|| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "While body missing inner block".to_string() },
            span
        ))?;
    let (body, _) = build_cst_block(block_pair, id_gen)?;
    Ok(CstStatement::While {
        while_keyword,
        condition,
        body,
    })
}

fn build_cst_for_statement(pair: Pair<Rule>, id_gen: &mut CstIdGenerator) -> Result<CstStatement, pest::error::Error<Rule>> {
    let span = pair.as_span();
    let text = pair.as_str();
    let start_offset = span.start();
    // Find "for" keyword
    let for_pos = text.find("for")
        .ok_or_else(|| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "For statement missing 'for'".to_string() },
            span
        ))?;
    let for_keyword = Span::from_usize(start_offset + for_pos, start_offset + for_pos + 3);
    let after_for = &text[for_pos + 3..];
    // Find "in" as word boundary
    let in_pos_in_after = find_word_boundary(after_for, "in")
        .ok_or_else(|| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "For statement missing 'in'".to_string() },
            span
        ))?;
    // Extract variable name
    let var_name_text = after_for[..in_pos_in_after].trim();
    if var_name_text.is_empty() {
        return Err(pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "For statement missing variable name".to_string() },
            span
        ));
    }
    let var_name_start_offset = start_offset + for_pos + 3 + (after_for[..in_pos_in_after].len() - var_name_text.len());
    let var_name = Spanned::new(id_gen.next(), 
        Span::from_usize(var_name_start_offset, var_name_start_offset + var_name_text.len()),
        var_name_text.to_string()
    );
    // "in" keyword position
    let in_start_in_text = start_offset + for_pos + 3 + in_pos_in_after;
    let in_keyword = Span::from_usize(in_start_in_text, in_start_in_text + 2);
    // Find ".." range operator
    let after_in = &after_for[in_pos_in_after + 2..];
    let dotdot_pos = after_in.find("..")
        .ok_or_else(|| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "For statement missing '..'".to_string() },
            span
        ))?;
    let dotdot_start_in_text = start_offset + for_pos + 3 + in_pos_in_after + 2 + dotdot_pos;
    let dotdot = Span::from_usize(dotdot_start_in_text, dotdot_start_in_text + 2);
    // Find opening brace
    // Note: brace_start_offset is relative to the substring after_in[dotdot_pos + 2..]
    let brace_start_offset_relative = find_opening_brace(&after_in[dotdot_pos + 2..], span, "For statement")?;
    // Convert to absolute offset within after_in
    let brace_start_offset_absolute = dotdot_pos + 2 + brace_start_offset_relative;
    // Extract start and end expressions with bounds checking
    let start_text = if dotdot_pos <= after_in.len() {
        after_in[..dotdot_pos].trim()
    } else {
        return Err(pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { 
                message: format!("For statement range: dotdot_pos {} exceeds string length {}", dotdot_pos, after_in.len())
            },
            span
        ));
    };
    let end_start = dotdot_pos + 2;
    if end_start > after_in.len() {
        return Err(pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { 
                message: format!("For statement range: end_start {} exceeds string length {}", end_start, after_in.len())
            },
            span
        ));
    }
    if brace_start_offset_absolute < end_start {
        return Err(pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { 
                message: format!("For statement range: invalid span ({}..{}) - brace comes before range end", end_start, brace_start_offset_absolute)
            },
            span
        ));
    }
    if brace_start_offset_absolute > after_in.len() {
        return Err(pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { 
                message: format!("For statement range: brace_start_offset {} exceeds string length {}", brace_start_offset_absolute, after_in.len())
            },
            span
        ));
    }
    let end_text = after_in[end_start..brace_start_offset_absolute].trim();
    if start_text.is_empty() {
        return Err(pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "For statement range missing start expression".to_string() },
            span
        ));
    }
    if end_text.is_empty() {
        return Err(pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "For statement range missing end expression".to_string() },
            span
        ));
    }
    let start = build_cst_expression_from_text(start_text, span, id_gen)?;
    let end = build_cst_expression_from_text(end_text, span, id_gen)?;
    // Extract body block using the parse tree (need to clone since we've used pair)
    let mut inner = pair.clone().into_inner();
    let _braced_block = inner.find(|p| p.as_rule() == Rule::braced_block)
        .ok_or_else(|| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "For body missing block".to_string() },
            span
        ))?;
    // Parse body block
    let mut inner = pair.into_inner();
    let braced_block = inner.find(|p| p.as_rule() == Rule::braced_block)
        .ok_or_else(|| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "For body missing block".to_string() },
            span
        ))?;
    let block_pair = braced_block.into_inner()
        .find(|p| p.as_rule() == Rule::block)
        .ok_or_else(|| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "For body missing inner block".to_string() },
            span
        ))?;
    let (body, _) = build_cst_block(block_pair, id_gen)?;
    Ok(CstStatement::For {
        for_keyword,
        var_name,
        in_keyword,
        start,
        dotdot,
        end,
        body,
    })
}

fn build_cst_break_statement(pair: Pair<Rule>, id_gen: &mut CstIdGenerator) -> Result<CstStatement, pest::error::Error<Rule>> {
    let span = pair.as_span();
    let text = pair.as_str();
    let start_offset = span.start();
    // Find "break" keyword
    let break_start = text.find("break").unwrap_or(0) + start_offset;
    let break_keyword = Span::from_usize(break_start, break_start + 5);
    // Check if there's an expression after "break"
    let expr_start = 6; // After "break "
    let expr_text = if expr_start < text.len() {
        text[expr_start..].trim()
    } else {
        ""
    };
    let expression = if expr_text.is_empty() {
        None
    } else {
        Some(build_cst_expression_from_text(expr_text, span, id_gen)?)
    };
    Ok(CstStatement::Break {
        break_keyword,
        expression,
    })
}

fn build_cst_use_statement(pair: Pair<Rule>, id_gen: &mut CstIdGenerator) -> Result<CstStatement, pest::error::Error<Rule>> {
    let span = pair.as_span();
    let text = pair.as_str();
    let start_offset = span.start();
    // Find "use" keyword - use trimmed start to find actual position
    let trimmed_text = text.trim_start();
    let leading_whitespace = text.len() - trimmed_text.len();
    let use_start = trimmed_text.find("use").ok_or_else(|| {
        pest::error::Error::new_from_pos(
            pest::error::ErrorVariant::CustomError {
                message: "Missing 'use' keyword in use statement".to_string(),
            },
            pest::Position::from_start(""),
        )
    })? + start_offset + leading_whitespace;
    let use_keyword = Span::from_usize(use_start, use_start + 3);
    // Find "from" keyword - search after "use"
    let after_use = &text[use_start + 3 - start_offset..];
    let from_pos = after_use.find("from").ok_or_else(|| {
        pest::error::Error::new_from_pos(
            pest::error::ErrorVariant::CustomError {
                message: "Missing 'from' keyword in use statement".to_string(),
            },
            pest::Position::from_start(""),
        )
    })?;
    let from_start = use_start + 3 + from_pos;
    let from_keyword = Span::from_usize(from_start, from_start + 4);
    // Parse import items (between "use" and "from")
    let items_text = &text[use_start + 3 - start_offset..from_start - start_offset];
    // IMPORTANT: Pass untrimmed text so parse_cst_import_selector can calculate trim offset correctly
    let selector = parse_cst_import_selector(items_text, Span::from_usize(use_start + 3, from_start), span, id_gen)?;
    // Parse import path (after "from") - split by '.' and track positions accurately
    let path_text = &text[from_start + 4 - start_offset..];
    let mut path_parts: Vec<Spanned<String>> = Vec::new();
    let mut current_pos = from_start + 4;
    let trimmed_path = path_text.trim_start();
    let path_leading_whitespace = path_text.len() - trimmed_path.len();
    current_pos += path_leading_whitespace;
    for (idx, part) in trimmed_path.split('.').enumerate() {
        let trimmed_part = part.trim();
        if trimmed_part.is_empty() {
            continue;
        }
        // Find the position of this part in the original text
        let part_in_path = if idx == 0 {
            trimmed_path
        } else {
            // Find where this part starts in the trimmed path
            let mut search_start = 0;
            for prev_part in trimmed_path.split('.').take(idx) {
                search_start += prev_part.len() + 1; // +1 for the '.'
            }
            &trimmed_path[search_start..]
        };
        let part_offset = part_in_path.find(trimmed_part).unwrap_or(0);
        let part_start = current_pos + part_offset;
        path_parts.push(Spanned::new(id_gen.next(), 
            Span::from_usize(part_start, part_start + trimmed_part.len()),
            trimmed_part.to_string()
        ));
        // Move current_pos to after this part and the '.'
        current_pos += part_in_path.len().min(part.len());
    }
    Ok(CstStatement::Use {
        use_keyword,
        selector,
        from_keyword,
        path: path_parts,
    })
}

fn build_cst_struct_statement(pair: Pair<Rule>, id_gen: &mut CstIdGenerator) -> Result<CstStatement, pest::error::Error<Rule>> {
    let span = pair.as_span();
    let text = pair.as_str();
    let start_offset = span.start();
    // Check for pub visibility
    let pub_keyword = if text.trim_start().starts_with("pub") {
        Some(Span::from_usize(start_offset, start_offset + 3))
    } else {
        None
    };
    // Find "struct" keyword
    let struct_start = text.find("struct").unwrap_or(0) + start_offset;
    let struct_keyword = Span::from_usize(struct_start, struct_start + 6);
    // Extract struct name
    let name_start = if pub_keyword.is_some() { "pub struct ".len() } else { "struct ".len() };
    let name_end = text[name_start..].find(|c: char| c.is_whitespace() || c == '{')
        .map(|pos| name_start + pos)
        .unwrap_or(text.len());
    let name = Spanned::new(id_gen.next(), 
        Span::from_usize(start_offset + name_start, start_offset + name_end),
        text[name_start..name_end].trim().to_string()
    );
    // Find opening brace
    let brace_start = text.find('{')
        .ok_or_else(|| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "Struct missing opening brace".to_string() },
            span
        ))?;
    // Extract struct fields content
    let mut brace_count = 0;
    let mut found_start = false;
    let mut brace_end = None;
    for (i, ch) in text[brace_start..].char_indices() {
        if ch == '{' {
            brace_count += 1;
            found_start = true;
        } else if ch == '}' {
            brace_count -= 1;
            if found_start && brace_count == 0 {
                brace_end = Some(brace_start + i);
                break;
            }
        }
    }
    let brace_end = brace_end.ok_or_else(|| pest::error::Error::new_from_span(
        pest::error::ErrorVariant::CustomError { message: "Struct missing closing brace".to_string() },
        span
    ))?;
    let fields_content = text[brace_start + 1..brace_end].trim();
    let fields = if fields_content.is_empty() {
        Vec::new()
    } else {
        parse_cst_struct_fields(fields_content, Span::from_usize(start_offset + brace_start + 1, start_offset + brace_end), span, id_gen)?
    };
    Ok(CstStatement::Struct {
        pub_keyword,
        struct_keyword,
        name,
        fields,
    })
}

/// Build a CST expression from a Pest pair.
/// This is a simplified version that delegates to the full expression builder.
fn build_cst_expression_from_pair(pair: Pair<Rule>, id_gen: &mut CstIdGenerator) -> Result<Spanned<CstExpr>, pest::error::Error<Rule>> {
    let full_text = pair.as_str();
    let span = pair.as_span();
    build_cst_expression_from_text(full_text, span, id_gen)
}

/// Build a CST expression from text using the Pratt parser.
fn build_cst_expression_from_text(text: &str, span: pest::Span, id_gen: &mut CstIdGenerator) -> Result<Spanned<CstExpr>, pest::error::Error<Rule>> {
    // Since expression is a silent rule (_), pest flattens it.
    // When we parse Rule::expression, pest returns the flattened inner pairs directly.
    // We get: prefix?, atom, infix, atom, infix, atom, ...
    // NOT: expression(prefix?, atom, infix, atom, ...)
    let pairs = CantaLoopParser::parse(Rule::expression, text)
        .map_err(|e| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { 
                message: format!("Failed to parse expression: {:?}", e)
            },
            span
        ))?;
    // parse_expression expects to iterate over the flattened pairs (atoms, operators)
    // Since expression is silent, pairs is already the flattened sequence
    let result = parse_expression(pairs, id_gen)?;
    Ok(result)
}

/// Build a CST atom (primary with postfix operators).
pub(crate) fn build_cst_atom(atom_pair: Pair<Rule>, id_gen: &mut CstIdGenerator) -> Result<Spanned<CstExpr>, pest::error::Error<Rule>> {
    let atom_span = atom_pair.as_span();
    let mut inner = atom_pair.into_inner();
    let primary_pair = inner.next().ok_or_else(|| {
        pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError {
                message: "Expected primary expression in atom".to_string(),
            },
            atom_span,
        )
    })?;
    let mut expr = build_cst_primary(primary_pair, id_gen)?;
    // Apply any postfix operators
    for postfix_pair in inner {
        match postfix_pair.as_rule() {
            Rule::postfix => {
                let postfix_span = postfix_pair.as_span();
                let mut postfix_inner = postfix_pair.into_inner();
                let postfix_op_pair = postfix_inner.next().ok_or_else(|| {
                    pest::error::Error::new_from_span(
                        pest::error::ErrorVariant::CustomError {
                            message: "Expected postfix operator".to_string(),
                        },
                        postfix_span,
                    )
                })?;
                match postfix_op_pair.as_rule() {
                    Rule::invoke => {
                        let invoke_span = postfix_op_pair.as_span();
                        let invoke_text = postfix_op_pair.as_str();
                        // Find the "!" token
                        let bang_pos = invoke_text.find('!').unwrap_or(0);
                        let bang_span = Span::from_usize(invoke_span.start() + bang_pos, invoke_span.start() + bang_pos + 1);
                        // Check if invoke has arguments (invoke_args)
                        let mut invoke_inner = postfix_op_pair.into_inner();
                        invoke_inner.next(); // Skip the "!" token
                        // Check for invoke arguments (!(...))
                        let _invoke_args = if invoke_inner.peek().is_some() {
                            // Has arguments: parse them
                            let args_pair = invoke_inner.next().ok_or_else(|| {
                                pest::error::Error::new_from_span(
                                    pest::error::ErrorVariant::CustomError {
                                        message: "Expected invoke arguments".to_string(),
                                    },
                                    invoke_span,
                                )
                            })?;
                            if args_pair.as_rule() == Rule::invoke_args {
                                let args_text = args_pair.as_str();
                                let args_start = args_pair.as_span().start();
                                let args_span = args_pair.as_span();
                                // Extract content between parentheses
                                let inner_text = if args_text.starts_with('(') && args_text.ends_with(')') {
                                    &args_text[1..args_text.len()-1].trim()
                                } else {
                                    args_text.trim()
                                };
                                if inner_text.is_empty() {
                                    Vec::new()
                                } else {
                                    parse_cst_call_argument_list(inner_text, args_span, id_gen)?
                                }
                            } else {
                                Vec::new()
                            }
                        } else {
                            Vec::new()
                        };
                        // For now, invoke postfix doesn't store arguments in CST
                        // The invoke operator is just "!" - arguments are parsed separately if needed
                        expr = Spanned::new(id_gen.next(), 
                            expr.span.merge(Span::from_pest_span(invoke_span)),
                            CstExpr::Postfix {
                                lhs: Box::new(expr),
                                op: Spanned::new(id_gen.next(), bang_span, CstPostfixOp::Invoke),
                            }
                        );
                    }
                    Rule::call => {
                        // Function call syntax: (args)
                        let call_text = postfix_op_pair.as_str();
                        let open_paren = Span::from_pest_span(postfix_op_pair.as_span());
                        let close_paren = Span::from_usize(
                            postfix_op_pair.as_span().end() - 1,
                            postfix_op_pair.as_span().end()
                        );
                        let call_content = call_text.trim();
                        let arguments = if call_content.len() >= 2 && call_content.starts_with('(') && call_content.ends_with(')') {
                            let inner_text = call_content[1..call_content.len()-1].trim();
                            if inner_text.is_empty() {
                                Vec::new()
                            } else {
                                parse_cst_call_argument_list(inner_text, postfix_op_pair.as_span(), id_gen)?
                            }
                        } else {
                            Vec::new()
                        };
                        expr = Spanned::new(id_gen.next(), 
                            expr.span.merge(close_paren),
                            CstExpr::FunctionCall {
                                callee: Box::new(expr),
                                open_paren,
                                arguments,
                                close_paren,
                            }
                        );
                    }
                    Rule::array_index => {
                        // Array indexing syntax: [index_specs]
                        let index_span = postfix_op_pair.as_span();
                        let index_text = postfix_op_pair.as_str();
                        let open_bracket = Span::from_usize(index_span.start(), index_span.start() + 1);
                        let close_bracket = Span::from_usize(index_span.end() - 1, index_span.end());
                        let index_content = index_text.trim();
                        let indices = if index_content.len() >= 2 && index_content.starts_with('[') && index_content.ends_with(']') {
                            let inner_text = index_content[1..index_content.len()-1].trim();
                            if inner_text.is_empty() {
                                Vec::new()
                            } else {
                                parse_cst_index_spec_list(inner_text, index_span, id_gen)?
                            }
                        } else {
                            Vec::new()
                        };
                        expr = Spanned::new(id_gen.next(), 
                            expr.span.merge(Span::from_pest_span(index_span)),
                            CstExpr::ArrayIndex {
                                array: Box::new(expr),
                                open_bracket,
                                indices,
                                close_bracket,
                            }
                        );
                    }
                    _ => unreachable!(),
                }
            }
            _ => unreachable!(),
        }
    }
    Ok(expr)
}

/// Build a CST primary expression.
fn build_cst_primary(primary_pair: Pair<Rule>, id_gen: &mut CstIdGenerator) -> Result<Spanned<CstExpr>, pest::error::Error<Rule>> {
    match primary_pair.as_rule() {
        Rule::primary => {
            let primary_span = primary_pair.as_span();
            let mut inner = primary_pair.into_inner();
            let base_pair = inner.next().ok_or_else(|| {
                pest::error::Error::new_from_span(
                    pest::error::ErrorVariant::CustomError {
                        message: "Expected primary expression".to_string(),
                    },
                    primary_span,
                )
            })?;
            build_cst_primary(base_pair, id_gen)
        }
        Rule::value => build_cst_value(primary_pair, id_gen),
        Rule::array_literal => build_cst_array_literal(primary_pair, id_gen),
        Rule::expression => {
            // Parenthesized expression
            let span = primary_pair.as_span();
            let text = primary_pair.as_str();
            let inner_text = &text[1..text.len()-1].trim();
            let inner_expr = build_cst_expression_from_text(inner_text, span, id_gen)?;
            let id = id_gen.next();
            Ok(Spanned::new(id, 
                Span::from_pest_span(span),
                CstExpr::Group {
                    open_paren: Span::from_usize(span.start(), span.start() + 1),
                    inner: Box::new(inner_expr),
                    close_paren: Span::from_usize(span.end() - 1, span.end()),
                }
            ))
        }
        Rule::identifier => build_cst_identifier_expr(primary_pair, id_gen),
        Rule::loop_expression => {
            build_cst_loop_expression(primary_pair, id_gen)
        }
        Rule::closure_expression | Rule::atomic_closure_expression => {
            build_cst_closure_expression(primary_pair, id_gen)
        }
        Rule::member_access => {
            build_cst_member_access(primary_pair, id_gen)
        }
        Rule::struct_literal => {
            build_cst_struct_literal(primary_pair, id_gen)
        }
        Rule::atom => {
            // CRITICAL: Handle atom rules in primary
            // When parentheses are parsed, Pest may produce atom(primary(...)) structures
            // We need to unwrap the atom to get the primary and recursively parse it
            let atom_span = primary_pair.as_span();
            let mut inner = primary_pair.into_inner();
            let inner_primary = inner.next().ok_or_else(|| {
                pest::error::Error::new_from_span(
                    pest::error::ErrorVariant::CustomError {
                        message: "Expected primary expression in atom".to_string(),
                    },
                    atom_span,
                )
            })?;
            // Recursively parse the inner primary (which might contain parentheses)
            build_cst_primary(inner_primary, id_gen)
        }
        _ => {
            Err(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: format!("Unexpected rule in primary: {:?}", primary_pair.as_rule())
                },
                primary_pair.as_span()
            ))
        }
    }
}

/// Build a CST value (number, string, boolean, array).
fn build_cst_value(pair: Pair<Rule>, id_gen: &mut CstIdGenerator) -> Result<Spanned<CstExpr>, pest::error::Error<Rule>> {
    let pair_span = pair.as_span();
    let mut inner = pair.into_inner();
    let inner_pair = inner.next().ok_or_else(|| {
        pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError {
                message: "Expected value expression".to_string(),
            },
            pair_span,
        )
    })?;
    match inner_pair.as_rule() {
        Rule::number => build_cst_number(inner_pair, id_gen),
        Rule::string => build_cst_string(inner_pair, id_gen),
        Rule::boolean => build_cst_boolean(inner_pair, id_gen),
        Rule::array_literal => build_cst_array_literal(inner_pair, id_gen),
        _ => unreachable!(),
    }
}

/// Build a CST number literal.
fn build_cst_number(pair: Pair<Rule>, id_gen: &mut CstIdGenerator) -> Result<Spanned<CstExpr>, pest::error::Error<Rule>> {
    let span = Span::from_pest_span(pair.as_span());
    let value = pair.as_str().trim().parse::<f64>()
        .map_err(|e| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: format!("Failed to parse number: {}", e) },
            pair.as_span()
        ))?;
    let id = id_gen.next();
    let lit_id = id_gen.next();
    Ok(Spanned::new(id, 
        span,
        CstExpr::Literal(Spanned::new(lit_id, span, CstLiteral::Number(value)))
    ))
}

/// Build a CST string literal.
fn build_cst_string(pair: Pair<Rule>, id_gen: &mut CstIdGenerator) -> Result<Spanned<CstExpr>, pest::error::Error<Rule>> {
    let span = Span::from_pest_span(pair.as_span());
    let str_with_quotes = pair.as_str();
    // Strip the surrounding quotes
    let string_with_escapes = &str_with_quotes[1..str_with_quotes.len() - 1];
    // Unescape the string (same logic as AST builder)
    let mut string_value = String::new();
    let mut chars = string_with_escapes.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(escaped) = chars.next() {
                match escaped {
                    '"' => string_value.push('"'),
                    '\\' => string_value.push('\\'),
                    'n' => string_value.push('\n'),
                    't' => string_value.push('\t'),
                    'r' => string_value.push('\r'),
                    'b' => string_value.push('\x08'),
                    'f' => string_value.push('\x0C'),
                    '0' => string_value.push('\0'),
                    _ => {
                        string_value.push('\\');
                        string_value.push(escaped);
                    }
                }
            } else {
                string_value.push('\\');
            }
        } else {
            string_value.push(ch);
        }
    }
    let id = id_gen.next();
    let lit_id = id_gen.next();
    Ok(Spanned::new(id, 
        span,
        CstExpr::Literal(Spanned::new(lit_id, span, CstLiteral::String(string_value)))
    ))
}

/// Build a CST boolean literal.
fn build_cst_boolean(pair: Pair<Rule>, id_gen: &mut CstIdGenerator) -> Result<Spanned<CstExpr>, pest::error::Error<Rule>> {
    let span = Span::from_pest_span(pair.as_span());
    let value = pair.as_str().trim().parse::<bool>()
        .map_err(|e| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: format!("Failed to parse boolean: {}", e) },
            pair.as_span()
        ))?;
    let id = id_gen.next();
    let lit_id = id_gen.next();
    Ok(Spanned::new(id, 
        span,
        CstExpr::Literal(Spanned::new(lit_id, span, CstLiteral::Boolean(value)))
    ))
}

/// Build a CST array literal.
fn build_cst_array_literal(pair: Pair<Rule>, id_gen: &mut CstIdGenerator) -> Result<Spanned<CstExpr>, pest::error::Error<Rule>> {
    let pest_span = pair.as_span();
    let span = Span::from_pest_span(pest_span);
    let text = pair.as_str();
    // Find opening bracket
    let bracket_start_pos = text.find('[')
        .ok_or_else(|| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "Array literal missing opening bracket".to_string() },
            pest_span
        ))?;
    let open_bracket = Span::from_usize(
        span.start as usize + bracket_start_pos,
        span.start as usize + bracket_start_pos + 1
    );
    // Find closing bracket
    let bracket_end_pos = text.rfind(']')
        .ok_or_else(|| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "Array literal missing closing bracket".to_string() },
            pest_span
        ))?;
    let close_bracket = Span::from_usize(
        span.start as usize + bracket_end_pos,
        span.start as usize + bracket_end_pos + 1
    );
    // Extract and parse elements
    let elements_text = text[bracket_start_pos + 1..bracket_end_pos].trim();
    let elements = if elements_text.is_empty() {
        Vec::new()
    } else {
        parse_cst_expression_list(elements_text, pest_span, id_gen)?
    };
    let id = id_gen.next();
    Ok(Spanned::new(id, 
        span,
        CstExpr::Array {
            open_bracket,
            elements,
            close_bracket,
        }
    ))
}

/// Build a CST identifier expression.
fn build_cst_identifier_expr(pair: Pair<Rule>, id_gen: &mut CstIdGenerator) -> Result<Spanned<CstExpr>, pest::error::Error<Rule>> {
    let span = Span::from_pest_span(pair.as_span());
    let identifier = pair.as_str().to_string();
    let id = id_gen.next();
    let ident_id = id_gen.next();
    Ok(Spanned::new(id, 
        span,
        CstExpr::Identifier(Spanned::new(ident_id, span, identifier))
    ))
}

/// Build a CST member access expression (e.g., "utils.add").
fn build_cst_member_access(pair: Pair<Rule>, id_gen: &mut CstIdGenerator) -> Result<Spanned<CstExpr>, pest::error::Error<Rule>> {
    let span = pair.as_span();
    let text = pair.as_str();
    let start_offset = span.start();
    // Member access is: identifier ~ ("." ~ identifier)+
    let mut identifiers = Vec::new();
    let mut dots = Vec::new();
    // Split by dots and track their positions
    let parts: Vec<&str> = text.split('.').collect();
    let mut current_pos = 0;
    for (i, part) in parts.iter().enumerate() {
        let trimmed = part.trim();
        if !trimmed.is_empty() {
            let id_start = start_offset + current_pos + (part.len() - trimmed.len());
            let id_span = Span::from_usize(id_start, id_start + trimmed.len());
            let ident_id = id_gen.next();
            identifiers.push(Spanned::new(ident_id, id_span, trimmed.to_string()));
        }
        // Track dot position (except for last part)
        if i < parts.len() - 1 {
            let dot_pos = text[current_pos..].find('.').unwrap_or(0) + current_pos;
            let dot_span = Span::from_usize(start_offset + dot_pos, start_offset + dot_pos + 1);
            dots.push(dot_span);
            current_pos = dot_pos + 1;
        }
    }
    if identifiers.len() < 2 {
        return Err(pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "Member access requires at least two identifiers".to_string() },
            span
        ));
    }
    // Build object expression (first identifier)
    let obj_id = id_gen.next();
    let obj_ident_id = id_gen.next();
    let object = Box::new(Spanned::new(
        obj_id,
        identifiers[0].span,
        CstExpr::Identifier(Spanned::new(obj_ident_id, identifiers[0].span, identifiers[0].node.clone()))
    ));
    // Rest are members
    let members = identifiers[1..].to_vec();
    let id = id_gen.next();
    Ok(Spanned::new(id, 
        Span::from_pest_span(span),
        CstExpr::MemberAccess {
            object,
            dots,
            members,
        }
    ))
}

/// Build a CST struct literal (e.g., "Point { x: 10, y: 20 }").
fn build_cst_struct_literal(pair: Pair<Rule>, id_gen: &mut CstIdGenerator) -> Result<Spanned<CstExpr>, pest::error::Error<Rule>> {
    let span = pair.as_span();
    let text = pair.as_str();
    let start_offset = span.start();
    // Find opening brace
    let brace_start = text.find('{')
        .ok_or_else(|| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "Struct literal missing opening brace".to_string() },
            span
        ))?;
    let open_brace = Span::from_usize(start_offset + brace_start, start_offset + brace_start + 1);
    // Extract struct name (everything before "{")
    let struct_name_text = text[..brace_start].trim();
    let struct_name_id = id_gen.next();
    let struct_name = Spanned::new(
        struct_name_id,
        Span::from_usize(start_offset, start_offset + struct_name_text.len()),
        struct_name_text.to_string()
    );
    // Find closing brace
    let brace_end = text.rfind('}')
        .ok_or_else(|| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "Struct literal missing closing brace".to_string() },
            span
        ))?;
    let close_brace = Span::from_usize(start_offset + brace_end, start_offset + brace_end + 1);
    // Extract fields content
    let fields_content = text[brace_start + 1..brace_end].trim();
    let fields = if fields_content.is_empty() {
        Vec::new()
    } else {
        parse_cst_struct_init_fields(fields_content, Span::from_usize(start_offset + brace_start + 1, start_offset + brace_end), span, id_gen)?
    };
    Ok(Spanned::new(id_gen.next(), 
        Span::from_pest_span(span),
        CstExpr::StructInit {
            struct_name,
            open_brace,
            fields,
            close_brace,
        }
    ))
}

/// Build a CST closure expression (e.g., "fn(x) => x + 1").
fn build_cst_closure_expression(pair: Pair<Rule>, id_gen: &mut CstIdGenerator) -> Result<Spanned<CstExpr>, pest::error::Error<Rule>> {
    let span = pair.as_span();
    let text = pair.as_str();
    let start_offset = span.start();
    // Find "fn" keyword
    let fn_start = text.find("fn").unwrap_or(0) + start_offset;
    let fn_keyword = Span::from_usize(fn_start, fn_start + 2);
    // Find opening and closing parens
    let open_paren_pos = text.find('(')
        .ok_or_else(|| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "Closure missing opening paren".to_string() },
            span
        ))?;
    let close_paren_pos = text[open_paren_pos..].find(')')
        .map(|pos| open_paren_pos + pos)
        .ok_or_else(|| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "Closure missing closing paren".to_string() },
            span
        ))?;
    let open_paren = Span::from_usize(start_offset + open_paren_pos, start_offset + open_paren_pos + 1);
    let close_paren = Span::from_usize(start_offset + close_paren_pos, start_offset + close_paren_pos + 1);
    // Parse arguments (between parens)
    let args_text = text[open_paren_pos + 1..close_paren_pos].trim();
    let arguments = if args_text.is_empty() {
        Vec::new()
    } else {
        parse_cst_closure_arguments(args_text, Span::from_usize(start_offset + open_paren_pos + 1, start_offset + close_paren_pos), span, id_gen)?
    };
    // Find return type arrow and body
    let after_paren = &text[close_paren_pos + 1..];
    let (return_type_arrow, body_start_offset) = extract_cst_closure_return_type(after_paren, start_offset + close_paren_pos + 1, span, id_gen)?;
    // Find arrow "=>" or opening brace
    let body_text_start = &after_paren[body_start_offset..];
    let arrow_pos = body_text_start.find("=>");
    let brace_pos = body_text_start.find('{');
    let arrow = arrow_pos.map(|pos| Span::from_usize(start_offset + close_paren_pos + 1 + body_start_offset + pos, start_offset + close_paren_pos + 1 + body_start_offset + pos + 2));
    let body = if let Some(pos) = arrow_pos {
        // Has arrow, extract body after "=>"
        let body_text = body_text_start[pos + 2..].trim();
        if body_text.starts_with('{') {
            // Block body
            let block_start_in_text = close_paren_pos + 1 + body_start_offset + pos + 2;
            let whitespace_skip = body_text_start[pos + 2..].len() - body_text.len();
            let actual_block_start = block_start_in_text + whitespace_skip;
            let block_content = extract_cst_braced_content(text, actual_block_start, span)?;
            let block_text = format!("{{{}}}", block_content);
            
            // CRITICAL FIX: Clone the string to ensure independent parse
            let isolated_block_text = block_text.clone();
            let mut block_parse_result = CantaLoopParser::parse(Rule::braced_block, &isolated_block_text)
                .map_err(|e| pest::error::Error::new_from_span(
                    pest::error::ErrorVariant::CustomError { message: format!("Failed to parse closure block: {}", e) },
                    span
                ))?;
            
            if let Some(block_pair) = block_parse_result.next() {
                let mut block_inner_iter = block_pair.into_inner();
                let block_inner = block_inner_iter
                    .find(|p| p.as_rule() == Rule::block)
                    .ok_or_else(|| pest::error::Error::new_from_span(
                        pest::error::ErrorVariant::CustomError { message: "Closure block missing inner block".to_string() },
                        span
                    ))?;
                let (body_block, _) = build_cst_block(block_inner, id_gen)?;
                eprintln!(
                    "[CST Builder] Closure body block created: id={:?}, ptr={:p}, stmt_count={}, statements_vec_ptr={:p}",
                    body_block.id,
                    &body_block.node,
                    body_block.node.statements.len(),
                    body_block.node.statements.as_ptr()
                );
                CstClosureBody::Block(body_block)
            } else {
                let empty_block = Spanned::new(id_gen.next(), Span::from_pest_span(span), CstBlock { statements: Vec::new() });
                eprintln!(
                    "[CST Builder] Closure body block (empty): id={:?}, ptr={:p}",
                    empty_block.id,
                    &empty_block.node
                );
                CstClosureBody::Block(empty_block)
            }
        } else {
            // Expression body
            let expr = build_cst_expression_from_text(body_text, span, id_gen)?;
            CstClosureBody::Expression(Box::new(expr))
        }
    } else if let Some(bp) = brace_pos {
        // No arrow, but has block
        let block_start = close_paren_pos + 1 + body_start_offset + bp;
        let block_content = extract_cst_braced_content(text, block_start, span)?;
        let block_text = format!("{{{}}}", block_content);
        
        // CRITICAL FIX: Clone the string to ensure independent parse
        let isolated_block_text = block_text.clone();
        let mut block_parse_result = CantaLoopParser::parse(Rule::braced_block, &isolated_block_text)
            .map_err(|e| pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError { message: format!("Failed to parse closure block: {}", e) },
                span
            ))?;
        
        if let Some(block_pair) = block_parse_result.next() {
            let mut block_inner_iter = block_pair.into_inner();
            let block_inner = block_inner_iter
                .find(|p| p.as_rule() == Rule::block)
                .ok_or_else(|| pest::error::Error::new_from_span(
                    pest::error::ErrorVariant::CustomError { message: "Closure block missing inner block".to_string() },
                    span
                ))?;
            let (body_block, _) = build_cst_block(block_inner, id_gen)?;
            eprintln!(
                "[CST Builder] Closure body block created: id={:?}, ptr={:p}, stmt_count={}, statements_vec_ptr={:p}",
                body_block.id,
                &body_block.node,
                body_block.node.statements.len(),
                body_block.node.statements.as_ptr()
            );
            CstClosureBody::Block(body_block)
        } else {
            let empty_block = Spanned::new(id_gen.next(), Span::from_pest_span(span), CstBlock { statements: Vec::new() });
            eprintln!(
                "[CST Builder] Closure body block (empty): id={:?}, ptr={:p}",
                empty_block.id,
                &empty_block.node
            );
            CstClosureBody::Block(empty_block)
        }
    } else {
        return Err(pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "Closure must have either '=>' followed by expression/block, or a block body".to_string() },
            span
        ));
    };
    Ok(Spanned::new(id_gen.next(), 
        Span::from_pest_span(span),
        CstExpr::Closure {
            fn_keyword,
            open_paren,
            arguments,
            close_paren,
            return_type_arrow,
            arrow,
            body,
        }
    ))
}

/// Parse an expression list (for arrays, function calls, etc.) with spans.
/// Forward declaration helper.
fn parse_cst_expression_list(text: &str, parent_span: pest::Span, id_gen: &mut CstIdGenerator) -> Result<Vec<Spanned<CstExpr>>, pest::error::Error<Rule>> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    // Split by commas, being careful about commas inside nested structures
    let mut expressions = Vec::new();
    let mut current_start = 0;
    let mut depth = 0;
    let mut in_string = false;
    let mut escape_next = false;
    let chars: Vec<char> = text.chars().collect();
    let start_offset = parent_span.start();
    for (i, &ch) in chars.iter().enumerate() {
        if escape_next {
            escape_next = false;
            continue;
        }
        match ch {
            '\\' if in_string => escape_next = true,
            '"' => in_string = !in_string,
            '(' | '[' | '{' if !in_string => depth += 1,
            ')' | ']' | '}' if !in_string => depth -= 1,
            ',' if depth == 0 && !in_string => {
                let expr_text = text[current_start..i].trim();
                if !expr_text.is_empty() {
                    let expr_span = safe_pest_span(
                        parent_span.as_str(),
                        start_offset + current_start + (text[current_start..i].len() - expr_text.len()),
                        start_offset + current_start + (text[current_start..i].len() - expr_text.len()) + expr_text.len(),
                        parent_span,
                    )?;
                    expressions.push(build_cst_expression_from_text(expr_text, expr_span, id_gen)?);
                }
                current_start = i + 1;
            }
            _ => {}
        }
    }
    // Parse the last expression
    let expr_text = text[current_start..].trim();
    if !expr_text.is_empty() {
        let expr_span = safe_pest_span(
            parent_span.as_str(),
            start_offset + current_start + (text[current_start..].len() - expr_text.len()),
            start_offset + current_start + (text[current_start..].len() - expr_text.len()) + expr_text.len(),
            parent_span,
        )?;
        expressions.push(build_cst_expression_from_text(expr_text, expr_span, id_gen)?);
    }
    Ok(expressions)
}

/// Parse a call argument list (expressions and holes) with spans.
fn parse_cst_call_argument_list(text: &str, parent_span: pest::Span, id_gen: &mut CstIdGenerator) -> Result<Vec<Spanned<CstCallArgument>>, pest::error::Error<Rule>> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    // Split by commas, being careful about commas inside nested structures
    let mut arguments = Vec::new();
    let mut current_start_byte = 0;
    let mut depth = 0;
    let mut in_string = false;
    let mut escape_next = false;
    let start_offset = parent_span.start();
    // Use char_indices to track both character and byte positions
    let mut char_iter = text.char_indices().peekable();
    let mut current_char_idx = 0;
    while let Some((byte_pos, ch)) = char_iter.next() {
        if escape_next {
            escape_next = false;
            current_char_idx += 1;
            continue;
        }
        match ch {
            '\\' if in_string => {
                escape_next = true;
                current_char_idx += 1;
            }
            '"' => {
                in_string = !in_string;
                current_char_idx += 1;
            }
            '(' | '[' | '{' if !in_string => {
                depth += 1;
                current_char_idx += 1;
            }
            ')' | ']' | '}' if !in_string => {
                depth -= 1;
                current_char_idx += 1;
            }
            ',' if depth == 0 && !in_string => {
                // byte_pos is the position of the comma, so we want everything before it
                let arg_slice = &text[current_start_byte..byte_pos];
                let arg_text = arg_slice.trim();
                if !arg_text.is_empty() {
                    // Find the trimmed start position within the slice
                    let trimmed_start_in_slice = arg_slice.find(arg_text).unwrap_or(0);
                    let arg_start_byte_pos = current_start_byte + trimmed_start_in_slice;
                    let arg_end_byte_pos = arg_start_byte_pos + arg_text.len();
                    // Create span relative to parent
                    let arg_start_pos = start_offset + arg_start_byte_pos;
                    let arg_end_pos = start_offset + arg_end_byte_pos;
                    if let Some(arg_span) = pest::Span::new(
                        parent_span.as_str(),
                        arg_start_pos,
                        arg_end_pos
                    ) {
                        arguments.push(parse_cst_call_argument(arg_text, arg_span, id_gen)?);
                    } else {
                        // Fallback: use the text string directly for span creation
                        let fallback_span = pest::Span::new(
                            text,
                            arg_start_byte_pos,
                            arg_end_byte_pos
                        ).ok_or_else(|| pest::error::Error::new_from_span(
                            pest::error::ErrorVariant::CustomError { message: "Failed to create span for call argument".to_string() },
                            parent_span
                        ))?;
                        arguments.push(parse_cst_call_argument(arg_text, fallback_span, id_gen)?);
                    }
                }
                // Move past the comma - find the next character's byte position
                current_start_byte = if let Some((next_byte_pos, _)) = char_iter.peek() {
                    *next_byte_pos
                } else {
                    text.len()
                };
                current_char_idx += 1;
            }
            _ => {
                current_char_idx += 1;
            }
        }
    }
    // Parse the last argument
    let arg_slice = &text[current_start_byte..];
    let arg_text = arg_slice.trim();
    if !arg_text.is_empty() {
        let trimmed_start_in_slice = arg_slice.find(arg_text).unwrap_or(0);
        let arg_start_byte_pos = current_start_byte + trimmed_start_in_slice;
        let arg_end_byte_pos = arg_start_byte_pos + arg_text.len();
        let arg_start_pos = start_offset + arg_start_byte_pos;
        let arg_end_pos = start_offset + arg_end_byte_pos;
        if let Some(arg_span) = pest::Span::new(
            parent_span.as_str(),
            arg_start_pos,
            arg_end_pos
        ) {
            arguments.push(parse_cst_call_argument(arg_text, arg_span, id_gen)?);
        } else {
            // Fallback: use the text string directly for span creation
            let fallback_span = pest::Span::new(
                text,
                arg_start_byte_pos,
                arg_end_byte_pos
            ).ok_or_else(|| pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError { message: "Failed to create span for call argument".to_string() },
                parent_span
            ))?;
            arguments.push(parse_cst_call_argument(arg_text, fallback_span, id_gen)?);
        }
    }
    Ok(arguments)
}

/// Parse a single call argument (expression or hole) with span.
fn parse_cst_call_argument(text: &str, span: pest::Span, id_gen: &mut CstIdGenerator) -> Result<Spanned<CstCallArgument>, pest::error::Error<Rule>> {
    let trimmed = text.trim();
    if trimmed == "?" {
        let hole_span = Span::from_pest_span(span);
        // #region agent log
        debug_log("D", "cst/builder.rs:parse_cst_call_argument", "Found ? placeholder", serde_json::json!({
            "span_start": hole_span.start,
            "span_end": hole_span.end,
            "text": trimmed
        }));
        // #endregion
        Ok(Spanned::new(id_gen.next(), 
            hole_span,
            CstCallArgument::Hole(hole_span)
        ))
    } else {
        let expr = build_cst_expression_from_text(trimmed, span, id_gen)?;
        Ok(Spanned::new(id_gen.next(), 
            Span::from_pest_span(span),
            CstCallArgument::Expr(expr)
        ))
    }
}

/// Extract return type arrow from function/closure text (-> or ~>).
fn extract_cst_return_type_arrow(
    text: &str,
    start_offset: usize,
    parent_span: pest::Span,
    id_gen: &mut CstIdGenerator,
) -> Result<Option<Spanned<ReturnTypeArrow>>, pest::error::Error<Rule>> {
    let trimmed = text.trim_start();
    let trim_offset = text.len() - trimmed.len();
    if let Some(arrow_pos) = trimmed.find("->") {
        let arrow_start = start_offset + trim_offset + arrow_pos;
        let arrow_span = Span::from_usize(arrow_start, arrow_start + 2);
        // Find where type annotation ends (before => or { or end)
        let after_arrow = &trimmed[arrow_pos + 2..];
        let after_arrow_trimmed = after_arrow.trim_start();
        let after_arrow_trim_offset = after_arrow.len() - after_arrow_trimmed.len();
        let arrow_arrow_pos = after_arrow_trimmed.find("=>").unwrap_or(after_arrow_trimmed.len());
        let brace_pos = after_arrow_trimmed.find('{').unwrap_or(after_arrow_trimmed.len());
        let delimiter_pos = arrow_arrow_pos.min(brace_pos);
        let type_text = after_arrow_trimmed[..delimiter_pos].trim();
        let type_start = arrow_start + 2 + after_arrow_trim_offset;
        let type_span = Span::from_usize(type_start, type_start + type_text.len());
        Ok(Some(Spanned::new(id_gen.next(), 
            arrow_span,
            ReturnTypeArrow {
                arrow: arrow_span,
                is_effectful: false,
                type_annotation: Spanned::new(id_gen.next(), type_span, type_text.to_string()),
            }
        )))
    } else if let Some(arrow_pos) = trimmed.find("~>") {
        let arrow_start = start_offset + trim_offset + arrow_pos;
        let arrow_span = Span::from_usize(arrow_start, arrow_start + 2);
        // Find where type annotation ends (before => or { or end)
        let after_arrow = &trimmed[arrow_pos + 2..];
        let after_arrow_trimmed = after_arrow.trim_start();
        let after_arrow_trim_offset = after_arrow.len() - after_arrow_trimmed.len();
        let arrow_arrow_pos = after_arrow_trimmed.find("=>").unwrap_or(after_arrow_trimmed.len());
        let brace_pos = after_arrow_trimmed.find('{').unwrap_or(after_arrow_trimmed.len());
        let delimiter_pos = arrow_arrow_pos.min(brace_pos);
        let type_text = after_arrow_trimmed[..delimiter_pos].trim();
        let type_start = arrow_start + 2 + after_arrow_trim_offset;
        let type_span = Span::from_usize(type_start, type_start + type_text.len());
        Ok(Some(Spanned::new(id_gen.next(), 
            arrow_span,
            ReturnTypeArrow {
                arrow: arrow_span,
                is_effectful: true,
                type_annotation: Spanned::new(id_gen.next(), type_span, type_text.to_string()),
            }
        )))
    } else {
        Ok(None)
    }
}

/// Extract closure return type and calculate body start offset.
fn extract_cst_closure_return_type(
    text: &str,
    start_offset: usize,
    parent_span: pest::Span,
    id_gen: &mut CstIdGenerator,
) -> Result<(Option<Spanned<ReturnTypeArrow>>, usize), pest::error::Error<Rule>> {
    let trimmed = text.trim_start();
    let trim_offset = text.len() - trimmed.len();
    if !trimmed.starts_with("->") {
        return Ok((None, trim_offset));
    }
    let arrow_pos = trimmed.find("->").ok_or_else(|| {
        pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError {
                message: "Expected '->' in return type".to_string(),
            },
            parent_span,
        )
    })?;
    let after_arrow_raw = &trimmed[arrow_pos + 2..];
    let after_arrow = after_arrow_raw.trim_start();
    let after_arrow_trim_offset = after_arrow_raw.len() - after_arrow.len();
    // Find the position of => or {, whichever comes first
    let arrow_arrow_pos = after_arrow.find("=>").unwrap_or(after_arrow.len());
    let brace_pos = after_arrow.find('{').unwrap_or(after_arrow.len());
    let delimiter_pos = arrow_arrow_pos.min(brace_pos);
    let type_text = after_arrow[..delimiter_pos].trim();
    let arrow_start = start_offset + trim_offset + arrow_pos;
    let arrow_span = Span::from_usize(arrow_start, arrow_start + 2);
    let type_start = arrow_start + 2 + after_arrow_trim_offset;
    let type_span = Span::from_usize(type_start, type_start + type_text.len());
    let return_type = Some(Spanned::new(id_gen.next(), 
        arrow_span,
        ReturnTypeArrow {
            arrow: arrow_span,
            is_effectful: false,
            type_annotation: Spanned::new(id_gen.next(), type_span, type_text.to_string()),
        }
    ));
    // Calculate the offset where the body starts (after "-> type " or "-> type=>" or "-> type{")
    let body_start_offset = trim_offset + arrow_pos + 2 + after_arrow_trim_offset + delimiter_pos;
    Ok((return_type, body_start_offset))
}

/// Parse import selector with spans.
fn parse_cst_import_selector(
    text: &str,
    span: Span,
    parent_span: pest::Span,
    id_gen: &mut CstIdGenerator,
) -> Result<Spanned<CstImportSelector>, pest::error::Error<Rule>> {
    let trimmed = text.trim();
    // Calculate offset due to leading whitespace that was trimmed
    let trim_offset = text.len() - text.trim_start().len();

    // Check for wildcard: *
    if trimmed == "*" {
        return Ok(Spanned::new(id_gen.next(),
            span,
            CstImportSelector::Wildcard(Span::new(span.start + trim_offset as u32, span.start + trim_offset as u32 + 1))
        ));
    }
    // Parse comma-separated identifiers
    let parts: Vec<&str> = trimmed.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
    if parts.len() == 1 {
        // Single identifier
        let part = parts[0];
        let part_start = trimmed.find(part).unwrap_or(0);
        Ok(Spanned::new(id_gen.next(),
            span,
            CstImportSelector::Single(Spanned::new(id_gen.next(),
                Span::from_usize(span.start as usize + trim_offset + part_start, span.start as usize + trim_offset + part_start + part.len()),
                part.to_string()
            ))
        ))
    } else if parts.len() > 1 {
        // Multiple identifiers
        let mut spanned_parts = Vec::new();
        let mut current_pos = 0;
        for part in &parts {
            let part_pos = trimmed[current_pos..].find(part).unwrap_or(0) + current_pos;
            spanned_parts.push(Spanned::new(id_gen.next(),
                Span::from_usize(span.start as usize + trim_offset + part_pos, span.start as usize + trim_offset + part_pos + part.len()),
                part.to_string()
            ));
            current_pos = part_pos + part.len();
            // Skip comma
            if let Some(comma_pos) = trimmed[current_pos..].find(',') {
                current_pos += comma_pos + 1;
            }
        }
        Ok(Spanned::new(id_gen.next(), span, CstImportSelector::Multiple(spanned_parts)))
    } else {
        Err(pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "Invalid import items".to_string() },
            parent_span
        ))
    }
}

/// Parse struct fields with spans.
fn parse_cst_struct_fields(
    fields_text: &str,
    span: Span,
    parent_span: pest::Span,
    id_gen: &mut CstIdGenerator,
) -> Result<Vec<Spanned<crate::core::cst::CstStructField>>, pest::error::Error<Rule>> {
    use crate::core::cst::CstStructField;
    let trimmed = fields_text.trim();
    let mut parse_result = CantaLoopParser::parse(Rule::struct_fields, trimmed)
        .map_err(|e| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: format!("Failed to parse struct fields: {}", e) },
            parent_span
        ))?;
    let mut fields = Vec::new();
    if let Some(fields_pair) = parse_result.next() {
        for field_pair in fields_pair.into_inner() {
            if field_pair.as_rule() == Rule::struct_field {
                let field_span = field_pair.as_span();
                let field_text = field_pair.as_str();
                let start_offset = span.start as usize + field_text.as_ptr() as usize - fields_text.as_ptr() as usize;
                let field_pair_span = field_pair.as_span();
                let mut field_inner = field_pair.into_inner();
                let field_name_pair = field_inner.next().ok_or_else(|| {
                    pest::error::Error::new_from_span(
                        pest::error::ErrorVariant::CustomError {
                            message: "Expected field name in struct field".to_string(),
                        },
                        field_pair_span,
                    )
                })?;
                let field_name = field_name_pair.as_str().to_string();
                let name_pos_in_field = field_text.find(&field_name).unwrap_or(0);
                let colon_pos = field_text.find(':').unwrap_or(0);
                let type_pair = field_inner.next().ok_or_else(|| {
                    pest::error::Error::new_from_span(
                        pest::error::ErrorVariant::CustomError {
                            message: "Expected field type in struct field".to_string(),
                        },
                        field_pair_span,
                    )
                })?;
                let type_text = type_pair.as_str();
                let type_pos_in_field = field_text.find(type_text).unwrap_or(colon_pos + 1);
                fields.push(Spanned::new(id_gen.next(), 
                    Span::from_usize(span.start as usize + start_offset, span.start as usize + start_offset + field_text.len()),
                    CstStructField {
                        name: Spanned::new(id_gen.next(), 
                            Span::from_usize(span.start as usize + start_offset + name_pos_in_field, span.start as usize + start_offset + name_pos_in_field + field_name.len()),
                            field_name
                        ),
                        colon: Span::from_usize(span.start as usize + start_offset + colon_pos, span.start as usize + start_offset + colon_pos + 1),
                        type_annotation: Spanned::new(id_gen.next(), 
                            Span::from_usize(span.start as usize + start_offset + type_pos_in_field, span.start as usize + start_offset + type_pos_in_field + type_text.len()),
                            type_text.to_string()
                        ),
                    }
                ));
            }
        }
    }
    Ok(fields)
}

/// Parse struct initialization fields with spans.
fn parse_cst_struct_init_fields(
    fields_text: &str,
    span: Span,
    parent_span: pest::Span,
    id_gen: &mut CstIdGenerator,
) -> Result<Vec<Spanned<crate::core::cst::CstStructInitField>>, pest::error::Error<Rule>> {
    use crate::core::cst::CstStructInitField;
    let trimmed = fields_text.trim();
    let mut parse_result = CantaLoopParser::parse(Rule::struct_init_fields, trimmed)
        .map_err(|e| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: format!("Failed to parse struct init fields: {}", e) },
            parent_span
        ))?;
    let mut fields = Vec::new();
    if let Some(fields_pair) = parse_result.next() {
        for field_pair in fields_pair.into_inner() {
            if field_pair.as_rule() == Rule::struct_init_field {
                let field_span = field_pair.as_span();
                let field_text = field_pair.as_str();
                let start_offset = span.start as usize + field_text.as_ptr() as usize - fields_text.as_ptr() as usize;
                // Find the colon to split identifier from expression
                let colon_pos = field_text.find(':')
                    .ok_or_else(|| pest::error::Error::new_from_span(
                        pest::error::ErrorVariant::CustomError { message: "Struct init field missing colon".to_string() },
                        field_span
                    ))?;
                let identifier_text = field_text[..colon_pos].trim();
                let expr_text = field_text[colon_pos + 1..].trim();
                if identifier_text.is_empty() {
                    return Err(pest::error::Error::new_from_span(
                        pest::error::ErrorVariant::CustomError { message: "Struct init field missing identifier".to_string() },
                        field_span
                    ));
                }
                if expr_text.is_empty() {
                    return Err(pest::error::Error::new_from_span(
                        pest::error::ErrorVariant::CustomError { message: "Struct init field missing expression".to_string() },
                        field_span
                    ));
                }
                let id_pos_in_field = field_text.find(identifier_text).unwrap_or(0);
                let expr_pos_in_field = field_text.find(expr_text).unwrap_or(colon_pos + 1);
                let value = build_cst_expression_from_text(expr_text, field_span, id_gen)?;
                fields.push(Spanned::new(id_gen.next(), 
                    Span::from_usize(span.start as usize + start_offset, span.start as usize + start_offset + field_text.len()),
                    CstStructInitField {
                        name: Spanned::new(id_gen.next(), 
                            Span::from_usize(span.start as usize + start_offset + id_pos_in_field, span.start as usize + start_offset + id_pos_in_field + identifier_text.len()),
                            identifier_text.to_string()
                        ),
                        colon: Span::from_usize(span.start as usize + start_offset + colon_pos, span.start as usize + start_offset + colon_pos + 1),
                        value,
                    }
                ));
            }
        }
    }
    Ok(fields)
}

/// Parse closure arguments with spans.
fn parse_cst_closure_arguments(
    args_text: &str,
    span: Span,
    parent_span: pest::Span,
    id_gen: &mut CstIdGenerator,
) -> Result<Vec<Spanned<crate::core::cst::CstClosureArg>>, pest::error::Error<Rule>> {
    use crate::core::cst::CstClosureArg;
    let mut args_pairs = CantaLoopParser::parse(Rule::closure_args, args_text)
        .map_err(|e| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: format!("Failed to parse closure arguments: {}", e) },
            parent_span
        ))?;
    let mut arguments = Vec::new();
    if let Some(args_pair) = args_pairs.next() {
        for arg_pair in args_pair.into_inner() {
            if arg_pair.as_rule() == Rule::closure_arg {
                let arg_span = arg_pair.as_span();
                let arg_text = arg_pair.as_str();
                let start_offset = span.start as usize + arg_text.as_ptr() as usize - args_text.as_ptr() as usize;
                let mut arg_inner = arg_pair.into_inner();
                let first = arg_inner.next()
                    .ok_or_else(|| pest::error::Error::new_from_span(
                        pest::error::ErrorVariant::CustomError { message: "Closure argument missing identifier or placeholder".to_string() },
                        arg_span
                    ))?;
                let (identifier_text, is_placeholder) = match first.as_rule() {
                    Rule::identifier => (first.as_str().to_string(), false),
                    Rule::placeholder => ("?".to_string(), true),
                    _ => return Err(pest::error::Error::new_from_span(
                        pest::error::ErrorVariant::CustomError { message: "Expected identifier or placeholder".to_string() },
                        arg_span
                    )),
                };
                let id_pos_in_arg = arg_text.find(&identifier_text).unwrap_or(0);
                // Get optional type annotation
                let (colon, type_annotation) = if let Some(type_pair) = arg_inner.next() {
                    let colon_pos = arg_text.find(':').unwrap_or(id_pos_in_arg + identifier_text.len());
                    let type_text = type_pair.as_str();
                    let type_pos_in_arg = arg_text.find(type_text).unwrap_or(colon_pos + 1);
                    (
                        Some(Span::from_usize(span.start as usize + start_offset + colon_pos, span.start as usize + start_offset + colon_pos + 1)),
                        Some(Spanned::new(id_gen.next(), 
                            Span::from_usize(span.start as usize + start_offset + type_pos_in_arg, span.start as usize + start_offset + type_pos_in_arg + type_text.len()),
                            type_text.to_string()
                        ))
                    )
                } else {
                    (None, None)
                };
                arguments.push(Spanned::new(id_gen.next(), 
                    Span::from_usize(span.start as usize + start_offset, span.start as usize + start_offset + arg_text.len()),
                    CstClosureArg {
                        identifier: Spanned::new(id_gen.next(), 
                            Span::from_usize(span.start as usize + start_offset + id_pos_in_arg, span.start as usize + start_offset + id_pos_in_arg + identifier_text.len()),
                            identifier_text
                        ),
                        is_placeholder,
                        colon,
                        type_annotation,
                    }
                ));
            }
        }
    }
    Ok(arguments)
}

/// Parse index spec list with spans (ranges, single indices, etc.).
fn parse_cst_index_spec_list(
    text: &str,
    parent_span: pest::Span,
    id_gen: &mut CstIdGenerator,
) -> Result<Vec<Spanned<CstIndexSpec>>, pest::error::Error<Rule>> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    // Split by commas, being careful about commas inside nested structures
    let mut specs = Vec::new();
    let mut current_start = 0;
    let mut depth = 0;
    let mut in_string = false;
    let mut escape_next = false;
    let chars: Vec<char> = text.chars().collect();
    let start_offset = parent_span.start();
    for (i, &ch) in chars.iter().enumerate() {
        if escape_next {
            escape_next = false;
            continue;
        }
        match ch {
            '\\' if in_string => escape_next = true,
            '"' => in_string = !in_string,
            '(' | '[' | '{' if !in_string => depth += 1,
            ')' | ']' | '}' if !in_string => depth -= 1,
            ',' if depth == 0 && !in_string => {
                let spec_text = text[current_start..i].trim();
                if !spec_text.is_empty() {
                    let spec_span = safe_pest_span(
                        parent_span.as_str(),
                        start_offset + current_start + (text[current_start..i].len() - spec_text.len()),
                        start_offset + current_start + (text[current_start..i].len() - spec_text.len()) + spec_text.len(),
                        parent_span,
                    )?;
                    specs.push(parse_cst_index_spec(spec_text, spec_span, id_gen)?);
                }
                current_start = i + 1;
            }
            _ => {}
        }
    }
    // Parse the last spec
    let spec_text = text[current_start..].trim();
    if !spec_text.is_empty() {
        let spec_span = safe_pest_span(
            parent_span.as_str(),
            start_offset + current_start + (text[current_start..].len() - spec_text.len()),
            start_offset + current_start + (text[current_start..].len() - spec_text.len()) + spec_text.len(),
            parent_span,
        )?;
        specs.push(parse_cst_index_spec(spec_text, spec_span, id_gen)?);
    }
    Ok(specs)
}

/// Parse a single index spec from text.
fn parse_cst_index_spec(text: &str, span: pest::Span, id_gen: &mut CstIdGenerator) -> Result<Spanned<CstIndexSpec>, pest::error::Error<Rule>> {
    let trimmed = text.trim();
    let start_offset = span.start();
    // Handle full range: ..
    if trimmed == ".." {
        let dotdot_span = Span::from_usize(start_offset, start_offset + 2);
        return Ok(Spanned::new(id_gen.next(), 
            Span::from_pest_span(span),
            CstIndexSpec::Range {
                start: None,
                dotdot: dotdot_span,
                end: None,
                step: None,
            }
        ));
    }
    // Handle partial ranges: ..expr or expr..
    if trimmed.starts_with("..") && trimmed != ".." {
        // From start: ..expr
        let expr_text = trimmed[2..].trim();
        if expr_text.is_empty() {
            return Err(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError { message: "Invalid index spec: .. with no expression".to_string() },
                span
            ));
        }
        let trim_offset = text.len() - trimmed.len();
        let dotdot_span = Span::from_usize(start_offset + trim_offset, start_offset + trim_offset + 2);
        // Find position relative to trimmed text, then adjust for trim_offset to get position in text
        let expr_pos_in_trimmed = trimmed.find(expr_text).ok_or_else(|| {
            pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: format!("Could not find expression '{}' in index spec", expr_text),
                },
                span,
            )
        })?;
        // Position in original text (accounting for trim_offset)
        let expr_pos_in_text = trim_offset + expr_pos_in_trimmed;
        // Create span using positions relative to span.as_str() (which should match text)
        let expr_span = pest::Span::new(
            span.as_str(),
            expr_pos_in_text,
            expr_pos_in_text + expr_text.len(),
        ).ok_or_else(|| {
            pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: format!("Invalid span for expression '{}': pos={}, len={}, text_len={}", expr_text, expr_pos_in_text, expr_text.len(), text.len()),
                },
                span,
            )
        })?;
        let end_expr = build_cst_expression_from_text(expr_text, expr_span, id_gen)?;
        return Ok(Spanned::new(id_gen.next(), 
            Span::from_pest_span(span),
            CstIndexSpec::Range {
                start: None,
                dotdot: dotdot_span,
                end: Some(end_expr),
                step: None,
            }
        ));
    }
    if trimmed.ends_with("..") && trimmed != ".." {
        // To end: expr..
        let expr_text = trimmed[..trimmed.len()-2].trim();
        if expr_text.is_empty() {
            return Err(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError { message: "Invalid index spec: .. with no expression".to_string() },
                span
            ));
        }
        let trim_offset = text.len() - trimmed.len();
        // Find position relative to trimmed text, then adjust for trim_offset
        let expr_pos_in_trimmed = trimmed.find(expr_text).ok_or_else(|| {
            pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: format!("Could not find expression '{}' in index spec", expr_text),
                },
                span,
            )
        })?;
        let expr_pos_in_text = trim_offset + expr_pos_in_trimmed;
        // Create span using positions relative to span.as_str()
        let expr_span = pest::Span::new(
            span.as_str(),
            expr_pos_in_text,
            expr_pos_in_text + expr_text.len(),
        ).ok_or_else(|| {
            pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: format!("Invalid span for expression '{}': pos={}, len={}, text_len={}", expr_text, expr_pos_in_text, expr_text.len(), text.len()),
                },
                span,
            )
        })?;
        let start_expr = build_cst_expression_from_text(expr_text, expr_span, id_gen)?;
        // Find dotdot position in trimmed, then adjust
        let dotdot_pos_in_trimmed = trimmed.rfind("..").ok_or_else(|| {
            pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: "Could not find '..' in index spec".to_string(),
                },
                span,
            )
        })?;
        let dotdot_pos_in_text = trim_offset + dotdot_pos_in_trimmed;
        let dotdot_span = Span::from_usize(start_offset + dotdot_pos_in_text, start_offset + dotdot_pos_in_text + 2);
        return Ok(Spanned::new(id_gen.next(), 
            Span::from_pest_span(span),
            CstIndexSpec::Range {
                start: Some(start_expr),
                dotdot: dotdot_span,
                end: None,
                step: None,
            }
        ));
    }
    // Handle inclusive range: expr..=expr
    if let Some(pos) = trimmed.find("..=") {
        let start_text = trimmed[..pos].trim();
        let end_text = trimmed[pos+3..].trim();
        if start_text.is_empty() || end_text.is_empty() {
            return Err(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError { message: "Invalid inclusive range: missing start or end".to_string() },
                span
            ));
        }
        let trim_offset = text.len() - trimmed.len();
        let start_pos_in_trimmed = trimmed.find(start_text).unwrap_or(0);
        let start_pos_in_text = trim_offset + start_pos_in_trimmed;
        let start_span = pest::Span::new(
            span.as_str(),
            start_pos_in_text,
            start_pos_in_text + start_text.len(),
        ).ok_or_else(|| {
            pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: format!("Invalid span for start '{}'", start_text),
                },
                span,
            )
        })?;
        let start_expr = build_cst_expression_from_text(start_text, start_span, id_gen)?;
        let end_pos_in_trimmed = trimmed.find(end_text).unwrap_or(pos + 3);
        let end_pos_in_text = trim_offset + end_pos_in_trimmed;
        let end_span = pest::Span::new(
            span.as_str(),
            end_pos_in_text,
            end_pos_in_text + end_text.len(),
        ).ok_or_else(|| {
            pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: format!("Invalid span for end '{}'", end_text),
                },
                span,
            )
        })?;
        let end_expr = build_cst_expression_from_text(end_text, end_span, id_gen)?;
        let dotdoteq_pos_in_text = trim_offset + pos;
        let dotdoteq_span = Span::from_usize(start_offset + dotdoteq_pos_in_text, start_offset + dotdoteq_pos_in_text + 3);
        return Ok(Spanned::new(id_gen.next(), 
            Span::from_pest_span(span),
            CstIndexSpec::InclusiveRange {
                start: Some(start_expr),
                dotdoteq: dotdoteq_span,
                end: Some(end_expr),
            }
        ));
    }
    // Handle range with step: expr..expr..expr
    let mut dot_dot_positions = Vec::new();
    let chars: Vec<char> = trimmed.chars().collect();
    let mut i = 0;
    while i < chars.len().saturating_sub(1) {
        if chars[i] == '.' && chars[i+1] == '.' {
            dot_dot_positions.push(i);
            i += 2;
        } else {
            i += 1;
        }
    }
    if dot_dot_positions.len() == 2 {
        // Range with step: start..end..step
        let start_text = trimmed[..dot_dot_positions[0]].trim();
        let end_text = trimmed[dot_dot_positions[0]+2..dot_dot_positions[1]].trim();
        let step_text = trimmed[dot_dot_positions[1]+2..].trim();
        if start_text.is_empty() || end_text.is_empty() || step_text.is_empty() {
            return Err(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError { message: "Invalid range with step: missing start, end, or step".to_string() },
                span
            ));
        }
        let trim_offset = text.len() - trimmed.len();
        let start_pos_in_trimmed = trimmed.find(start_text).unwrap_or(0);
        let start_pos_in_text = trim_offset + start_pos_in_trimmed;
        let start_span = pest::Span::new(
            span.as_str(),
            start_pos_in_text,
            start_pos_in_text + start_text.len(),
        ).ok_or_else(|| {
            pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: format!("Invalid span for start '{}'", start_text),
                },
                span,
            )
        })?;
        let start_expr = build_cst_expression_from_text(start_text, start_span, id_gen)?;
        let end_pos_in_trimmed = trimmed.find(end_text).unwrap_or(dot_dot_positions[0] + 2);
        let end_pos_in_text = trim_offset + end_pos_in_trimmed;
        let end_span = pest::Span::new(
            span.as_str(),
            end_pos_in_text,
            end_pos_in_text + end_text.len(),
        ).ok_or_else(|| {
            pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: format!("Invalid span for end '{}'", end_text),
                },
                span,
            )
        })?;
        let end_expr = build_cst_expression_from_text(end_text, end_span, id_gen)?;
        let step_pos_in_trimmed = trimmed.find(step_text).unwrap_or(dot_dot_positions[1] + 2);
        let step_pos_in_text = trim_offset + step_pos_in_trimmed;
        let step_span = pest::Span::new(
            span.as_str(),
            step_pos_in_text,
            step_pos_in_text + step_text.len(),
        ).ok_or_else(|| {
            pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: format!("Invalid span for step '{}'", step_text),
                },
                span,
            )
        })?;
        let step_expr = build_cst_expression_from_text(step_text, step_span, id_gen)?;
        // First dotdot is between start and end
        let first_dotdot_pos_in_text = trim_offset + dot_dot_positions[0];
        let first_dotdot_span = Span::from_usize(start_offset + first_dotdot_pos_in_text, start_offset + first_dotdot_pos_in_text + 2);
        // Second dotdot is between end and step
        let second_dotdot_pos_in_text = trim_offset + dot_dot_positions[1];
        let second_dotdot_span = Span::from_usize(start_offset + second_dotdot_pos_in_text, start_offset + second_dotdot_pos_in_text + 2);
        return Ok(Spanned::new(id_gen.next(), 
            Span::from_pest_span(span),
            CstIndexSpec::Range {
                start: Some(start_expr),
                dotdot: first_dotdot_span,
                end: Some(end_expr),
                step: Some((second_dotdot_span, step_expr)),
            }
        ));
    } else if dot_dot_positions.len() == 1 {
        // Regular range: expr..expr
        let start_text = trimmed[..dot_dot_positions[0]].trim();
        let end_text = trimmed[dot_dot_positions[0]+2..].trim();
        if start_text.is_empty() || end_text.is_empty() {
            return Err(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError { message: "Invalid range: missing start or end".to_string() },
                span
            ));
        }
        let trim_offset = text.len() - trimmed.len();
        let start_pos_in_trimmed = trimmed.find(start_text).unwrap_or(0);
        let start_pos_in_text = trim_offset + start_pos_in_trimmed;
        let start_span = pest::Span::new(
            span.as_str(),
            start_pos_in_text,
            start_pos_in_text + start_text.len(),
        ).ok_or_else(|| {
            pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: format!("Invalid span for start '{}'", start_text),
                },
                span,
            )
        })?;
        let start_expr = build_cst_expression_from_text(start_text, start_span, id_gen)?;
        let end_pos_in_trimmed = trimmed.find(end_text).unwrap_or(dot_dot_positions[0] + 2);
        let end_pos_in_text = trim_offset + end_pos_in_trimmed;
        let end_span = pest::Span::new(
            span.as_str(),
            end_pos_in_text,
            end_pos_in_text + end_text.len(),
        ).ok_or_else(|| {
            pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: format!("Invalid span for end '{}'", end_text),
                },
                span,
            )
        })?;
        let end_expr = build_cst_expression_from_text(end_text, end_span, id_gen)?;
        let dotdot_pos_in_text = trim_offset + dot_dot_positions[0];
        let dotdot_span = Span::from_usize(start_offset + dotdot_pos_in_text, start_offset + dotdot_pos_in_text + 2);
        return Ok(Spanned::new(id_gen.next(), 
            Span::from_pest_span(span),
            CstIndexSpec::Range {
                start: Some(start_expr),
                dotdot: dotdot_span,
                end: Some(end_expr),
                step: None,
            }
        ));
    }
    // Single index: just an expression
    let expr = build_cst_expression_from_text(trimmed, span, id_gen)?;
    Ok(Spanned::new(id_gen.next(), 
        Span::from_pest_span(span),
        CstIndexSpec::Single(expr)
    ))
}

/// Parse loop init variables with spans.
/// Returns Vec<(var_name, eq_span, expression)>
fn parse_cst_loop_init_vars(
    init_text: &str,
    span: Span,
    parent_span: pest::Span,
    id_gen: &mut CstIdGenerator,
) -> Result<Vec<(Spanned<String>, Span, Spanned<CstExpr>)>, pest::error::Error<Rule>> {
    if init_text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let mut init_parse_result = CantaLoopParser::parse(Rule::loop_init, init_text)
        .map_err(|e| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: format!("Failed to parse loop init vars: {:?}", e) },
            parent_span
        ))?;
    let mut init_vars = Vec::new();
    if let Some(init_pair) = init_parse_result.next() {
        for init_var_pair in init_pair.into_inner() {
            if init_var_pair.as_rule() == Rule::loop_init_var {
                let init_var_span = init_var_pair.as_span();
                let init_var_text = init_var_pair.as_str();
                let start_offset = span.start as usize + init_var_text.as_ptr() as usize - init_text.as_ptr() as usize;
                let eq_pos = init_var_text.find('=')
                    .ok_or_else(|| pest::error::Error::new_from_span(
                        pest::error::ErrorVariant::CustomError { message: "Loop init var missing '='".to_string() },
                        init_var_span
                    ))?;
                let identifier = init_var_text[..eq_pos].trim().to_string();
                let expr_text = init_var_text[eq_pos + 1..].trim();
                let id_pos = init_var_text.find(&identifier).unwrap_or(0);
                let id_span = Span::from_usize(span.start as usize + start_offset + id_pos, span.start as usize + start_offset + id_pos + identifier.len());
                let eq_span = Span::from_usize(span.start as usize + start_offset + eq_pos, span.start as usize + start_offset + eq_pos + 1);
                let expr_span = safe_pest_span(
                    parent_span.as_str(),
                    parent_span.start() + start_offset + eq_pos + 1,
                    parent_span.start() + start_offset + init_var_text.len(),
                    parent_span,
                )?;
                let expression = build_cst_expression_from_text(expr_text, expr_span, id_gen)?;
                init_vars.push((
                    Spanned::new(id_gen.next(), id_span, identifier),
                    eq_span,
                    expression,
                ));
            }
        }
    }
    Ok(init_vars)
}

/// Build a CST loop expression.
fn build_cst_loop_expression(pair: Pair<Rule>, id_gen: &mut CstIdGenerator) -> Result<Spanned<CstExpr>, pest::error::Error<Rule>> {
    let span = pair.as_span();
    let text = pair.as_str();
    let start_offset = span.start();
    if !text.starts_with("loop") {
        return Err(pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError {
                message: format!("Loop expression must start with 'loop', got: {}", text.chars().take(20).collect::<String>())
            },
            span
        ));
    }
    // Find opening brace
    let brace_start = text.find('{')
        .ok_or_else(|| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "Loop expression missing opening brace".to_string() },
            span
        ))?;
    let loop_keyword = Span::from_usize(start_offset, start_offset + 4);
    // Parse optional init vars (everything between "loop" and "{")
    let init_text = text[4..brace_start].trim();
    let init_vars = if init_text.is_empty() {
        Vec::new()
    } else {
        parse_cst_loop_init_vars(init_text, Span::from_usize(start_offset + 4, start_offset + brace_start), span, id_gen)?
    };
    // Extract block content
    let mut brace_count = 0;
    let mut found_start = false;
    let mut brace_end = None;
    for (i, ch) in text[brace_start..].char_indices() {
        if ch == '{' {
            brace_count += 1;
            found_start = true;
        } else if ch == '}' {
            brace_count -= 1;
            if found_start && brace_count == 0 {
                brace_end = Some(brace_start + i);
                break;
            }
        }
    }
    let brace_end = brace_end.ok_or_else(|| pest::error::Error::new_from_span(
        pest::error::ErrorVariant::CustomError { message: "Loop expression missing closing brace".to_string() },
        span
    ))?;
    let block_content = &text[brace_start + 1..brace_end];
    let block_text = format!("{{{}}}", block_content);
    
    // CRITICAL FIX: Clone the string to ensure independent parse
    let isolated_block_text = block_text.clone();
    let mut block_parse_result = CantaLoopParser::parse(Rule::braced_block, &isolated_block_text)
        .map_err(|e| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: format!("Failed to parse loop block: {}", e) },
            span
        ))?;
    
    let block_inner = if let Some(block_pair) = block_parse_result.next() {
        let mut block_inner_iter = block_pair.into_inner();
        block_inner_iter
            .find(|p| p.as_rule() == Rule::block)
            .ok_or_else(|| pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError { message: "Loop block missing inner block".to_string() },
                span
            ))?
    } else {
        return Err(pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "Loop block missing block".to_string() },
            span
        ));
    };
    let (body, _) = build_cst_block(block_inner, id_gen)?;
    Ok(Spanned::new(id_gen.next(), 
        Span::from_pest_span(span),
        CstExpr::Loop {
            loop_keyword,
            init_vars,
            body,
        }
    ))
}

