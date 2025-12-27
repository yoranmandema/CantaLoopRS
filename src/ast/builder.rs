use pest::iterators::{Pair, Pairs};

use crate::parser::{Rule, PRATT_PARSER, CantaLoopParser};
use pest::Parser;
use crate::ast::{Expression, Statement, Program, Block, Literal, UnaryOp, BinaryOp, PostfixOp};

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
            Rule::statement => {
                statements.push(build_statement(inner)?);
            },
            _ => unreachable!("unexpected rule: {:?}", inner.as_rule()),

        }
    }

    Ok(Block { statements })
}

fn build_function_declaration(pair: Pair<Rule>) -> Result<Statement, pest::error::Error<Rule>> {
    use crate::ast::{Argument, Statement};

    let span = pair.as_span();
    let text = pair.as_str();
    // #region agent log
    let log_path = ".cursor/debug.log";
    let log_line = format!("{{\"sessionId\":\"debug-session\",\"runId\":\"pre-fix\",\"hypothesisId\":\"H1,H2,H3,H4,H5\",\"location\":\"builder.rs:42\",\"message\":\"build_function_declaration entry\",\"data\":{{\"text_length\":{},\"text\":{:?}}},\"timestamp\":{}}}\n", text.len(), text, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis());
    let _ = std::fs::OpenOptions::new().create(true).append(true).open(log_path).and_then(|mut f| std::io::Write::write_all(&mut f, log_line.as_bytes()));
    // #endregion
    
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
    // Find the opening brace after the closing paren (skip whitespace)
    let after_paren = &text[paren_end..];
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
    // #region agent log
    let log_line3 = format!("{{\"sessionId\":\"debug-session\",\"runId\":\"pre-fix\",\"hypothesisId\":\"H1,H2,H3,H4,H5\",\"location\":\"builder.rs:93\",\"message\":\"Extracted block_text\",\"data\":{{\"brace_start\":{},\"brace_end\":{},\"block_text\":{:?},\"block_text_first_50_chars\":{:?}}},\"timestamp\":{}}}\n", brace_start, brace_end, block_text, block_text.chars().take(50).collect::<String>(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis());
    let _ = std::fs::OpenOptions::new().create(true).append(true).open(log_path).and_then(|mut f| std::io::Write::write_all(&mut f, log_line3.as_bytes()));
    // #endregion
    
    // Parse arguments from the text between identifier and closing paren
    let mut arguments = Vec::new();
    let args_start = text.find('(').ok_or_else(|| pest::error::Error::new_from_span(
        pest::error::ErrorVariant::CustomError { message: "Function missing opening paren".to_string() },
        span
    ))? + 1;
    let args_text = &text[args_start..paren_end - 1].trim();
    if !args_text.is_empty() {
        // Parse function_args
        if let Ok(mut args_pairs) = CantaLoopParser::parse(Rule::function_args, args_text) {
            if let Some(args_pair) = args_pairs.next() {
                for arg_pair in args_pair.into_inner() {
                    if arg_pair.as_rule() == Rule::argument {
                        let mut arg_inner = arg_pair.into_inner();
                        let id = arg_inner.next().unwrap().as_str().to_string();
                        let kind = if let Some(_colon) = arg_inner.next() {
                            let type_name = arg_inner.next().unwrap().as_str().to_string();
                            type_name
                        } else {
                            "Any".to_string()
                        };
                        arguments.push(Argument { identifier: id, kind });
                    }
                }
            }
        }
    }
    
    // Parse braced_block
    // #region agent log
    let log_path = ".cursor/debug.log";
    let log_line1 = format!("{{\"sessionId\":\"debug-session\",\"runId\":\"pre-fix\",\"hypothesisId\":\"H1,H2,H3,H4,H5\",\"location\":\"builder.rs:123\",\"message\":\"Attempting to parse braced_block\",\"data\":{{\"block_text_length\":{},\"block_text\":{:?},\"block_text_preview\":{:?}}},\"timestamp\":{}}}\n", block_text.len(), block_text, block_text.chars().take(200).collect::<String>(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis());
    let _ = std::fs::OpenOptions::new().create(true).append(true).open(log_path).and_then(|mut f| std::io::Write::write_all(&mut f, log_line1.as_bytes()));
    // #endregion
    let parse_result = CantaLoopParser::parse(Rule::braced_block, block_text);
    // #region agent log
    let log_line2 = match &parse_result {
        Ok(_) => format!("{{\"sessionId\":\"debug-session\",\"runId\":\"pre-fix\",\"hypothesisId\":\"H1,H2,H3,H4,H5\",\"location\":\"builder.rs:125\",\"message\":\"braced_block parse success\",\"data\":{{}},\"timestamp\":{}}}\n", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()),
        Err(e) => format!("{{\"sessionId\":\"debug-session\",\"runId\":\"pre-fix\",\"hypothesisId\":\"H1,H2,H3,H4,H5\",\"location\":\"builder.rs:125\",\"message\":\"braced_block parse error\",\"data\":{{\"error\":{:?},\"error_location\":{:?}}},\"timestamp\":{}}}\n", format!("{:?}", e), e.location, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()),
    };
    let _ = std::fs::OpenOptions::new().create(true).append(true).open(log_path).and_then(|mut f| std::io::Write::write_all(&mut f, log_line2.as_bytes()));
    // #endregion
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
    let mut inner = pair.into_inner();
    let identifier = inner.next().unwrap();
    let expression_list = inner.next();
    let arguments = if let Some(list_pair) = expression_list {
        build_expression_list(list_pair.into_inner())?
    } else {
        Vec::new()
    };
    Ok(Expression::FunctionCall {
        identifier: identifier.as_str().to_string(),
        arguments,
    })
}

fn build_expression_list(pairs: Pairs<Rule>) -> Result<Vec<Expression>, pest::error::Error<Rule>> {
    let mut expressions = Vec::new();
    for pair in pairs {
        expressions.push(build_expression(pair.into_inner())?);
    }
    Ok(expressions)
}

fn build_primary(primary_pair: Pair<Rule>) -> Result<Expression, pest::error::Error<Rule>> {
    match primary_pair.as_rule() {
        Rule::primary => {
            // Primary rule contains the base expression
            let mut inner = primary_pair.into_inner();
            let base_pair = inner.next().unwrap();
            
            match base_pair.as_rule() {
                Rule::value => build_value(base_pair),
                Rule::expression => Ok(Expression::Group(Box::new(build_expression(base_pair.into_inner())?))),
                Rule::identifier => build_identifier_expr(base_pair),
                Rule::call_expression => build_call_expression(base_pair),
                _ => unreachable!(),
            }
        }
        // Pratt parser might pass us the inner rule directly
        Rule::value => build_value(primary_pair),
        Rule::expression => Ok(Expression::Group(Box::new(build_expression(primary_pair.into_inner())?))),
        Rule::identifier => build_identifier_expr(primary_pair),
        Rule::call_expression => build_call_expression(primary_pair),
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
                let invoke_pair = postfix_inner.next().unwrap();
                match invoke_pair.as_rule() {
                    Rule::invoke => {
                        // Check if invoke has arguments
                        let mut invoke_inner = invoke_pair.into_inner();
                        // The first child is the "!" token (we can skip it), the second (if present) is invoke_args
                        invoke_inner.next(); // Skip the "!" token
                        let args = if let Some(invoke_args_pair) = invoke_inner.next() {
                            // Has arguments - invoke_args contains expression_list
                            let mut args_inner = invoke_args_pair.into_inner();
                            if let Some(expr_list_pair) = args_inner.next() {
                                build_expression_list(expr_list_pair.into_inner())?
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
                    _ => unreachable!(),
                }
            }
            _ => unreachable!(),
        }
    }
    
    Ok(expr)
}

fn build_expression(pairs: Pairs<Rule>) -> Result<Expression, pest::error::Error<Rule>> {
    // Use Pratt parser - it handles operator precedence correctly
    let result = PRATT_PARSER
        .map_primary(|atom| {
            let atom_str = atom.as_str();
            let atom_span = atom.as_span();
            build_atom(atom).unwrap_or_else(|e| {
                // If building atom fails, panic with a helpful error
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
            _ => unreachable!(),
        })
        .parse(pairs);
    Ok(result)
}

// Builds the chain of if-else if-else (if_statement) nodes from a Pair<Rule> of if_statement
fn build_if_chain(pair: Pair<Rule>) -> Result<Statement, pest::error::Error<Rule>> {
    let span = pair.as_span();
    let inner = pair.into_inner();

    // The structure is: expression, braced_block, (expression, braced_block)*, braced_block?
    // Collect all expressions and blocks first, then pair them up
    let mut expressions = Vec::new();
    let mut blocks = Vec::new();
    
    for item in inner {
        match item.as_rule() {
            Rule::expression => {
                expressions.push(item);
            },
            Rule::braced_block => blocks.push(item),
            _ => {} // Skip other tokens like parentheses, "if", "else"
        }
    }
    
    if expressions.is_empty() {
        return Err(pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { 
                message: "No expressions found in if statement".to_string()
            },
            span,
        ));
    }
    
    if blocks.len() != expressions.len() && blocks.len() != expressions.len() + 1 {
        return Err(pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { 
                message: format!("Mismatched expressions and blocks: {} expressions, {} blocks", expressions.len(), blocks.len())
            },
            span,
        ));
    }
    
    // Build arms: pair each expression with its corresponding block
    let mut arms = Vec::new();
    for i in 0..expressions.len() {
        arms.push((
            build_expression(expressions[i].clone().into_inner())?,
            build_block(blocks[i].clone().into_inner().next().unwrap())?,
        ));
    }
    
    // The last block (if there's one more than expressions) is the else block
    let else_block = if blocks.len() > expressions.len() {
        Some(build_block(blocks[expressions.len()].clone().into_inner().next().unwrap())?)
    } else {
        None
    };

    Ok(Statement::If {
        arms,
        else_block,
    })
}

fn build_statement(pair: Pair<Rule>) -> Result<Statement, pest::error::Error<Rule>> {
    let statement_inner = pair.into_inner().next().unwrap();

    match statement_inner.as_rule() {
        Rule::let_statement => {
            let span = statement_inner.as_span();
            let text = statement_inner.as_str();
            
            // Parse "let identifier = expression"
            // Find the identifier after "let "
            let let_keyword = "let ";
            let start = text.find(let_keyword).ok_or_else(|| pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError { message: "Let statement missing 'let' keyword".to_string() },
                span
            ))? + let_keyword.len();
            
            // Find where the identifier ends (whitespace or "=")
            let identifier_end = text[start..].find(|c: char| c.is_whitespace() || c == '=')
                .ok_or_else(|| pest::error::Error::new_from_span(
                    pest::error::ErrorVariant::CustomError { message: "Let statement missing identifier".to_string() },
                    span
                ))?;
            let identifier = text[start..start + identifier_end].trim().to_string();
            
            // Find the "=" sign
            let eq_pos = text.find('=').ok_or_else(|| pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError { message: "Let statement missing '='".to_string() },
                span
            ))?;
            
            // Parse the expression after "="
            let expr_text = text[eq_pos + 1..].trim();
            let parse_result = CantaLoopParser::parse(Rule::expression, expr_text)
                .map_err(|e| pest::error::Error::new_from_span(
                    pest::error::ErrorVariant::CustomError { message: format!("Failed to parse expression in let statement: {:?}", e) },
                    span
                ))?;
            let mut expr_pairs = parse_result;
            let expr_pair = expr_pairs.next().ok_or_else(|| pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError { message: "Let statement missing expression".to_string() },
                span
            ))?;
            
            Ok(Statement::Let {
                identifier,
                expression: build_expression(expr_pair.into_inner())?,
            })
        }
        Rule::assign_statement => {
            let mut inner = statement_inner.into_inner();
            let identifier = inner.next().unwrap();
            let expression = inner.next().unwrap();

            Ok(Statement::Assign {
                identifier: identifier.as_str().to_string(),
                expression: build_expression(expression.into_inner())?,
            })
        }
        Rule::assign_increment_statement => {
            let mut inner = statement_inner.into_inner();
            let identifier = inner.next().unwrap();
            let expression = inner.next().unwrap();

            Ok(Statement::AssignIncrement {
                identifier: identifier.as_str().to_string(),
                expression: build_expression(expression.into_inner())?,
            })
        }
        Rule::assign_decrement_statement => {
            let mut inner = statement_inner.into_inner();
            let identifier = inner.next().unwrap();
            let expression = inner.next().unwrap();

            Ok(Statement::AssignDecrement {
                identifier: identifier.as_str().to_string(),
                expression: build_expression(expression.into_inner())?,
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
        Rule::function_statement => {
            Ok(build_function_declaration(statement_inner)?)
        }
        Rule::return_statement => {
            let mut inner = statement_inner.into_inner();
            let expression = inner.next().unwrap();
            Ok(Statement::Return {
                expression: build_expression(expression.into_inner())?,
            })
        }
        Rule::expression_statement => {
            Ok(Statement::Expression(build_expression(statement_inner.into_inner())?))
        }
        _ => unreachable!(),
    }
}