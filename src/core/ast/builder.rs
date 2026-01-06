use pest::iterators::Pair;

use crate::core::parser::{Rule, PRATT_PARSER, CantaLoopParser};
use pest::Parser;
use crate::core::ast::{Expression, Statement, Program, Block, Literal, UnaryOp, BinaryOp, PostfixOp, CallArgument, ImportSelector, IndexSpec};

// ============================================================================
// Error Helpers
// ============================================================================

/// Creates a custom error with a message at the given span
fn error_at_span(span: pest::Span, message: String) -> pest::error::Error<Rule> {
    pest::error::Error::new_from_span(
        pest::error::ErrorVariant::CustomError { message },
        span
    )
}

/// Creates an error for a missing keyword
fn error_missing_keyword(span: pest::Span, keyword: &str) -> pest::error::Error<Rule> {
    error_at_span(span, format!("Missing '{}' keyword", keyword))
}

// ============================================================================
// Text Extraction Helpers
// ============================================================================

/// Finds a keyword in text and returns its position, or an error if not found
fn find_keyword(text: &str, keyword: &str, span: pest::Span) -> Result<usize, pest::error::Error<Rule>> {
    text.find(keyword).ok_or_else(|| error_missing_keyword(span, keyword))
}

/// Extracts an identifier after a keyword, ending at whitespace or a delimiter
fn extract_identifier_after_keyword(
    text: &str,
    keyword: &str,
    span: pest::Span,
    delimiters: &[char],
) -> Result<String, pest::error::Error<Rule>> {
    let start = find_keyword(text, keyword, span)? + keyword.len();
    let identifier_end = text[start..]
        .find(|c: char| c.is_whitespace() || delimiters.contains(&c))
        .ok_or_else(|| error_at_span(span, format!("Missing identifier after '{}'", keyword)))?;
    Ok(text[start..start + identifier_end].trim().to_string())
}

/// Finds the matching closing brace for an opening brace at the given position
fn find_matching_brace(text: &str, brace_start: usize) -> Option<usize> {
    let mut brace_count = 0;
    let mut found_start = false;
    
    for (i, ch) in text[brace_start..].char_indices() {
        match ch {
            '{' => {
                brace_count += 1;
                found_start = true;
            }
            '}' if found_start => {
                brace_count -= 1;
                if brace_count == 0 {
                    return Some(brace_start + i + 1);
                }
            }
            _ => {}
        }
    }
    None
}

/// Finds the opening brace and returns its position, or an error if not found
fn find_opening_brace(text: &str, span: pest::Span, context: &str) -> Result<usize, pest::error::Error<Rule>> {
    text.find('{').ok_or_else(|| {
        error_at_span(span, format!("{} missing opening brace", context))
    })
}

/// Extracts text between braces, handling nested braces
fn extract_braced_content<'a>(text: &'a str, brace_start: usize, span: pest::Span) -> Result<&'a str, pest::error::Error<Rule>> {
    let brace_end = find_matching_brace(text, brace_start)
        .ok_or_else(|| error_at_span(span, "Missing matching closing brace".to_string()))?;
    Ok(&text[brace_start + 1..brace_end - 1])
}

// ============================================================================
// Type Annotation Helpers
// ============================================================================

/// Builds a type annotation string from a Pair<Rule>
fn build_type_annotation(pair: Pair<Rule>) -> Result<String, pest::error::Error<Rule>> {
    Ok(pair.as_str().trim().to_string())
}

/// Parses a type annotation from text
fn parse_type_annotation_from_text(
    type_text: &str,
    _span: pest::Span,
) -> Result<Option<String>, pest::error::Error<Rule>> {
    if type_text.trim().is_empty() {
        return Ok(None);
    }
    
    if let Ok(mut type_pairs) = CantaLoopParser::parse(Rule::type_annotation, type_text) {
        if let Some(type_pair) = type_pairs.next() {
            Ok(Some(build_type_annotation(type_pair)?))
        } else {
            Ok(None)
        }
    } else {
        // Fallback: use text as-is
        Ok(Some(type_text.to_string()))
    }
}

/// Extracts return type annotation from text after "->"
fn extract_return_type(text: &str, span: pest::Span) -> Result<Option<String>, pest::error::Error<Rule>> {
    if !text.trim_start().starts_with("->") {
        return Ok(None);
    }
    
    let arrow_pos = text.find("->").unwrap();
    let after_arrow = text[arrow_pos + 2..].trim_start();
    let brace_pos = after_arrow.find('{').unwrap_or(after_arrow.len());
    let type_text = after_arrow[..brace_pos].trim();
    
    parse_type_annotation_from_text(type_text, span)
}

/// Extracts return type annotation from closure text after "->" (handles both => and { delimiters)
/// Returns the type string and the byte offset where the body starts (after the return type)
fn extract_closure_return_type(text: &str, span: pest::Span) -> Result<(Option<String>, usize), pest::error::Error<Rule>> {
    let trimmed = text.trim_start();
    let trim_offset = text.len() - trimmed.len();
    
    if !trimmed.starts_with("->") {
        return Ok((None, 0));
    }
    
    let arrow_pos = trimmed.find("->").unwrap();
    let after_arrow_raw = &trimmed[arrow_pos + 2..];
    let after_arrow = after_arrow_raw.trim_start();
    let after_arrow_trim_offset = after_arrow_raw.len() - after_arrow.len();
    
    // Find the position of => or {, whichever comes first
    let arrow_arrow_pos = after_arrow.find("=>").unwrap_or(after_arrow.len());
    let brace_pos = after_arrow.find('{').unwrap_or(after_arrow.len());
    let delimiter_pos = arrow_arrow_pos.min(brace_pos);
    
    let type_text = after_arrow[..delimiter_pos].trim();
    let return_type = parse_type_annotation_from_text(type_text, span)?;
    
    // Calculate the offset where the body starts (after "-> type " or "-> type=>" or "-> type{")
    // This is: trim_offset + arrow_pos + 2 (for "->") + after_arrow_trim_offset + delimiter_pos
    let body_start_offset = trim_offset + arrow_pos + 2 + after_arrow_trim_offset + delimiter_pos;
    
    Ok((return_type, body_start_offset))
}

// ============================================================================
// Block Parsing Helpers
// ============================================================================

/// Parses a block from braced_block text
fn parse_block_from_braced_block_text(
    block_text: &str,
    span: pest::Span,
    context: &str,
) -> Result<Block, pest::error::Error<Rule>> {
    let mut parse_result = CantaLoopParser::parse(Rule::braced_block, block_text)
        .map_err(|e| error_at_span(span, format!("Failed to parse {}: {:?}", context, e)))?;
    
    let body_pair = parse_result.next().ok_or_else(|| {
        error_at_span(span, format!("{} missing body", context))
    })?;
    
    let block_pair = body_pair.into_inner()
        .find(|p| p.as_rule() == Rule::block)
        .ok_or_else(|| {
            error_at_span(span, format!("{} missing block", context))
        })?;
    
    build_block(block_pair)
}

/// Parses a block directly from block text
fn parse_block_from_text(
    block_text: &str,
    span: pest::Span,
) -> Result<Block, pest::error::Error<Rule>> {
    let mut parse_result = CantaLoopParser::parse(Rule::block, block_text)
        .map_err(|e| error_at_span(span, format!("Failed to parse block: {:?}", e)))?;
    
    if let Some(block_pair) = parse_result.next() {
        build_block(block_pair)
    } else {
        Ok(Block { statements: Vec::new() })
    }
}

// ============================================================================
// Loop Init Var Parsing
// ============================================================================

/// Parses a single loop init variable from text (format: "identifier = expression")
fn parse_loop_init_var(
    init_var_text: &str,
    init_var_span: pest::Span,
) -> Result<(String, Expression), pest::error::Error<Rule>> {
    let eq_pos = init_var_text.find('=')
        .ok_or_else(|| error_at_span(init_var_span, "Loop init var missing '='".to_string()))?;
    let identifier = init_var_text[..eq_pos].trim().to_string();
    let expr_text = init_var_text[eq_pos + 1..].trim();
    let expression = parse_expression_from_text(expr_text, init_var_span)?;
    Ok((identifier, expression))
}

/// Parses loop init variables from text
fn parse_loop_init_vars(
    init_text: &str,
    span: pest::Span,
) -> Result<Vec<(String, Expression)>, pest::error::Error<Rule>> {
    if init_text.trim().is_empty() {
        return Ok(Vec::new());
    }
    
    let mut init_parse_result = CantaLoopParser::parse(Rule::loop_init, init_text)
        .map_err(|e| error_at_span(span, format!("Failed to parse loop init vars: {:?}", e)))?;
    
    let mut init_vars = Vec::new();
    if let Some(init_pair) = init_parse_result.next() {
        for init_var_pair in init_pair.into_inner() {
            if init_var_pair.as_rule() == Rule::loop_init_var {
                let (id, expr) = parse_loop_init_var(init_var_pair.as_str(), init_var_pair.as_span())?;
                init_vars.push((id, expr));
            }
        }
    }
    Ok(init_vars)
}

