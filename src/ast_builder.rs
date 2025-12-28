use pest::iterators::Pair;

use crate::parser::{Rule, PRATT_PARSER, CantaLoopParser};
use pest::Parser;
use crate::ast::{Expression, Statement, Program, Block, Literal, UnaryOp, BinaryOp, PostfixOp};

// Helper function to build type annotation string from a Pair<Rule>
fn build_type_annotation(pair: Pair<Rule>) -> Result<String, pest::error::Error<Rule>> {
    // Just return the string representation of the type annotation
    // The semantic analyser will parse it properly
    Ok(pair.as_str().trim().to_string())
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
    use crate::ast::{Argument, Statement};

    let span = pair.as_span();
    let text = pair.as_str();
    
    // For atomic rules, identifier is also atomic so we need to extract it from the string
    // Format: "fn " + identifier + "(" ...
    // Find the identifier after "fn "
    let fn_keyword = "fn ";
    let start = text.find(fn_keyword).ok_or_else(|| pest::error::Error::new_from_span(
        pest::error::ErrorVariant::CustomError { message: "Function missing 'fn' keyword".to_string() },
        span
    ))? + fn_keyword.len();
    
    // Find where the identifier ends (whitespace or "(")
    let identifier_end = text[start..].find(|c: char| c.is_whitespace() || c == '(')
        .ok_or_else(|| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "Function missing identifier".to_string() },
            span
        ))?;
    let identifier = text[start..start + identifier_end].trim().to_string();
    
    // For atomic rules, into_inner() returns nothing, so we need to parse manually
    // Find the block by looking for "{" in the text after ")"
    let paren_end = text.find(')').ok_or_else(|| pest::error::Error::new_from_span(
        pest::error::ErrorVariant::CustomError { message: "Function missing closing paren".to_string() },
        span
    ))? + 1;
    // Find the opening brace after the closing paren (skip whitespace and optional return type)
    let after_paren = &text[paren_end..];
    
    // Check for return type annotation (-> type)
    let return_type = if after_paren.trim_start().starts_with("->") {
        // Parse return type annotation
        // Find the position after "->" and whitespace
        let arrow_pos = after_paren.find("->").unwrap();
        let after_arrow = &after_paren[arrow_pos + 2..].trim_start();
        
        // Find where the type annotation ends (before the opening brace)
        let brace_pos = after_arrow.find('{').unwrap_or(after_arrow.len());
        let type_text = after_arrow[..brace_pos].trim();
        
        if !type_text.is_empty() {
            // Try to parse as type_annotation
            if let Ok(mut type_pairs) = CantaLoopParser::parse(Rule::type_annotation, type_text) {
                if let Some(type_pair) = type_pairs.next() {
                    let type_name = build_type_annotation(type_pair)?;
                    Some(type_name)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };
    
    // Find the opening brace (skip return type annotation if present)
    let brace_start_offset = after_paren.find('{').ok_or_else(|| pest::error::Error::new_from_span(
        pest::error::ErrorVariant::CustomError { message: format!("Function missing opening brace. Text after paren: {:?}", after_paren.chars().take(20).collect::<String>()) },
        span
    ))?;
    let brace_start = paren_end + brace_start_offset;
    // Find the matching closing brace
    let mut brace_count = 0;
    let mut brace_end = None;
    for (i, c) in text[brace_start..].char_indices() {
        if c == '{' {
            brace_count += 1;
        } else if c == '}' {
            brace_count -= 1;
            if brace_count == 0 {
                brace_end = Some(brace_start + i + 1);
                break;
            }
        }
    }
    let brace_end = brace_end.ok_or_else(|| pest::error::Error::new_from_span(
        pest::error::ErrorVariant::CustomError { message: "Function missing closing brace".to_string() },
        span
    ))?;
    let block_text = &text[brace_start..brace_end];
    
    // Parse arguments from the text between identifier and closing paren
    let mut arguments = Vec::new();
    let args_start = text.find('(').ok_or_else(|| pest::error::Error::new_from_span(
        pest::error::ErrorVariant::CustomError { message: "Function missing opening paren".to_string() },
        span
    ))? + 1;
    let args_text = &text[args_start..paren_end - 1].trim();
    if !args_text.is_empty() {
        // Parse function_args
        let args_pairs = CantaLoopParser::parse(Rule::function_args, args_text)
            .map_err(|e| pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError { message: format!("Failed to parse function arguments: {}", e) },
                span
            ))?;
        let mut args_pairs = args_pairs;
        if let Some(args_pair) = args_pairs.next() {
            for arg_pair in args_pair.into_inner() {
                if arg_pair.as_rule() == Rule::argument {
                    let arg_span = arg_pair.as_span();
                    let mut arg_inner = arg_pair.into_inner();
                    let id = arg_inner.next().ok_or_else(|| pest::error::Error::new_from_span(
                        pest::error::ErrorVariant::CustomError { message: "Function argument missing identifier".to_string() },
                        arg_span
                    ))?.as_str().to_string();
                    // Skip whitespace and colon - they are silent or not in inner pairs
                    // The next should be the type_annotation
                    let type_pair = arg_inner.next().ok_or_else(|| pest::error::Error::new_from_span(
                        pest::error::ErrorVariant::CustomError { message: "Function argument missing type annotation".to_string() },
                        arg_span
                    ))?;
                    let kind = build_type_annotation(type_pair)?;
                    arguments.push(Argument { identifier: id, kind });
                }
            }
        }
    }
    
    // Parse braced_block
    let parse_result = CantaLoopParser::parse(Rule::braced_block, block_text);
    let mut block_pairs = parse_result
        .map_err(|e| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: format!("Failed to parse function body: {:?}", e) },
            span
        ))?;
    let body_pair = block_pairs.next().ok_or_else(|| pest::error::Error::new_from_span(
        pest::error::ErrorVariant::CustomError { message: "Function missing body".to_string() },
        span
    ))?;
    
    // braced_block contains a block inside
    let block_pair = body_pair.into_inner().find(|p| p.as_rule() == Rule::block)
        .ok_or_else(|| pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "Function body missing block".to_string() },
            span
        ))?;
    let body_block = build_block(block_pair)?;

    Ok(Statement::FunctionDeclaration {
        identifier,
        arguments,
        return_type,
        body: body_block,
    })
}