// ============================================================================
// Expression Extraction Helpers
// ============================================================================

/// Extracts expression text after a keyword
fn extract_expression_after_keyword<'a>(
    text: &'a str,
    keyword: &str,
    span: pest::Span,
) -> Result<&'a str, pest::error::Error<Rule>> {
    let keyword_pos = find_keyword(text, keyword, span)?;
    let expr_start = keyword_pos + keyword.len();
    Ok(text[expr_start..].trim())
}

/// Extracts expression text between a keyword and a delimiter
#[allow(dead_code)]
fn extract_expression_between<'a>(
    text: &'a str,
    after_keyword: usize,
    before_delimiter: usize,
    span: pest::Span,
    context: &str,
) -> Result<&'a str, pest::error::Error<Rule>> {
    let expr_text = text[after_keyword..before_delimiter].trim();
    if expr_text.is_empty() {
        return Err(error_at_span(span, format!("{} missing expression", context)));
    }
    Ok(expr_text)
}

/// Finds a keyword as a word boundary (not part of another word)
fn find_word_boundary(text: &str, keyword: &str) -> Option<usize> {
    for i in 0..text.len() {
        if text[i..].starts_with(keyword) {
            // Check if it's a word boundary
            let after_keyword = if i + keyword.len() < text.len() {
                text.chars().nth(i + keyword.len())
            } else {
                None
            };
            if after_keyword.is_none() || !after_keyword.unwrap().is_alphanumeric() {
                return Some(i);
            }
        }
    }
    None
}

pub fn build_program(pair: Pair<Rule>) -> Result<Program, pest::error::Error<Rule>> {
    let mut blocks: Vec<Block> = Vec::new();

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::block => {
                blocks.push(build_block(inner)?);
            }
            Rule::EOI => {}
            _ => unreachable!("unexpected rule: {:?}", inner.as_rule()),
        }
    }

    Ok(Program { blocks })
}

fn build_block (pair: Pair<Rule>) -> Result<Block, pest::error::Error<Rule>> {
    let mut statements: Vec<Statement> = Vec::new();

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::statement_with_semicolon => {
                // Pass the wrapper rule to build_statement, which will extract the inner statement
                statements.push(build_statement(inner)?);
            },
            Rule::statement_without_semicolon => {
                // Pass the wrapper rule to build_statement, which will extract the inner statement
                statements.push(build_statement(inner)?);
            },
            Rule::statement => {
                statements.push(build_statement(inner)?);
            },
            _ => {
                // Ignore semicolons and whitespace
            }

        }
    }

    Ok(Block { statements })
}

fn build_function_declaration(pair: Pair<Rule>) -> Result<Statement, pest::error::Error<Rule>> {
    use crate::core::ast::Statement;

    let span = pair.as_span();
    let text = pair.as_str();
    
    // Check for pub visibility
    let pub_visibility = text.trim_start().starts_with("pub");
    let fn_keyword = if pub_visibility { "pub fn " } else { "fn " };
    
    // Extract identifier after "fn " or "pub fn "
    let identifier = extract_identifier_after_keyword(text, fn_keyword, span, &['('])?;
    
    // Find closing paren
    let paren_end = text.find(')')
        .ok_or_else(|| error_at_span(span, "Function missing closing paren".to_string()))? + 1;
    
    // Extract return type and find brace
    let after_paren = &text[paren_end..];
    let return_type = extract_return_type(after_paren, span)?;
    
    // Find opening brace
    let brace_start_offset = find_opening_brace(after_paren, span, "Function")?;
    let brace_start = paren_end + brace_start_offset;
    
    // Extract block content
    let block_content = extract_braced_content(text, brace_start, span)?;
    let body_block = parse_block_from_braced_block_text(
        &format!("{{{}}}", block_content),
        span,
        "Function body",
    )?;
    
    // Parse function arguments
    let args_start = text.find('(')
        .ok_or_else(|| error_at_span(span, "Function missing opening paren".to_string()))? + 1;
    let args_text = text[args_start..paren_end - 1].trim();
    
    let arguments = if args_text.is_empty() {
        Vec::new()
    } else {
        parse_function_arguments(args_text, span)?
    };

    Ok(Statement::FunctionDeclaration {
        identifier,
        arguments,
        return_type,
        body: body_block,
        pub_visibility,
    })
}

/// Parses function arguments from text
fn parse_function_arguments(
    args_text: &str,
    span: pest::Span,
) -> Result<Vec<crate::core::ast::Argument>, pest::error::Error<Rule>> {
    use crate::core::ast::Argument;
    
    let mut args_pairs = CantaLoopParser::parse(Rule::function_args, args_text)
        .map_err(|e| error_at_span(span, format!("Failed to parse function arguments: {}", e)))?;
    
    let mut arguments = Vec::new();
    if let Some(args_pair) = args_pairs.next() {
        for arg_pair in args_pair.into_inner() {
            if arg_pair.as_rule() == Rule::argument {
                let arg_span = arg_pair.as_span();
                let mut arg_inner = arg_pair.into_inner();
                let id = arg_inner.next()
                    .ok_or_else(|| error_at_span(arg_span, "Function argument missing identifier".to_string()))?
                    .as_str()
                    .to_string();
                let type_pair = arg_inner.next()
                    .ok_or_else(|| error_at_span(arg_span, "Function argument missing type annotation".to_string()))?;
                let kind = build_type_annotation(type_pair)?;
                arguments.push(Argument { identifier: id, kind });
            }
        }
    }
    Ok(arguments)
}

/// Parses closure arguments from text (supports identifiers and _ placeholders)
fn parse_closure_arguments(
    args_text: &str,
    span: pest::Span,
) -> Result<Vec<crate::core::ast::Argument>, pest::error::Error<Rule>> {
    use crate::core::ast::Argument;
    
    let mut args_pairs = CantaLoopParser::parse(Rule::closure_args, args_text)
        .map_err(|e| error_at_span(span, format!("Failed to parse closure arguments: {}", e)))?;
    
    let mut arguments = Vec::new();
    if let Some(args_pair) = args_pairs.next() {
        for arg_pair in args_pair.into_inner() {
            if arg_pair.as_rule() == Rule::closure_arg {
                let arg_span = arg_pair.as_span();
                let mut arg_inner = arg_pair.into_inner();
                
                // Get identifier or placeholder
                let first = arg_inner.next()
                    .ok_or_else(|| error_at_span(arg_span, "Closure argument missing identifier or placeholder".to_string()))?;
                
                let id = match first.as_rule() {
                    Rule::identifier => first.as_str().to_string(),
                    Rule::placeholder => "?".to_string(), // Use "?" as the identifier name for placeholders
                    _ => return Err(error_at_span(arg_span, "Expected identifier or placeholder".to_string())),
                };
                
                // Get optional type annotation
                let kind = if let Some(type_pair) = arg_inner.next() {
                    build_type_annotation(type_pair)?
                } else {
                    "".to_string() // No type annotation
                };
                
                arguments.push(Argument { identifier: id, kind });
            }
        }
    }
    Ok(arguments)
}

fn build_identifier_expr(pair: Pair<Rule>) -> Result<Expression, pest::error::Error<Rule>> {
    let identifier = pair.as_str().to_string();
    // Keywords are prevented by grammar, but check here as a safety measure
    const KEYWORDS: &[&str] = &["fn", "if", "else", "elseif", "match", "return", "let", "true", "false"];
    if KEYWORDS.contains(&identifier.as_str()) {
        return Err(pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { 
                message: format!("'{}' is a keyword and cannot be used as an identifier", identifier)
            },
            pair.as_span(),
        ));
    }
    Ok(Expression::Identifier(identifier))
}

fn build_number(pair: Pair<Rule>) -> Result<Expression, pest::error::Error<Rule>> {
    let value = pair.as_str().trim().parse::<f64>()
        .map_err(|e| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: format!("Failed to parse number: {}", e) },
            pair.as_span()
        ))?;

    Ok(Expression::Literal(Literal::Number(value)))
}

fn build_string(pair: Pair<Rule>) -> Result<Expression, pest::error::Error<Rule>> {
    let str_with_quotes = pair.as_str();

    // Strip the surrounding quotes
    let string_value = str_with_quotes[1..str_with_quotes.len() - 1].to_string();

    Ok(Expression::Literal(Literal::String(string_value)))
}

fn build_boolean(pair: Pair<Rule>) -> Result<Expression, pest::error::Error<Rule>> {
    let value = pair.as_str().trim().parse::<bool>()
        .map_err(|e| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: format!("Failed to parse boolean: {}", e) },
            pair.as_span()
        ))?;

    Ok(Expression::Literal(Literal::Boolean(value)))
}

fn build_array_literal(pair: Pair<Rule>) -> Result<Expression, pest::error::Error<Rule>> {
    let span = pair.as_span();
    let text = pair.as_str();
    
    // Find opening bracket
    let bracket_start = text.find('[')
        .ok_or_else(|| error_at_span(span, "Array literal missing opening bracket".to_string()))?;
    
    // Find closing bracket
    let bracket_end = text.rfind(']')
        .ok_or_else(|| error_at_span(span, "Array literal missing closing bracket".to_string()))?;
    
    // Extract and parse elements
    let elements_text = text[bracket_start + 1..bracket_end].trim();
    let elements = if elements_text.is_empty() {
        Vec::new()
    } else {
        // Parse expression list
        parse_expression_list_from_text(elements_text, span)?
    };
    
    Ok(Expression::Array(elements))
}

fn build_value(pair: Pair<Rule>) -> Result<Expression, pest::error::Error<Rule>> {
    let mut inner = pair.into_inner();
    let inner_pair = inner.next().unwrap();
    match inner_pair.as_rule() {
        Rule::number => build_number(inner_pair),
        Rule::string => build_string(inner_pair),
        Rule::boolean => build_boolean(inner_pair),
        Rule::array_literal => build_array_literal(inner_pair),
        _ => unreachable!(),
    }
}

fn build_call_expression(pair: Pair<Rule>) -> Result<Expression, pest::error::Error<Rule>> {
    let span = pair.as_span();
    let text = pair.as_str();
    
    // Find opening paren
    let paren_start = text.find('(')
        .ok_or_else(|| error_at_span(span, "Call expression missing opening paren".to_string()))?;
    let identifier = text[..paren_start].trim().to_string();
    
    // Find closing paren
    let paren_end = text.rfind(')')
        .ok_or_else(|| error_at_span(span, "Call expression missing closing paren".to_string()))?;
    
    // Extract and parse arguments
    let args_text = text[paren_start + 1..paren_end].trim();
    let call_args = if args_text.is_empty() {
        Vec::new()
    } else {
        parse_call_argument_list_from_text(args_text, span)?
    };
    
    // Convert to appropriate call type
    if call_args.iter().any(|arg| matches!(arg, CallArgument::Hole)) {
        Ok(Expression::PartialCall {
            func: Box::new(Expression::Identifier(identifier)),
            args: call_args,
        })
    } else {
        let arguments: Vec<Expression> = call_args.into_iter()
            .map(|arg| match arg {
                CallArgument::Expr(expr) => expr,
                CallArgument::Hole => unreachable!("No holes should exist here"),
            })
            .collect();
        Ok(Expression::FunctionCall {
            callee: Box::new(Expression::Identifier(identifier)),
            arguments,
        })
    }
}

// Helper function to parse a call argument list from text (supports expressions and holes)
fn parse_call_argument_list_from_text(text: &str, span: pest::Span) -> Result<Vec<CallArgument>, pest::error::Error<Rule>> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    
    // Split by commas, being careful about commas inside nested structures (parens, brackets, braces, strings)
    let mut arguments = Vec::new();
    let mut current_start = 0;
    let mut depth = 0; // Track depth of nested structures
    let mut in_string = false;
    let mut escape_next = false;
    let chars: Vec<char> = text.chars().collect();
    
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
                let arg_text = text[current_start..i].trim();
                if !arg_text.is_empty() {
                    arguments.push(parse_call_argument_from_text(arg_text, span)?);
                }
                current_start = i + 1;
            }
            _ => {}
        }
    }
    
    // Parse the last argument
    let arg_text = text[current_start..].trim();
    if !arg_text.is_empty() {
        arguments.push(parse_call_argument_from_text(arg_text, span)?);
    }
    
    Ok(arguments)
}

// Helper function to parse a single call argument (expression or hole)
fn parse_call_argument_from_text(text: &str, span: pest::Span) -> Result<CallArgument, pest::error::Error<Rule>> {
    let trimmed = text.trim();
    if trimmed == "?" {
        Ok(CallArgument::Hole)
    } else {
        Ok(CallArgument::Expr(parse_expression_from_text(trimmed, span)?))
    }
}

// Helper function to parse an expression list from text
// Since expression is a silent rule, we need to manually split on commas and parse each expression
fn parse_expression_list_from_text(text: &str, span: pest::Span) -> Result<Vec<Expression>, pest::error::Error<Rule>> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    
    // Split by commas, being careful about commas inside nested structures (parens, brackets, braces, strings)
    let mut expressions = Vec::new();
    let mut current_start = 0;
    let mut depth = 0; // Track depth of nested structures
    let mut in_string = false;
    let mut escape_next = false;
    let chars: Vec<char> = text.chars().collect();
    
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
                    expressions.push(parse_expression_from_text(expr_text, span)?);
                }
                current_start = i + 1;
            }
            _ => {}
        }
    }
    
    // Parse the last expression
    let expr_text = text[current_start..].trim();
    if !expr_text.is_empty() {
        expressions.push(parse_expression_from_text(expr_text, span)?);
    }
    
    Ok(expressions)
}

// Helper function to parse an index spec list from text (supports multi-dimensional indexing)
fn parse_index_spec_list_from_text(text: &str, span: pest::Span) -> Result<Vec<IndexSpec>, pest::error::Error<Rule>> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    
    // Split by commas, being careful about commas inside nested structures (parens, brackets, braces, strings)
    let mut specs = Vec::new();
    let mut current_start = 0;
    let mut depth = 0; // Track depth of nested structures
    let mut in_string = false;
    let mut escape_next = false;
    let chars: Vec<char> = text.chars().collect();
    
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
                    specs.push(parse_index_spec_from_text(spec_text, span)?);
                }
                current_start = i + 1;
            }
            _ => {}
        }
    }
    
    // Parse the last spec
    let spec_text = text[current_start..].trim();
    if !spec_text.is_empty() {
        specs.push(parse_index_spec_from_text(spec_text, span)?);
    }
    
    Ok(specs)
}

// Helper function to parse a single index spec from text
fn parse_index_spec_from_text(text: &str, span: pest::Span) -> Result<IndexSpec, pest::error::Error<Rule>> {
    let trimmed = text.trim();
    
    // Handle full range: ..
    if trimmed == ".." {
        return Ok(IndexSpec::Range {
            start: None,
            end: None,
            step: None,
        });
    }
    
    // Handle partial ranges: ..expr or expr..
    if trimmed.starts_with("..") && trimmed != ".." {
        // From start: ..expr
        let expr_text = trimmed[2..].trim();
        if expr_text.is_empty() {
            return Err(error_at_span(span, "Invalid index spec: .. with no expression".to_string()));
        }
        let end_expr = parse_expression_from_text(expr_text, span)?;
        return Ok(IndexSpec::Range {
            start: None,
            end: Some(end_expr),
            step: None,
        });
    }
    
    if trimmed.ends_with("..") && trimmed != ".." {
        // To end: expr..
        let expr_text = trimmed[..trimmed.len()-2].trim();
        if expr_text.is_empty() {
            return Err(error_at_span(span, "Invalid index spec: .. with no expression".to_string()));
        }
        let start_expr = parse_expression_from_text(expr_text, span)?;
        return Ok(IndexSpec::Range {
            start: Some(start_expr),
            end: None,
            step: None,
        });
    }
    
    // Handle inclusive range: expr..=expr
    if let Some(pos) = trimmed.find("..=") {
        let start_text = trimmed[..pos].trim();
        let end_text = trimmed[pos+3..].trim();
        if start_text.is_empty() || end_text.is_empty() {
            return Err(error_at_span(span, "Invalid inclusive range: missing start or end".to_string()));
        }
        let start_expr = parse_expression_from_text(start_text, span)?;
        let end_expr = parse_expression_from_text(end_text, span)?;
        return Ok(IndexSpec::InclusiveRange {
            start: Some(start_expr),
            end: Some(end_expr),
        });
    }
    
    // Handle range with step: expr..expr..expr
    // Check for two ".." separators
    let mut dot_dot_positions = Vec::new();
    let chars: Vec<char> = trimmed.chars().collect();
    let mut i = 0;
    while i < chars.len().saturating_sub(1) {
        if chars[i] == '.' && chars[i+1] == '.' {
            dot_dot_positions.push(i);
            i += 2; // Skip both dots
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
            return Err(error_at_span(span, "Invalid range with step: missing start, end, or step".to_string()));
        }
        let start_expr = parse_expression_from_text(start_text, span)?;
        let end_expr = parse_expression_from_text(end_text, span)?;
        let step_expr = parse_expression_from_text(step_text, span)?;
        return Ok(IndexSpec::Range {
            start: Some(start_expr),
            end: Some(end_expr),
            step: Some(step_expr),
        });
    } else if dot_dot_positions.len() == 1 {
        // Regular range: expr..expr
        let start_text = trimmed[..dot_dot_positions[0]].trim();
        let end_text = trimmed[dot_dot_positions[0]+2..].trim();
        if start_text.is_empty() || end_text.is_empty() {
            return Err(error_at_span(span, "Invalid range: missing start or end".to_string()));
        }
        let start_expr = parse_expression_from_text(start_text, span)?;
        let end_expr = parse_expression_from_text(end_text, span)?;
        return Ok(IndexSpec::Range {
            start: Some(start_expr),
            end: Some(end_expr),
            step: None,
        });
    }
    
    // Single index: just an expression
    let expr = parse_expression_from_text(trimmed, span)?;
    Ok(IndexSpec::Single(expr))
}