fn build_identifier_expr(pair: Pair<Rule>) -> Result<Expression, pest::error::Error<Rule>> {
    let identifier = pair.as_str().to_string();
    // Keywords are prevented by grammar, but check here as a safety measure
    const KEYWORDS: &[&str] = &["fn", "if", "else", "elseif", "match", "return", "let", "true", "false", "and", "or"];
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

fn build_value(pair: Pair<Rule>) -> Result<Expression, pest::error::Error<Rule>> {
    let mut inner = pair.into_inner();
    let inner_pair = inner.next().unwrap();
    match inner_pair.as_rule() {
        Rule::number => build_number(inner_pair),
        Rule::string => build_string(inner_pair),
        Rule::boolean => build_boolean(inner_pair),
        _ => unreachable!(),
    }
}

fn build_call_expression(pair: Pair<Rule>) -> Result<Expression, pest::error::Error<Rule>> {
    let span = pair.as_span();
    let text = pair.as_str();
    // Since call_expression is atomic, parse it from text
    // Format: identifier "(" [expression_list] ")"
    // Find the identifier (everything before "(")
    let paren_start = text.find('(').ok_or_else(|| pest::error::Error::new_from_span(
        pest::error::ErrorVariant::CustomError { message: "Call expression missing opening paren".to_string() },
        span
    ))?;
    let identifier = text[..paren_start].trim().to_string();
    
    // Find the closing paren
    let paren_end = text.rfind(')').ok_or_else(|| pest::error::Error::new_from_span(
        pest::error::ErrorVariant::CustomError { message: "Call expression missing closing paren".to_string() },
        span
    ))?;
    
    // Extract the expression list text (between the parens)
    let args_text = text[paren_start + 1..paren_end].trim();
    let arguments = if args_text.is_empty() {
        Vec::new()
    } else {
        // Parse the expression list by splitting on commas and parsing each expression
        parse_expression_list_from_text(args_text, span)?
    };
    
    Ok(Expression::FunctionCall {
        callee: Box::new(Expression::Identifier(identifier)),
        arguments,
    })
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

// Helper function to parse a loop expression from text
// This is used when loop expressions need to be parsed from text directly
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
    
    // The entire loop expression is matched manually, so we parse everything from the text
    // Format: "loop" [init_vars] "{" block "}"
    if !full_text.starts_with("loop") {
        return Err(pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { 
                message: format!("Loop expression must start with 'loop', got: {}", full_text.chars().take(20).collect::<String>()) 
            },
            span,
        ));
    }
    
    // Find the opening brace (separates init vars from block)
    let brace_start = full_text.find('{').ok_or_else(|| pest::error::Error::new_from_span(
        pest::error::ErrorVariant::CustomError { message: "Loop expression missing opening brace".to_string() },
        span,
    ))?;
    
    // Parse optional init vars (everything between "loop" and "{")
    let init_text = full_text[4..brace_start].trim(); // Skip "loop" (4 chars)
    let mut init_vars = Vec::new();
    if !init_text.is_empty() {
        // Parse init vars: "a = 0, b = 1, i = 0"
        let init_parse_result = CantaLoopParser::parse(Rule::loop_init, init_text);
        match init_parse_result {
            Ok(mut init_pairs) => {
                if let Some(init_pair) = init_pairs.next() {
                    for init_var_pair in init_pair.into_inner() {
                        if init_var_pair.as_rule() == Rule::loop_init_var {
                            // Since expression is silent, we need to extract it manually from the text
                            let init_var_text = init_var_pair.as_str();
                            let init_var_span = init_var_pair.as_span();
                            // Format: "identifier = expression"
                            let eq_pos = init_var_text.find('=').ok_or_else(|| pest::error::Error::new_from_span(
                                pest::error::ErrorVariant::CustomError { message: "Loop init var missing '='".to_string() },
                                init_var_span
                            ))?;
                            let identifier = init_var_text[..eq_pos].trim().to_string();
                            let expr_text = init_var_text[eq_pos + 1..].trim();
                            // Use parse_expression_from_text since expressions are now handled by *_from_text functions
                            init_vars.push((identifier, parse_expression_from_text(expr_text, init_var_span)?));
                        }
                    }
                }
            }
            Err(e) => {
                return Err(pest::error::Error::new_from_span(
                    pest::error::ErrorVariant::CustomError { 
                        message: format!("Failed to parse loop init vars: {:?}. Init text: '{}'", e, init_text)
                    },
                    span
                ));
            }
        }
    }
    
    // Find matching closing brace by counting braces (handles nested braces)
    let mut brace_count = 0;
    let mut found_start = false;
    let mut brace_end = None;
    for (i, ch) in full_text[brace_start..].char_indices() {
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
    
    // Extract the block content between the braces
    let block_content = if let Some(end) = brace_end {
        full_text[brace_start + 1..end].trim()
    } else {
        return Err(pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "Loop expression missing matching closing brace".to_string() },
            span,
        ));
    };
    
    // Manually parse the block content as a block (statements, not expressions)
    // This ensures statements are parsed correctly even when loop_expression is part of an expression context
    let block_parse_result = CantaLoopParser::parse(Rule::block, block_content);
    let body_block = match block_parse_result {
        Ok(mut block_pairs) => {
            if let Some(block_pair) = block_pairs.next() {
                build_block(block_pair)?
            } else {
                // Empty block
                Block { statements: Vec::new() }
            }
        }
        Err(e) => {
            return Err(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError { 
                    message: format!("Failed to parse loop body as block: {:?}. Content: {:?}", e, block_content.chars().take(100).collect::<String>())
                },
                span,
            ));
        }
    };
    
    Ok(Expression::Loop {
        init_vars,
        body: body_block,
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
                Rule::expression => Ok(Expression::Group(Box::new(build_expression(base_pair)?))),
                Rule::identifier => build_identifier_expr(base_pair),
                Rule::call_expression => build_call_expression(base_pair),
                Rule::loop_expression => build_loop_expression(base_pair),
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
        Rule::expression => Ok(Expression::Group(Box::new(build_expression(primary_pair)?))),
        Rule::identifier => build_identifier_expr(primary_pair),
        Rule::call_expression => build_call_expression(primary_pair),
        Rule::loop_expression => build_loop_expression(primary_pair),
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
                        // Function call syntax: (args) - creates a FunctionCall expression
                        let call_text = postfix_op_pair.as_str();
                        // Remove the surrounding parentheses
                        let call_content = call_text.trim();
                        if call_content.len() >= 2 && call_content.starts_with('(') && call_content.ends_with(')') {
                            let inner_text = call_content[1..call_content.len()-1].trim();
                            let args = if inner_text.is_empty() {
                                Vec::new()
                            } else {
                                parse_expression_list_from_text(inner_text, postfix_op_pair.as_span())?
                            };
                            
                            // Create FunctionCall with the current expression as the callee
                            expr = Expression::FunctionCall {
                                callee: Box::new(expr),
                                arguments: args,
                            };
                        } else {
                            return Err(pest::error::Error::new_from_span(
                                pest::error::ErrorVariant::CustomError {
                                    message: "Call syntax missing parentheses".to_string()
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
        Rule::let_statement => {
            let span = statement_inner.as_span();
            let text = statement_inner.as_str();
            
            // Parse "let identifier [: type] = expression"
            // Find the identifier after "let "
            let let_keyword = "let ";
            let start = text.find(let_keyword).ok_or_else(|| pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError { message: "Let statement missing 'let' keyword".to_string() },
                span
            ))? + let_keyword.len();
            
            // Find where the identifier ends (whitespace before ":" or "=")
            let identifier_end = text[start..].find(|c: char| c.is_whitespace() || c == ':' || c == '=')
                .ok_or_else(|| pest::error::Error::new_from_span(
                    pest::error::ErrorVariant::CustomError { message: "Let statement missing identifier".to_string() },
                    span
                ))?;
            let identifier = text[start..start + identifier_end].trim().to_string();
            
            // Check if there's a type annotation (look for ":" before "=")
            let eq_pos = text.find('=').ok_or_else(|| pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError { message: "Let statement missing '='".to_string() },
                span
            ))?;
            
            // Try to parse the let statement using the grammar to get proper type annotation parsing
            let type_annotation = if let Some(colon_pos) = text[start + identifier_end..eq_pos].find(':') {
                // Type annotation is present - parse it using the grammar
                let colon_abs_pos = start + identifier_end + colon_pos;
                let type_text = &text[colon_abs_pos + 1..eq_pos].trim();
                // Try to parse as type_annotation
                if let Ok(mut type_pairs) = CantaLoopParser::parse(Rule::type_annotation, type_text) {
                    if let Some(type_pair) = type_pairs.next() {
                        Some(build_type_annotation(type_pair)?)
                    } else {
                        None
                    }
                } else {
                    // Fallback: just use the text (for backwards compatibility)
                    Some(type_text.to_string())
                }
            } else {
                // No type annotation
                None
            };
            
            // Parse the expression after "="
            let expr_text = text[eq_pos + 1..].trim();
            // Remove any trailing semicolon (shouldn't be there, but handle it just in case)
            let expr_text = expr_text.strip_suffix(';').unwrap_or(expr_text).trim();
            // Since expression is a silent rule, parse it directly using the helper function
            let expression = parse_expression_from_text(expr_text, span)?;
            
            Ok(Statement::Let {
                identifier,
                type_annotation,
                expression,
            })
        }
        Rule::assign_statement => {
            let span = statement_inner.as_span();
            let text = statement_inner.as_str();
            
            // Parse "identifier = expression"
            // Find the identifier (everything before "=")
            let eq_pos = text.find('=').ok_or_else(|| pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError { message: "Assign statement missing '='".to_string() },
                span
            ))?;
            let identifier = text[..eq_pos].trim().to_string();
            
            // Parse the expression after "="
            let expr_text = text[eq_pos + 1..].trim();
            // Since expression is a silent rule, parse it directly using the helper function
            let expression = parse_expression_from_text(expr_text, span)?;

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
            // Parse "return expression" - find the expression after "return" keyword
            let return_keyword = "return";
            let expr_start = text.find(return_keyword).ok_or_else(|| pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError { message: "Return statement missing 'return' keyword".to_string() },
                span
            ))? + return_keyword.len();
            let expr_text = text[expr_start..].trim();
            // Since expression is a silent rule, parse it directly using the helper function
            Ok(Statement::Return {
                expression: parse_expression_from_text(expr_text, span)?,
            })
        }
        Rule::loop_statement => {
            let span = statement_inner.as_span();
            let mut inner = statement_inner.into_inner();
            
            // Parse optional loop_init
            let mut init_vars = Vec::new();
            let next = inner.peek();
            if let Some(p) = next {
                if p.as_rule() == Rule::loop_init {
                    let loop_init_pair = inner.next().unwrap();
                    for init_var_pair in loop_init_pair.into_inner() {
                        if init_var_pair.as_rule() == Rule::loop_init_var {
                            // Since expression is silent, we need to extract it manually from the text
                            let init_var_text = init_var_pair.as_str();
                            let init_var_span = init_var_pair.as_span();
                            // Format: "identifier = expression"
                            let eq_pos = init_var_text.find('=').ok_or_else(|| pest::error::Error::new_from_span(
                                pest::error::ErrorVariant::CustomError { message: "Loop init var missing '='".to_string() },
                                init_var_span
                            ))?;
                            let identifier = init_var_text[..eq_pos].trim().to_string();
                            let expr_text = init_var_text[eq_pos + 1..].trim();
                            // Use parse_expression_from_text since expressions are now handled by *_from_text functions
                            init_vars.push((identifier, parse_expression_from_text(expr_text, init_var_span)?));
                        }
                    }
                }
            }
            
            let braced_block = inner.next().unwrap();
            let block_pair = braced_block.into_inner().find(|p| p.as_rule() == Rule::block)
                .ok_or_else(|| pest::error::Error::new_from_span(
                    pest::error::ErrorVariant::CustomError { message: "Loop body missing block".to_string() },
                    span,
                ))?;
            Ok(Statement::Loop {
                init_vars,
                body: build_block(block_pair)?,
            })
        }
        Rule::while_statement => {
            let span = statement_inner.as_span();
            let text = statement_inner.as_str();
            // Parse "while expression { ... }"
            // Find the condition expression between "while" and "{"
            let while_keyword = "while";
            let while_pos = text.find(while_keyword).ok_or_else(|| pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError { message: "While statement missing 'while' keyword".to_string() },
                span
            ))?;
            
            // Find the opening brace (need to handle nested braces)
            let mut brace_count = 0;
            let mut found_brace = false;
            let mut brace_pos = None;
            for (i, ch) in text[while_pos + while_keyword.len()..].char_indices() {
                if ch == '{' {
                    brace_count += 1;
                    if !found_brace {
                        found_brace = true;
                        brace_pos = Some(while_pos + while_keyword.len() + i);
                    }
                } else if ch == '}' && found_brace {
                    brace_count -= 1;
                    if brace_count == 0 {
                        break;
                    }
                }
            }
            
            let brace_start = brace_pos.ok_or_else(|| pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError { message: "While statement missing opening brace".to_string() },
                span
            ))?;
            
            let condition_text = text[while_pos + while_keyword.len()..brace_start].trim();
            let condition = parse_expression_from_text(condition_text, span)?;
            
            // Parse the body block - get it from the parse tree
            let mut inner = statement_inner.into_inner();
            let braced_block = inner.find(|p| p.as_rule() == Rule::braced_block)
                .ok_or_else(|| pest::error::Error::new_from_span(
                    pest::error::ErrorVariant::CustomError { message: "While body missing block".to_string() },
                    span,
                ))?;
            let block_pair = braced_block.into_inner().find(|p| p.as_rule() == Rule::block)
                .ok_or_else(|| pest::error::Error::new_from_span(
                    pest::error::ErrorVariant::CustomError { message: "While body missing inner block".to_string() },
                    span,
                ))?;
            
            Ok(Statement::While {
                condition,
                body: build_block(block_pair)?,
            })
        }
        Rule::for_statement => {
            let span = statement_inner.as_span();
            let text = statement_inner.as_str();
            // Parse "for identifier in expression .. expression { ... }"
            // Format: "for x in start .. end { ... }"
            // The grammar now uses for_range_content and for_block_manual, so parse manually from text
            let for_keyword = "for";
            let for_pos = text.find(for_keyword).ok_or_else(|| pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError { message: "For statement missing 'for' keyword".to_string() },
                span
            ))?;
            
            // Find the "in" after the variable name
            let after_for = &text[for_pos + for_keyword.len()..];
            // Look for "in" as a word boundary (not part of another word like "print")
            let in_pattern = "in";
            let mut in_pos = None;
            for i in 0..after_for.len() {
                if after_for[i..].starts_with(in_pattern) {
                    // Check if it's a word boundary (not followed by alphanumeric)
                    let after_in = if i + in_pattern.len() < after_for.len() {
                        after_for.chars().nth(i + in_pattern.len())
                    } else {
                        None
                    };
                    if after_in.is_none() || !after_in.unwrap().is_alphanumeric() {
                        in_pos = Some(i);
                        break;
                    }
                }
            }
            let in_pos_byte = in_pos.ok_or_else(|| pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError { message: "For statement missing 'in'".to_string() },
                span
            ))?;
            let var_name = after_for[..in_pos_byte].trim().to_string();
            
            // Find the ".." range operator
            let after_in = &after_for[in_pos_byte + in_pattern.len()..];
            let dotdot_pos = after_in.find("..").ok_or_else(|| pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError { message: "For statement missing '..'".to_string() },
                span
            ))?;
            let start_text = after_in[..dotdot_pos].trim();
            
            // Find the opening brace
            let after_dotdot = &after_in[dotdot_pos + 2..];
            let mut brace_count = 0;
            let mut found_brace = false;
            let mut brace_pos = None;
            for (i, ch) in after_dotdot.char_indices() {
                if ch == '{' {
                    brace_count += 1;
                    if !found_brace {
                        found_brace = true;
                        brace_pos = Some(i);
                    }
                } else if ch == '}' && found_brace {
                    brace_count -= 1;
                    if brace_count == 0 {
                        break;
                    }
                }
            }
            
            let brace_start = brace_pos.ok_or_else(|| pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError { message: "For statement missing opening brace".to_string() },
                span
            ))?;
            
            let end_text = after_dotdot[..brace_start].trim();
            
            let start = parse_expression_from_text(start_text, span)?;
            let end = parse_expression_from_text(end_text, span)?;
            
            // Extract the body block content (between braces)
            let brace_end_pos = brace_start;
            // Find the matching closing brace
            let mut brace_end = None;
            let mut count = 1;
            for (i, ch) in after_dotdot[brace_end_pos + 1..].char_indices() {
                if ch == '{' {
                    count += 1;
                } else if ch == '}' {
                    count -= 1;
                    if count == 0 {
                        brace_end = Some(brace_end_pos + 1 + i);
                        break;
                    }
                }
            }
            let body_content = if let Some(end_pos) = brace_end {
                &after_dotdot[brace_end_pos + 1..end_pos]
            } else {
                return Err(pest::error::Error::new_from_span(
                    pest::error::ErrorVariant::CustomError { message: "For statement missing closing brace".to_string() },
                    span
                ));
            };
            
            // Parse the body content as a block
            let block_parse_result = CantaLoopParser::parse(Rule::block, body_content);
            let body_block = match block_parse_result {
                Ok(mut block_pairs) => {
                    if let Some(block_pair) = block_pairs.next() {
                        build_block(block_pair)?
                    } else {
                        Block { statements: Vec::new() }
                    }
                }
                Err(e) => {
                    return Err(pest::error::Error::new_from_span(
                        pest::error::ErrorVariant::CustomError { 
                            message: format!("Failed to parse for loop body as block: {:?}", e)
                        },
                        span,
                    ));
                }
            };
            
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
            // Parse "break [expression]" - expression is optional
            let break_keyword = "break";
            if let Some(keyword_pos) = text.find(break_keyword) {
                let expr_start = keyword_pos + break_keyword.len();
                let expr_text = text[expr_start..].trim();
                Ok(Statement::Break {
                    expression: if expr_text.is_empty() {
                        None
                    } else {
                        // Since expression is a silent rule, parse it directly using the helper function
                        Some(parse_expression_from_text(expr_text, span)?)
                    },
                })
            } else {
                Ok(Statement::Break {
                    expression: None,
                })
            }
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
        _ => unreachable!("unexpected rule in build_statement: {:?}, text: {:?}", statement_inner.as_rule(), statement_inner.as_str()),
    }
}