// Helper function to parse a loop expression from text
// This is used when loop expressions need to be parsed from text directly
#[allow(dead_code)]
fn parse_loop_expression_from_text(text: &str, span: pest::Span) -> Result<Expression, pest::error::Error<Rule>> {
    // Parse the loop expression using the grammar rule
    let mut parse_result = CantaLoopParser::parse(Rule::loop_expression, text)
        .map_err(|e| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { 
                message: format!("Failed to parse loop expression: {:?}", e)
            },
            span
        ))?;
    
    // Extract the loop_expression pair and build it
    if let Some(loop_pair) = parse_result.next() {
        build_loop_expression(loop_pair)
    } else {
        Err(pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { 
                message: "Failed to parse loop expression: no loop_expression found".to_string()
            },
            span
        ))
    }
}

fn build_loop_expression(pair: Pair<Rule>) -> Result<Expression, pest::error::Error<Rule>> {
    let span = pair.as_span();
    let full_text = span.as_str();
    
    if !full_text.starts_with("loop") {
        return Err(error_at_span(
            span,
            format!("Loop expression must start with 'loop', got: {}", 
                full_text.chars().take(20).collect::<String>())
        ));
    }
    
    // Find opening brace
    let brace_start = find_opening_brace(full_text, span, "Loop expression")?;
    
    // Parse optional init vars (everything between "loop" and "{")
    let init_text = full_text[4..brace_start].trim();
    let init_vars = parse_loop_init_vars(init_text, span)?;
    
    // Extract block content
    let block_content = extract_braced_content(full_text, brace_start, span)?;
    let body_block = parse_block_from_text(block_content, span)?;
    
    Ok(Expression::Loop {
        init_vars,
        body: body_block,
    })
}

fn build_closure_expression(pair: Pair<Rule>) -> Result<Expression, pest::error::Error<Rule>> {
    use crate::core::ast::ClosureBody;
    
    let span = pair.as_span();
    let full_text = span.as_str();
    
    if !full_text.starts_with("fn") {
        return Err(error_at_span(
            span,
            format!("Closure expression must start with 'fn', got: {}", 
                full_text.chars().take(20).collect::<String>())
        ));
    }
    
    // Find opening paren
    let paren_start = full_text.find('(')
        .ok_or_else(|| error_at_span(span, "Closure missing opening paren".to_string()))?;
    
    // Find closing paren
    let paren_end = full_text[paren_start..].find(')')
        .ok_or_else(|| error_at_span(span, "Closure missing closing paren".to_string()))? + paren_start + 1;
    
    // Parse arguments (supports both identifiers and _ placeholders)
    let args_text = full_text[paren_start + 1..paren_end - 1].trim();
    let arguments = if args_text.is_empty() {
        Vec::new()
    } else {
        parse_closure_arguments(args_text, span)?
    };
    
    // Extract return type annotation if present
    let after_paren = &full_text[paren_end..];
    let (return_type, body_start_offset) = extract_closure_return_type(after_paren, span)?;
    let body_text_start = &after_paren[body_start_offset..];
    
    // Check if there's an arrow or if it goes directly to a block
    let body = if let Some(arrow_pos) = body_text_start.find("=>") {
        // Has arrow: fn(args) => body or fn(args) => { body } or fn(args) -> type => body
        let body_text = body_text_start[arrow_pos + 2..].trim();
        
        if body_text.starts_with('{') {
            // Block body with arrow
            // Find the actual '{' position in full_text (accounting for trimmed whitespace)
            let body_start_in_body_text = arrow_pos + 2;
            let whitespace_skip = body_text_start[body_start_in_body_text..].len() - body_text.len();
            let block_start = paren_end + body_start_offset + body_start_in_body_text + whitespace_skip;
            let block_content = extract_braced_content(full_text, block_start, span)?;
            let body_block = parse_block_from_text(block_content, span)?;
            ClosureBody::Block(body_block)
        } else {
            // Expression body with arrow
            let body_expr = parse_expression_from_text(body_text, span)?;
            ClosureBody::Expression(Box::new(body_expr))
        }
    } else if body_text_start.trim().starts_with('{') {
        // No arrow, but has block: fn(args) { body } or fn(args) -> type { body }
        let block_start = paren_end + body_start_offset + body_text_start.find('{').unwrap();
        let block_content = extract_braced_content(full_text, block_start, span)?;
        let body_block = parse_block_from_text(block_content, span)?;
        ClosureBody::Block(body_block)
    } else {
        return Err(error_at_span(span, "Closure must have either '=>' followed by an expression/block, or a block body without arrow".to_string()));
    };
    
    Ok(Expression::Closure {
        arguments,
        return_type,
        body,
    })
}

fn build_primary(primary_pair: Pair<Rule>) -> Result<Expression, pest::error::Error<Rule>> {
    match primary_pair.as_rule() {
        Rule::primary => {
            // Get text and span before consuming primary_pair
            let text = primary_pair.as_str().trim();
            let span = primary_pair.as_span();
            
            // Primary rule contains the base expression
            let mut inner = primary_pair.into_inner();
            let base_pair = inner.next().unwrap();
            
            match base_pair.as_rule() {
                Rule::value => build_value(base_pair),
                Rule::array_literal => build_array_literal(base_pair),
                Rule::expression => Ok(Expression::Group(Box::new(build_expression(base_pair)?))),
                Rule::identifier => build_identifier_expr(base_pair),
                Rule::loop_expression => build_loop_expression(base_pair),
                Rule::closure_expression | Rule::atomic_closure_expression => build_closure_expression(base_pair),
                Rule::member_access => {
                    let span = base_pair.as_span();
                    let mut member_inner = base_pair.into_inner();
                    let first = member_inner.next().unwrap();
                    let mut identifiers = vec![first.as_str().to_string()];
                    for id_pair in member_inner {
                        identifiers.push(id_pair.as_str().to_string());
                    }
                    // Build member access: utils.add becomes MemberAccess(Identifier("utils"), "add")
                    // For field access like p.x, we'll use FieldAccess instead
                    if identifiers.len() == 2 {
                        // Single field access: p.x -> FieldAccess(p, "x")
                        let object_expr = parse_expression_from_text(&identifiers[0], span)?;
                        Ok(Expression::FieldAccess {
                            object: Box::new(object_expr),
                            field: identifiers[1].clone(),
                        })
                    } else {
                        // Multi-level member access: utils.add -> MemberAccess(Identifier("utils"), "add")
                        let object = Box::new(Expression::Identifier(identifiers[0].clone()));
                        let member = identifiers[1..].join(".");
                        Ok(Expression::MemberAccess {
                            object,
                            member,
                        })
                    }
                }
                Rule::struct_literal => {
                    build_struct_literal(base_pair)
                }
                // Handle parenthesized expressions
                // When we have "(" ~ expression ~ ")", Pest might structure it differently
                // Try to find the expression in the inner sequence
                _ => {
                    // Check if this is a parenthesized expression by looking at the text
                    if text.starts_with('(') && text.ends_with(')') {
                        // This is a parenthesized expression, parse the inner expression
                        let inner_text = &text[1..text.len()-1].trim();
                        parse_expression_from_text(inner_text, span)
                    } else {
                        // Unknown case - provide better error message
                        Err(pest::error::Error::new_from_span(
                            pest::error::ErrorVariant::CustomError {
                                message: format!("Unexpected rule in primary: {:?}, text: '{}'", base_pair.as_rule(), text)
                            },
                            base_pair.as_span()
                        ))
                    }
                }
            }
        }
        // Pratt parser might pass us the inner rule directly
        Rule::value => build_value(primary_pair),
        Rule::array_literal => build_array_literal(primary_pair),
        Rule::expression => Ok(Expression::Group(Box::new(build_expression(primary_pair)?))),
        Rule::identifier => build_identifier_expr(primary_pair),
        Rule::loop_expression => build_loop_expression(primary_pair),
        Rule::closure_expression | Rule::atomic_closure_expression => build_closure_expression(primary_pair),
        Rule::atom => {
            // Pratt parser might pass atom directly
            build_atom(primary_pair)
        }
        _ => unreachable!("unexpected rule in build_primary: {:?}", primary_pair.as_rule()),
    }
}

fn build_atom(atom_pair: Pair<Rule>) -> Result<Expression, pest::error::Error<Rule>> {
    let mut inner = atom_pair.into_inner();
    let primary_pair = inner.next().unwrap();
    let mut expr = build_primary(primary_pair)?;
    
    // Apply any postfix operators
    for postfix_pair in inner {
        match postfix_pair.as_rule() {
            Rule::postfix => {
                let mut postfix_inner = postfix_pair.into_inner();
                let postfix_op_pair = postfix_inner.next().unwrap();
                match postfix_op_pair.as_rule() {
                    Rule::invoke => {
                        // Check if invoke has arguments
                        let mut invoke_inner = postfix_op_pair.into_inner();
                        // The first child is the "!" token (we can skip it), the second (if present) is invoke_args
                        invoke_inner.next(); // Skip the "!" token
                        let args = if let Some(invoke_args_pair) = invoke_inner.next() {
                            // Has arguments - invoke_args contains expression_list
                            // Extract the text between the parentheses
                            let args_text = invoke_args_pair.as_str();
                            // Remove the surrounding parentheses
                            let args_content = args_text.trim();
                            if args_content.len() >= 2 && args_content.starts_with('(') && args_content.ends_with(')') {
                                let inner_text = args_content[1..args_content.len()-1].trim();
                                if inner_text.is_empty() {
                                    Vec::new()
                                } else {
                                    parse_expression_list_from_text(inner_text, invoke_args_pair.as_span())?
                                }
                            } else {
                                Vec::new()
                            }
                        } else {
                            // No arguments
                            Vec::new()
                        };
                        
                        // Create Postfix expression with optional arguments
                        expr = Expression::Postfix {
                            lhs: Box::new(expr),
                            op: PostfixOp::Invoke,
                            args: if args.is_empty() { None } else { Some(args) },
                        };
                    }
                    Rule::call => {
                        // Function call syntax: (args) - creates a FunctionCall or PartialCall expression
                        let call_text = postfix_op_pair.as_str();
                        // Remove the surrounding parentheses
                        let call_content = call_text.trim();
                        if call_content.len() >= 2 && call_content.starts_with('(') && call_content.ends_with(')') {
                            let inner_text = call_content[1..call_content.len()-1].trim();
                            let call_args = if inner_text.is_empty() {
                                Vec::new()
                            } else {
                                parse_call_argument_list_from_text(inner_text, postfix_op_pair.as_span())?
                            };
                            
                            // Check if there are any holes
                            let has_holes = call_args.iter().any(|arg| matches!(arg, CallArgument::Hole));
                            
                            if has_holes {
                                // Create PartialCall
                                expr = Expression::PartialCall {
                                    func: Box::new(expr),
                                    args: call_args,
                                };
                            } else {
                                // Convert to regular FunctionCall
                                let arguments: Vec<Expression> = call_args.into_iter()
                                    .map(|arg| match arg {
                                        CallArgument::Expr(e) => e,
                                        CallArgument::Hole => unreachable!("No holes should exist here"),
                                    })
                                    .collect();
                                expr = Expression::FunctionCall {
                                    callee: Box::new(expr),
                                    arguments,
                                };
                            }
                        } else {
                            return Err(pest::error::Error::new_from_span(
                                pest::error::ErrorVariant::CustomError {
                                    message: "Call syntax missing parentheses".to_string()
                                },
                                postfix_op_pair.as_span()
                            ));
                        }
                    }
                    Rule::array_index => {
                        // Array indexing syntax: [index_specs]
                        let index_text = postfix_op_pair.as_str();
                        // Remove the surrounding brackets
                        let index_content = index_text.trim();
                        if index_content.len() >= 2 && index_content.starts_with('[') && index_content.ends_with(']') {
                            let inner_text = index_content[1..index_content.len()-1].trim();
                            let indices = if inner_text.is_empty() {
                                Vec::new()
                            } else {
                                parse_index_spec_list_from_text(inner_text, postfix_op_pair.as_span())?
                            };
                            
                            expr = Expression::ArrayIndex {
                                array: Box::new(expr),
                                indices,
                            };
                        } else {
                            return Err(pest::error::Error::new_from_span(
                                pest::error::ErrorVariant::CustomError {
                                    message: "Array index syntax missing brackets".to_string()
                                },
                                postfix_op_pair.as_span()
                            ));
                        }
                    }
                    _ => unreachable!(),
                }
            }
            _ => unreachable!(),
        }
    }
    
    Ok(expr)
}

// Helper function to parse an expression from text using the Pratt parser
fn parse_expression_from_text(text: &str, span: pest::Span) -> Result<Expression, pest::error::Error<Rule>> {
    let full_parse = CantaLoopParser::parse(Rule::expression, text)
        .map_err(|e| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { 
                message: format!("Failed to parse expression: {:?}", e)
            },
            span
        ))?;
    
    // Use Pratt parser with the parsed pairs
    // Loop expressions are now in primary via atomic_loop_expression, so they'll be handled by build_atom -> build_primary
    let result = PRATT_PARSER
            .map_primary(|atom| {
                let atom_str = atom.as_str();
                let atom_span = atom.as_span();
                build_atom(atom).unwrap_or_else(|e| {
                    let error_msg = match &e.variant {
                        pest::error::ErrorVariant::CustomError { message } => message.clone(),
                        _ => format!("{:?}", e.variant),
                    };
                    panic!("Failed to build atom expression: {} at {:?}. Atom content: '{}'", error_msg, atom_span, atom_str);
                })
            })
            .map_prefix(|op, rhs| match op.as_rule() {
                Rule::neg => Expression::Prefix {
                    op: UnaryOp::Neg,
                    rhs: Box::new(rhs),
                },
                Rule::not => Expression::Prefix {
                    op: UnaryOp::Not,
                    rhs: Box::new(rhs),
                },
                Rule::increment => Expression::Prefix {
                    op: UnaryOp::Increment,
                    rhs: Box::new(rhs),
                },
                Rule::decrement => Expression::Prefix {
                    op: UnaryOp::Decrement,
                    rhs: Box::new(rhs),
                },
                _ => unreachable!(),
            })
            .map_infix(|lhs, op, rhs| match op.as_rule() {
                Rule::add => Expression::Infix {
                    lhs: Box::new(lhs),
                    op: BinaryOp::Add,
                    rhs: Box::new(rhs),
                },
                Rule::sub => Expression::Infix {
                    lhs: Box::new(lhs),
                    op: BinaryOp::Sub,
                    rhs: Box::new(rhs),
                },
                Rule::mul => Expression::Infix {
                    lhs: Box::new(lhs),
                    op: BinaryOp::Mul,
                    rhs: Box::new(rhs),
                },
                Rule::div => Expression::Infix {
                    lhs: Box::new(lhs),
                    op: BinaryOp::Div,
                    rhs: Box::new(rhs),
                },
                Rule::modulo => Expression::Infix {
                    lhs: Box::new(lhs),
                    op: BinaryOp::Mod,
                    rhs: Box::new(rhs),
                },
                Rule::pow => Expression::Infix {
                    lhs: Box::new(lhs),
                    op: BinaryOp::Pow,
                    rhs: Box::new(rhs),
                },
                Rule::and => Expression::Infix {
                    lhs: Box::new(lhs),
                    op: BinaryOp::And,
                    rhs: Box::new(rhs),
                },
                Rule::or => Expression::Infix {
                    lhs: Box::new(lhs),
                    op: BinaryOp::Or,
                    rhs: Box::new(rhs),
                },
                Rule::eq => Expression::Infix {
                    lhs: Box::new(lhs),
                    op: BinaryOp::Eq,
                    rhs: Box::new(rhs),
                },
                Rule::ne => Expression::Infix {
                    lhs: Box::new(lhs),
                    op: BinaryOp::Ne,
                    rhs: Box::new(rhs),
                },
                Rule::gt => Expression::Infix {
                    lhs: Box::new(lhs),
                    op: BinaryOp::Gt,
                    rhs: Box::new(rhs),
                },
                Rule::lt => Expression::Infix {
                    lhs: Box::new(lhs),
                    op: BinaryOp::Lt,
                    rhs: Box::new(rhs),
                },
                Rule::ge => Expression::Infix {
                    lhs: Box::new(lhs),
                    op: BinaryOp::Ge,
                    rhs: Box::new(rhs),
                },
                Rule::le => Expression::Infix {
                    lhs: Box::new(lhs),
                    op: BinaryOp::Le,
                    rhs: Box::new(rhs),
                },
                Rule::pipe_forward => Expression::Compose {
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                    reverse: false,
                },
                Rule::pipe_reverse => Expression::Compose {
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                    reverse: true,
                },
                _ => unreachable!(),
            })
            .parse(full_parse);
    Ok(result)
}

fn build_expression(pair: Pair<Rule>) -> Result<Expression, pest::error::Error<Rule>> {
    // Get the full expression text from the pair's span
    let full_text = pair.as_str();
    let span = pair.as_span();
    parse_expression_from_text(full_text, span)
}

// Builds the chain of if-else if-else (if_statement) nodes from a Pair<Rule> of if_statement
fn build_if_chain(pair: Pair<Rule>) -> Result<Statement, pest::error::Error<Rule>> {
    let span = pair.as_span();
    let text = pair.as_str();
    let inner = pair.into_inner();

    // Since expression is a silent rule, we can't extract it directly from the parse tree
    // Instead, we need to manually extract expressions from the text
    // The structure is: "if" expression braced_block [elseif expression braced_block]* [else braced_block]?
    
    let mut arms = Vec::new();
    let mut else_block = None;
    
    // Collect braced_blocks and their positions (these we can extract from the parse tree)
    let mut blocks = Vec::new();
    for item in inner {
        match item.as_rule() {
            Rule::braced_block => {
                blocks.push(item);
            },
            _ => {} // Skip other tokens
        }
    }
    
    // Extract expressions by finding text between "if"/"elseif" keywords and the corresponding "{"
    // Structure: "if" expr block [ "elseif" expr block ]* [ "else" block ]?
    let mut expressions = Vec::new();
    let mut else_block_index = None;
    
    // Get block start positions relative to the if_statement span
    let block_starts: Vec<usize> = blocks.iter()
        .map(|b| b.as_span().start() - span.start())
        .collect();
    
    // Find the first "if " keyword
    let first_if_pos = text.find("if ").ok_or_else(|| pest::error::Error::new_from_span(
        pest::error::ErrorVariant::CustomError { 
            message: "If statement missing 'if' keyword".to_string()
        },
        span,
    ))?;
    
    let mut prev_block_end = first_if_pos + 3; // After "if "
    
    // Process each block
    for (i, &block_start) in block_starts.iter().enumerate() {
        // Text between previous block end and this block start
        let between = &text[prev_block_end..block_start];
        let trimmed = between.trim_start();
        
        // Check for "else " (must be followed by whitespace or "{")
        if trimmed.starts_with("else") {
            // Check it's "else " or "else{" (word boundary)
            let after_else = &trimmed[4..];
            if after_else.is_empty() || after_else.starts_with(' ') || after_else.starts_with('{') || after_else.starts_with('\t') || after_else.starts_with('\n') {
                // This is the else block
                else_block_index = Some(i);
                break;
            }
        }
        
        // Check for "elseif "
        if trimmed.starts_with("elseif ") {
            // Extract expression after "elseif "
            let expr_start = prev_block_end + (between.len() - trimmed.len()) + 7; // "elseif "
            let expr_text = text[expr_start..block_start].trim();
            if expr_text.is_empty() {
                return Err(pest::error::Error::new_from_span(
                    pest::error::ErrorVariant::CustomError { 
                        message: "Empty expression in elseif statement".to_string()
                    },
                    span,
                ));
            }
            expressions.push(expr_text.to_string());
        } else {
            // First block: extract expression after "if "
            let expr_text = text[prev_block_end..block_start].trim();
            if expr_text.is_empty() {
                return Err(pest::error::Error::new_from_span(
                    pest::error::ErrorVariant::CustomError { 
                        message: "Empty expression in if statement".to_string()
                    },
                    span,
                ));
            }
            expressions.push(expr_text.to_string());
        }
        
        // Move to end of this block for next iteration
        prev_block_end = blocks[i].as_span().end() - span.start();
    }
    
    if expressions.is_empty() {
        return Err(pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { 
                message: "No expressions found in if statement".to_string()
            },
            span,
        ));
    }
    
    // Build arms: pair each expression with its corresponding block
    for (i, expr_text) in expressions.iter().enumerate() {
        if i < blocks.len() {
            let expr = parse_expression_from_text(expr_text, span)?;
            let block_pair = blocks[i].clone().into_inner().next().ok_or_else(|| pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError { message: "If block missing inner block".to_string() },
                span
            ))?;
            arms.push((
                expr,
                build_block(block_pair)?,
            ));
        }
    }
    
    // Handle else block if found
    if let Some(else_idx) = else_block_index {
        if else_idx < blocks.len() {
            let block_pair = blocks[else_idx].clone().into_inner().next().ok_or_else(|| pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError { message: "Else block missing inner block".to_string() },
                span
            ))?;
            else_block = Some(build_block(block_pair)?);
        }
    }

    Ok(Statement::If {
        arms,
        else_block,
    })
}

// Helper function to recursively extract the actual statement from wrapper rules
fn extract_statement_inner(mut pair: Pair<Rule>) -> Pair<Rule> {
    loop {
        let rule = pair.as_rule();
        if matches!(rule, Rule::statement_with_semicolon | Rule::statement_without_semicolon | Rule::statement | Rule::non_assign_statement) {
            let mut inner_iter = pair.into_inner();
            // Wrapper rules should always have at least one inner element
            pair = inner_iter.next().expect("Wrapper rule should have inner element");
        } else {
            return pair;
        }
    }
}

fn build_statement(pair: Pair<Rule>) -> Result<Statement, pest::error::Error<Rule>> {
    // For wrapper rules like statement_with_semicolon, statement, non_assign_statement, extract the inner statement
    // For actual statement rules like let_statement, use the pair directly
    let statement_inner = extract_statement_inner(pair);

    match statement_inner.as_rule() {
        Rule::mod_statement => {
            build_mod_statement(statement_inner)
        }
        Rule::let_statement => {
            let span = statement_inner.as_span();
            let text = statement_inner.as_str();
            
            // Check for pub visibility
            let pub_visibility = text.trim_start().starts_with("pub");
            let let_keyword = if pub_visibility { "pub let " } else { "let " };
            
            // Extract identifier after "let " or "pub let "
            let identifier = extract_identifier_after_keyword(text, let_keyword, span, &[':', '='])?;
            
            // Find '=' position
            let eq_pos = text.find('=')
                .ok_or_else(|| error_at_span(span, "Let statement missing '='".to_string()))?;
            
            // Extract type annotation if present
            let type_annotation = if let Some(colon_pos) = text.find(':') {
                if colon_pos < eq_pos {
                    let type_text = text[colon_pos + 1..eq_pos].trim();
                    parse_type_annotation_from_text(type_text, span)?
                } else {
                    None
                }
            } else {
                None
            };
            
            // Parse expression after "="
            let expr_text = text[eq_pos + 1..].trim().strip_suffix(';').unwrap_or(&text[eq_pos + 1..]).trim();
            let expression = parse_expression_from_text(expr_text, span)?;
            
            Ok(Statement::Let {
                identifier,
                type_annotation,
                expression,
                pub_visibility,
            })
        }
        Rule::const_statement => {
            let span = statement_inner.as_span();
            let text = statement_inner.as_str();
            
            // Check for pub visibility
            let pub_visibility = text.trim_start().starts_with("pub");
            let const_keyword = if pub_visibility { "pub const " } else { "const " };
            
            // Extract identifier after "const " or "pub const "
            let identifier = extract_identifier_after_keyword(text, const_keyword, span, &['='])?;
            
            // Find '=' position
            let eq_pos = text.find('=')
                .ok_or_else(|| error_at_span(span, "Const statement missing '='".to_string()))?;
            
            // Parse expression after "="
            let expr_text = text[eq_pos + 1..].trim().strip_suffix(';').unwrap_or(&text[eq_pos + 1..]).trim();
            let expression = parse_expression_from_text(expr_text, span)?;
            
            Ok(Statement::Const {
                identifier,
                expression,
                pub_visibility,
            })
        }
        Rule::assign_statement => {
            let span = statement_inner.as_span();
            let text = statement_inner.as_str();
            
            let eq_pos = text.find('=')
                .ok_or_else(|| error_at_span(span, "Assign statement missing '='".to_string()))?;
            let identifier = text[..eq_pos].trim().to_string();
            let expression = parse_expression_from_text(text[eq_pos + 1..].trim(), span)?;

            Ok(Statement::Assign {
                identifier,
                expression,
            })
        }
        Rule::assign_increment_statement => {
            let mut inner = statement_inner.into_inner();
            let identifier = inner.next().unwrap();
            let expression = inner.next().unwrap();

            Ok(Statement::AssignIncrement {
                identifier: identifier.as_str().to_string(),
                expression: build_expression(expression)?,
            })
        }
        Rule::assign_decrement_statement => {
            let mut inner = statement_inner.into_inner();
            let identifier = inner.next().unwrap();
            let expression = inner.next().unwrap();

            Ok(Statement::AssignDecrement {
                identifier: identifier.as_str().to_string(),
                expression: build_expression(expression)?,
            })
        }
        Rule::if_statement => {
            Ok(build_if_chain(statement_inner)?)
        }
        Rule::pattern_if_statement => {
            // TODO: Implement pattern if statement handling
            return Err(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError { message: "pattern_if_statement not yet implemented".to_string() },
                statement_inner.as_span(),
            ));
        }
        Rule::pattern_match_statement => {
            // Since pattern_match_statement uses silent expression and pattern_value rules,
            // we parse it completely from text, similar to function declarations
            let span = statement_inner.as_span();
            let text = statement_inner.as_str();
            
            // Extract the expression being matched (after "match" and before "{")
            // Format: "match" ~ WHITESPACE+ ~ expression ~ WHITESPACE* ~ "{"
            let match_keyword = "match";
            let match_start = text.find(match_keyword).ok_or_else(|| pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError { message: "Match statement missing 'match' keyword".to_string() },
                span,
            ))?;
            
            // Find the opening brace
            let brace_start = text.find('{').ok_or_else(|| pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError { message: "Match statement missing opening brace".to_string() },
                span,
            ))?;
            
            // Extract expression text between "match " and "{"
            let expr_start = match_start + match_keyword.len();
            let expr_text = text[expr_start..brace_start].trim();
            if expr_text.is_empty() {
                return Err(pest::error::Error::new_from_span(
                    pest::error::ErrorVariant::CustomError { message: "Match statement missing expression".to_string() },
                    span,
                ));
            }
            let match_expr = parse_expression_from_text(expr_text, span)?;
            
            // Parse pattern cases from text manually (bypassing Pest parsing)
            // Format: pattern_value ~ WHITESPACE+ ~ braced_block
            // We need to extract each case by finding the pattern_value text and the braced_block
            let match_text = statement_inner.as_str();
            let match_span = statement_inner.as_span();
            
            // Find the opening brace of the match statement
            let match_brace_start = match_text.find('{').ok_or_else(|| pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError { message: "Match statement missing opening brace".to_string() },
                match_span,
            ))?;
            
            // Extract the content between the braces (the pattern cases)
            let mut brace_count = 0;
            let mut found_start = false;
            let mut match_brace_end = None;
            for (i, ch) in match_text[match_brace_start..].char_indices() {
                if ch == '{' {
                    brace_count += 1;
                    found_start = true;
                } else if ch == '}' {
                    brace_count -= 1;
                    if found_start && brace_count == 0 {
                        match_brace_end = Some(match_brace_start + i);
                        break;
                    }
                }
            }
            let match_brace_end = match_brace_end.ok_or_else(|| pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError { message: "Match statement missing closing brace".to_string() },
                match_span,
            ))?;
            
            let cases_text = &match_text[match_brace_start + 1..match_brace_end];
            
            // Parse each pattern case manually
            // Each case is: pattern_value ~ WHITESPACE+ ~ braced_block
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
                        match_span,
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
                    match_span,
                ))?;
                
                // Extract the block text
                let block_text = &cases_text[case_brace_start..case_brace_end + 1];
                
                // Parse the pattern_value text to determine the pattern
                let pattern = if pattern_text.trim() == "_" {
                    // Wildcard case
                    None
                } else if let Some(op_match) = ["<=", ">=", "==", "!=", "<", ">"].iter()
                    .find(|&op| pattern_text.trim_start().starts_with(op)) {
                    // Comparison pattern: operator ~ WHITESPACE+ ~ expression
                    let op_text = *op_match;
                    let op = match op_text {
                        ">" => BinaryOp::Gt,
                        "<" => BinaryOp::Lt,
                        ">=" => BinaryOp::Ge,
                        "<=" => BinaryOp::Le,
                        "==" => BinaryOp::Eq,
                        "!=" => BinaryOp::Ne,
                        _ => unreachable!(),
                    };
                    
                    // Extract the expression after the operator
                    let expr_start = pattern_text.find(op_text).unwrap() + op_text.len();
                    let rhs_text = pattern_text[expr_start..].trim();
                    if rhs_text.is_empty() {
                        return Err(pest::error::Error::new_from_span(
                            pest::error::ErrorVariant::CustomError { message: "Comparison pattern missing expression".to_string() },
                            match_span,
                        ));
                    }
                    let rhs = parse_expression_from_text(rhs_text, match_span)?;
                    
                    Some(Expression::Infix {
                        lhs: Box::new(match_expr.clone()),
                        op,
                        rhs: Box::new(rhs),
                    })
                } else {
                    // Regular expression pattern
                    Some(parse_expression_from_text(pattern_text, match_span)?)
                };
                
                // Parse the braced_block
                let block_parse_result = CantaLoopParser::parse(Rule::braced_block, block_text);
                let block = match block_parse_result {
                    Ok(mut block_pairs) => {
                        if let Some(block_pair) = block_pairs.next() {
                            let inner_block = block_pair.into_inner().find(|p| p.as_rule() == Rule::block)
                                .ok_or_else(|| pest::error::Error::new_from_span(
                                    pest::error::ErrorVariant::CustomError { message: "Pattern case block missing block".to_string() },
                                    match_span,
                                ))?;
                            build_block(inner_block)?
                        } else {
                            Block { statements: Vec::new() }
                        }
                    }
                    Err(e) => {
                        return Err(pest::error::Error::new_from_span(
                            pest::error::ErrorVariant::CustomError { 
                                message: format!("Failed to parse pattern case block: {:?}", e)
                            },
                            match_span,
                        ));
                    }
                };
                
                cases.push((pattern, block));
                
                // Move to after this case
                pos = case_brace_end + 1;
            }
            
            if cases.is_empty() {
                return Err(pest::error::Error::new_from_span(
                    pest::error::ErrorVariant::CustomError { message: "Match statement must have at least one case".to_string() },
                    span,
                ));
            }
            
            Ok(Statement::Match {
                expression: match_expr,
                cases,
            })
        }
        Rule::function_statement => {
            Ok(build_function_declaration(statement_inner)?)
        }
        Rule::return_statement => {
            let span = statement_inner.as_span();
            let text = statement_inner.as_str();
            let expr_text = extract_expression_after_keyword(text, "return", span)?;
            Ok(Statement::Return {
                expression: parse_expression_from_text(expr_text, span)?,
            })
        }
        Rule::loop_statement => {
            let span = statement_inner.as_span();
            let mut inner = statement_inner.into_inner();
            
            // Parse optional loop_init
            let init_vars = if let Some(p) = inner.peek() {
                if p.as_rule() == Rule::loop_init {
                    let loop_init_pair = inner.next().unwrap();
                    let mut vars = Vec::new();
                    for init_var_pair in loop_init_pair.into_inner() {
                        if init_var_pair.as_rule() == Rule::loop_init_var {
                            let (id, expr) = parse_loop_init_var(
                                init_var_pair.as_str(),
                                init_var_pair.as_span()
                            )?;
                            vars.push((id, expr));
                        }
                    }
                    vars
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };
            
            let braced_block = inner.next().unwrap();
            let block_pair = braced_block.into_inner()
                .find(|p| p.as_rule() == Rule::block)
                .ok_or_else(|| error_at_span(span, "Loop body missing block".to_string()))?;
            
            Ok(Statement::Loop {
                init_vars,
                body: build_block(block_pair)?,
            })
        }
        Rule::while_statement => {
            let span = statement_inner.as_span();
            let text = statement_inner.as_str();
            
            let while_pos = find_keyword(text, "while", span)?;
            let after_while = &text[while_pos + 5..];
            let brace_start_offset = find_opening_brace(after_while, span, "While statement")?;
            let brace_start = while_pos + 5 + brace_start_offset;
            
            let condition_text = text[while_pos + 5..brace_start].trim();
            let condition = parse_expression_from_text(condition_text, span)?;
            
            // Parse body block from parse tree
            let mut inner = statement_inner.into_inner();
            let braced_block = inner.find(|p| p.as_rule() == Rule::braced_block)
                .ok_or_else(|| error_at_span(span, "While body missing block".to_string()))?;
            let block_pair = braced_block.into_inner()
                .find(|p| p.as_rule() == Rule::block)
                .ok_or_else(|| error_at_span(span, "While body missing inner block".to_string()))?;
            
            Ok(Statement::While {
                condition,
                body: build_block(block_pair)?,
            })
        }
        Rule::for_statement => {
            let span = statement_inner.as_span();
            let text = statement_inner.as_str();
            
            let for_pos = find_keyword(text, "for", span)?;
            let after_for = &text[for_pos + 3..];
            
            // Find "in" as word boundary
            let in_pos = find_word_boundary(after_for, "in")
                .ok_or_else(|| error_at_span(span, "For statement missing 'in'".to_string()))?;
            let var_name = after_for[..in_pos].trim().to_string();
            
            // Find ".." range operator
            let after_in = &after_for[in_pos + 2..];
            let dotdot_pos = after_in.find("..")
                .ok_or_else(|| error_at_span(span, "For statement missing '..'".to_string()))?;
            let start_text = after_in[..dotdot_pos].trim();
            
            // Find opening brace
            let after_dotdot = &after_in[dotdot_pos + 2..];
            let brace_start_offset = find_opening_brace(after_dotdot, span, "For statement")?;
            let end_text = after_dotdot[..brace_start_offset].trim();
            
            let start = parse_expression_from_text(start_text, span)?;
            let end = parse_expression_from_text(end_text, span)?;
            
            // Extract body block
            let body_content = extract_braced_content(after_dotdot, brace_start_offset, span)?;
            let body_block = parse_block_from_text(body_content, span)?;
            
            Ok(Statement::For {
                var_name,
                start,
                end,
                body: body_block,
            })
        }
        Rule::break_statement => {
            let span = statement_inner.as_span();
            let text = statement_inner.as_str();
            let expr_text = extract_expression_after_keyword(text, "break", span)?;
            Ok(Statement::Break {
                expression: if expr_text.is_empty() {
                    None
                } else {
                    Some(parse_expression_from_text(expr_text, span)?)
                },
            })
        }
        Rule::continue_statement => {
            Ok(Statement::Continue)
        }
        Rule::expression_statement => {
            let span = statement_inner.as_span();
            let full_text = statement_inner.as_str();
            // Since expression is a silent rule, we can't get it from into_inner()
            // Instead, parse the full text directly using the helper function
            Ok(Statement::Expression(parse_expression_from_text(full_text, span)?))
        }
        Rule::use_statement => {
            build_use_statement(statement_inner)
        }
        Rule::struct_statement => {
            build_struct_statement(statement_inner)
        }
        _ => unreachable!("unexpected rule in build_statement: {:?}, text: {:?}", statement_inner.as_rule(), statement_inner.as_str()),
    }
}

fn build_mod_statement(pair: Pair<Rule>) -> Result<Statement, pest::error::Error<Rule>> {
    let span = pair.as_span();
    let mut identifier = None;
    
    // mod_statement = ${ "mod" ~ WHITESPACE+ ~ identifier }
    // The identifier is in the inner pairs
    for inner in pair.into_inner() {
        if inner.as_rule() == Rule::identifier {
            identifier = Some(inner.as_str().to_string());
            break;
        }
    }
    
    let identifier = identifier.ok_or_else(|| {
        error_at_span(span, "Mod statement missing identifier".to_string())
    })?;
    
    Ok(Statement::Mod {
        identifier,
    })
}

fn build_use_statement(pair: Pair<Rule>) -> Result<Statement, pest::error::Error<Rule>> {
    let span = pair.as_span();
    let mut path = Vec::new();
    let mut selector = None;
    
    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::import_items => {
                selector = Some(build_import_items(inner)?);
            }
            Rule::import_path => {
                // Parse dot-separated identifiers
                for part in inner.into_inner() {
                    if part.as_rule() == Rule::identifier {
                        path.push(part.as_str().to_string());
                    }
                }
            }
            _ => {}
        }
    }
    
    if path.is_empty() {
        return Err(error_at_span(span, "Use statement missing import path".to_string()));
    }
    
    if selector.is_none() {
        return Err(error_at_span(span, "Use statement missing import items".to_string()));
    }
    
    Ok(Statement::Use {
        path,
        selector: selector.unwrap(),
    })
}

fn build_import_items(pair: Pair<Rule>) -> Result<ImportSelector, pest::error::Error<Rule>> {
    use crate::core::ast::ImportSelector;
    
    let span = pair.as_span();
    let text = pair.as_str().trim();
    
    // Check for wildcard: *
    if text == "*" {
        return Ok(ImportSelector::Wildcard);
    }
    
    // Parse comma-separated identifiers
    let mut identifiers = Vec::new();
    for inner in pair.into_inner() {
        if inner.as_rule() == Rule::identifier {
            identifiers.push(inner.as_str().to_string());
        }
    }
    
    // If we found identifiers, return them
    if !identifiers.is_empty() {
        if identifiers.len() == 1 {
            return Ok(ImportSelector::Single(identifiers[0].clone()));
        } else {
            return Ok(ImportSelector::Multiple(identifiers));
        }
    }
    
    // Fallback: try to parse as a single identifier from the text
    // This handles cases where the grammar might not have matched identifiers properly
    if !text.is_empty() && text != "*" {
        // Try splitting by comma
        let parts: Vec<&str> = text.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
        if parts.len() == 1 {
            return Ok(ImportSelector::Single(parts[0].to_string()));
        } else if parts.len() > 1 {
            return Ok(ImportSelector::Multiple(parts.iter().map(|s| s.to_string()).collect()));
        }
    }
    
    Err(error_at_span(span, "Invalid import items".to_string()))
}

fn build_struct_statement(pair: Pair<Rule>) -> Result<Statement, pest::error::Error<Rule>> {
    use crate::core::ast::Statement;
    
    let span = pair.as_span();
    let text = pair.as_str();
    
    // Check for pub visibility
    let pub_visibility = text.trim_start().starts_with("pub");
    let struct_keyword = if pub_visibility { "pub struct " } else { "struct " };
    
    // Extract identifier after "struct " or "pub struct "
    let identifier = extract_identifier_after_keyword(text, struct_keyword, span, &['{'])?;
    
    // Find opening brace
    let brace_start = find_opening_brace(text, span, "Struct")?;
    
    // Extract struct fields content
    let fields_content = extract_braced_content(text, brace_start, span)?;
    
    // Parse struct fields
    let fields = if fields_content.trim().is_empty() {
        Vec::new()
    } else {
        // Trim the content to handle leading/trailing whitespace
        parse_struct_fields(fields_content.trim(), span)?
    };
    
    Ok(Statement::Struct {
        name: identifier,
        fields,
        pub_visibility,
    })
}

fn parse_struct_fields(
    fields_text: &str,
    span: pest::Span,
) -> Result<Vec<(String, String)>, pest::error::Error<Rule>> {
    // Trim the fields text to remove leading/trailing whitespace
    let trimmed = fields_text.trim();
    let mut parse_result = CantaLoopParser::parse(Rule::struct_fields, trimmed)
        .map_err(|e| error_at_span(span, format!("Failed to parse struct fields: {}", e)))?;
    
    let mut fields = Vec::new();
    if let Some(fields_pair) = parse_result.next() {
        for field_pair in fields_pair.into_inner() {
            if field_pair.as_rule() == Rule::struct_field {
                let field_span = field_pair.as_span();
                let mut field_inner = field_pair.into_inner();
                let field_name = field_inner.next()
                    .ok_or_else(|| error_at_span(field_span, "Struct field missing identifier".to_string()))?
                    .as_str()
                    .to_string();
                let type_pair = field_inner.next()
                    .ok_or_else(|| error_at_span(field_span, "Struct field missing type annotation".to_string()))?;
                let type_annotation = build_type_annotation(type_pair)?;
                fields.push((field_name, type_annotation));
            }
        }
    }
    
    Ok(fields)
}

fn build_struct_literal(pair: Pair<Rule>) -> Result<Expression, pest::error::Error<Rule>> {
    use crate::core::ast::Expression;
    
    let span = pair.as_span();
    let text = pair.as_str();
    
    // Find the struct name (identifier before "{")
    let brace_start = find_opening_brace(text, span, "Struct literal")?;
    let struct_name = text[..brace_start].trim().to_string();
    
    // Extract struct initialization fields content
    let fields_content = extract_braced_content(text, brace_start, span)?;
    
    // Parse struct initialization fields
    let fields = if fields_content.trim().is_empty() {
        Vec::new()
    } else {
        // Trim the content to handle leading/trailing whitespace
        parse_struct_init_fields(fields_content.trim(), span)?
    };
    
    Ok(Expression::StructInit {
        struct_name,
        fields,
    })
}

fn parse_struct_init_fields(
    fields_text: &str,
    span: pest::Span,
) -> Result<Vec<(String, Expression)>, pest::error::Error<Rule>> {
    // Trim the fields text to remove leading/trailing whitespace
    let trimmed = fields_text.trim();
    let mut parse_result = CantaLoopParser::parse(Rule::struct_init_fields, trimmed)
        .map_err(|e| error_at_span(span, format!("Failed to parse struct init fields: {}", e)))?;
    
    let mut fields = Vec::new();
    if let Some(fields_pair) = parse_result.next() {
        for field_pair in fields_pair.into_inner() {
            if field_pair.as_rule() == Rule::struct_init_field {
                let field_span = field_pair.as_span();
                let field_text = field_pair.as_str();
                
                // Since expression is a silent rule, we need to extract it manually from the text
                // Format: identifier ~ WHITESPACE* ~ ":" ~ WHITESPACE* ~ expression
                // Find the colon to split identifier from expression
                let colon_pos = field_text.find(':')
                    .ok_or_else(|| error_at_span(field_span, "Struct init field missing ':' separator".to_string()))?;
                
                // Extract identifier (everything before the colon)
                let identifier_text = field_text[..colon_pos].trim();
                let field_name = identifier_text.to_string();
                
                // Extract expression (everything after the colon)
                let expr_text = field_text[colon_pos + 1..].trim();
                if expr_text.is_empty() {
                    return Err(error_at_span(field_span, "Struct init field missing expression".to_string()));
                }
                
                let expression = parse_expression_from_text(expr_text, field_span)?;
                fields.push((field_name, expression));
            }
        }
    }
    
    Ok(fields)
}