//! Custom Pratt parser for CST expression parsing with CstId support.
//!
//! This module implements a precedence-climbing Pratt parser that properly threads
//! `&mut CstIdGenerator` through all parsing functions, ensuring every CST node
//! gets an ID from the same ID space.
//!
//! This replaces the closure-based `pest::pratt_parser::PrattParser` which cannot
//! easily pass `&mut CstIdGenerator` through closures.

use pest::iterators::{Pair, Pairs};
use pest::RuleType;

use crate::core::parser::Rule;
use crate::core::cst::{
    CstExpr, CstBinaryOp, CstComposeOp, CstUnaryOp, CstPostfixOp,
    Span, Spanned, CstIdGenerator,
};
use crate::core::cst::builder::build_cst_atom;

/// Operator precedence levels (higher = tighter binding)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Precedence {
    Pipe = 1,      // |> <|
    Or = 2,        // ||
    And = 3,       // &&
    Comparison = 4, // == != < > <= >=
    Add = 5,       // + -
    Mul = 6,       // * / %
    Pow = 7,       // ** (right-associative)
    Prefix = 8,    // ! - ++ --
    Postfix = 9,   // ! () []
}

impl Precedence {
    fn from_rule(rule: Rule) -> Option<Self> {
        use Rule::*;
        match rule {
            pipe_forward | pipe_reverse => Some(Precedence::Pipe),
            or => Some(Precedence::Or),
            and => Some(Precedence::And),
            eq | ne | gt | lt | ge | le => Some(Precedence::Comparison),
            add | sub => Some(Precedence::Add),
            mul | div | modulo => Some(Precedence::Mul),
            pow => Some(Precedence::Pow),
            not | neg | increment | decrement => Some(Precedence::Prefix),
            invoke | call => Some(Precedence::Postfix),
            _ => None,
        }
    }

    fn is_right_associative(&self) -> bool {
        matches!(self, Precedence::Pow)
    }

    fn next_level(self) -> Self {
        // Get next higher precedence level for left-associative operators
        match self {
            Precedence::Pipe => Precedence::Or,
            Precedence::Or => Precedence::And,
            Precedence::And => Precedence::Comparison,
            Precedence::Comparison => Precedence::Add,
            Precedence::Add => Precedence::Mul,
            Precedence::Mul => Precedence::Pow,
            Precedence::Pow => Precedence::Prefix,
            Precedence::Prefix => Precedence::Postfix,
            Precedence::Postfix => Precedence::Postfix, // Max level
        }
    }
}

/// Parse an expression using precedence climbing.
///
/// This implements the standard Pratt parser algorithm:
/// 1. Parse a primary (atom)
/// 2. While there's an operator with precedence >= min_precedence:
///    - If right-associative, parse RHS with same precedence
///    - If left-associative, parse RHS with next higher precedence
///    - Build the infix/prefix/postfix expression
pub fn parse_expression(
    pairs: Pairs<Rule>,
    id_gen: &mut CstIdGenerator,
) -> Result<Spanned<CstExpr>, pest::error::Error<Rule>> {
    let mut iter = pairs.peekable();
    parse_expression_with_precedence(&mut iter, id_gen, Precedence::Pipe)
}

fn parse_expression_with_precedence(
    iter: &mut std::iter::Peekable<Pairs<Rule>>,
    id_gen: &mut CstIdGenerator,
    min_precedence: Precedence,
) -> Result<Spanned<CstExpr>, pest::error::Error<Rule>> {
    // Parse prefix operators (if any)
    let mut expr = parse_prefix(iter, id_gen)?;

    // Parse infix/postfix operators
    // Note: Since expression and infix are silent rules (_), pest flattens them.
    // Operators appear directly as Rule::add, Rule::mul, etc., not wrapped in Rule::infix.
    while let Some(op_pair) = iter.peek() {
        let op_rule = op_pair.as_rule();
        
        // Check if this is an operator with precedence
        // Operators appear directly: Rule::add, Rule::mul, etc. (not wrapped in Rule::infix)
        // Note: Since expression and infix are silent rules (_), pest flattens them.
        if let Some(prec) = Precedence::from_rule(op_rule) {
            if prec < min_precedence {
                break;
            }

            if prec == Precedence::Postfix {
                // Handle postfix operators (invoke, call, index)
                let op_pair = iter.next().unwrap();
                expr = parse_postfix(expr, op_pair, iter, id_gen)?;
            } else if prec >= Precedence::Prefix {
                // This shouldn't happen - prefix is handled earlier
                break;
            } else {
                // Infix operator - operators appear directly, no unwrapping needed
                let op_pair = iter.next().unwrap();
                
                // Determine RHS precedence
                // For left-associative operators, parse RHS with next higher precedence
                // to prevent operators of the same precedence from being parsed in the RHS
                // For right-associative operators (like pow), parse RHS with same precedence
                let rhs_prec = if prec.is_right_associative() {
                    prec
                } else {
                    // For left-associative, use next higher precedence level
                    // This ensures a + b + c parses as (a + b) + c, not a + (b + c)
                    prec.next_level()
                };
                
                // Parse RHS
                let rhs = parse_expression_with_precedence(iter, id_gen, rhs_prec)?;
                
                // Build infix expression - operators appear directly, no unwrapping needed
                expr = build_infix_expr(expr, op_pair, rhs, id_gen)?;
            }
        } else {
            // Not an operator, stop parsing
            break;
        }
    }

    Ok(expr)
}

fn parse_prefix(
    iter: &mut std::iter::Peekable<Pairs<Rule>>,
    id_gen: &mut CstIdGenerator,
) -> Result<Spanned<CstExpr>, pest::error::Error<Rule>> {
    // Check for prefix operators
    if let Some(pair) = iter.peek() {
        let rule = pair.as_rule();
        if matches!(rule, Rule::not | Rule::neg | Rule::increment | Rule::decrement) {
            let op_pair = iter.next().unwrap();
            let rhs = parse_prefix(iter, id_gen)?; // Prefix operators are right-associative
            
            let op_span = Span::from_pest_span(op_pair.as_span());
            let op_node = match rule {
                Rule::neg => CstUnaryOp::Neg,
                Rule::not => CstUnaryOp::Not,
                Rule::increment => CstUnaryOp::Increment,
                Rule::decrement => CstUnaryOp::Decrement,
                _ => unreachable!(),
            };
            
            let id = id_gen.next();
            let op_id = id_gen.next();
            Ok(Spanned::new(
                id,
                op_span.merge(rhs.span),
                CstExpr::Prefix {
                    op: Spanned::new(op_id, op_span, op_node),
                    rhs: Box::new(rhs),
                },
            ))
        } else {
            // No prefix operator, parse atom
            parse_atom(iter, id_gen)
        }
    } else {
        // No more tokens
        Err(pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError {
                message: "Unexpected end of expression".to_string(),
            },
            pest::Span::new("", 0, 0).unwrap(),
        ))
    }
}

fn parse_atom(
    iter: &mut std::iter::Peekable<Pairs<Rule>>,
    id_gen: &mut CstIdGenerator,
) -> Result<Spanned<CstExpr>, pest::error::Error<Rule>> {
    let atom_pair = iter.next().ok_or_else(|| {
        pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError {
                message: "Expected expression atom".to_string(),
            },
            pest::Span::new("", 0, 0).unwrap(),
        )
    })?;

    build_cst_atom(atom_pair, id_gen)
}

fn parse_postfix(
    lhs: Spanned<CstExpr>,
    op_pair: Pair<Rule>,
    iter: &mut std::iter::Peekable<Pairs<Rule>>,
    id_gen: &mut CstIdGenerator,
) -> Result<Spanned<CstExpr>, pest::error::Error<Rule>> {
    // Postfix operators are handled in build_cst_atom
    // This function should not be called in the current architecture
    // But we include it for completeness
    unimplemented!("Postfix operators are handled in build_cst_atom")
}

fn build_infix_expr(
    lhs: Spanned<CstExpr>,
    op_pair: Pair<Rule>,
    rhs: Spanned<CstExpr>,
    id_gen: &mut CstIdGenerator,
) -> Result<Spanned<CstExpr>, pest::error::Error<Rule>> {
    use Rule::*;
    let op_span = Span::from_pest_span(op_pair.as_span());
    let combined_span = lhs.span.merge(op_span).merge(rhs.span);
    
    let expr = match op_pair.as_rule() {
        pipe_forward => CstExpr::Compose {
            lhs: Box::new(lhs),
            op: {
                let op_id = id_gen.next();
                Spanned::new(op_id, op_span, CstComposeOp::Forward)
            },
            rhs: Box::new(rhs),
        },
        pipe_reverse => CstExpr::Compose {
            lhs: Box::new(lhs),
            op: {
                let op_id = id_gen.next();
                Spanned::new(op_id, op_span, CstComposeOp::Reverse)
            },
            rhs: Box::new(rhs),
        },
        add => CstExpr::Infix {
            lhs: Box::new(lhs),
            op: {
                let op_id = id_gen.next();
                Spanned::new(op_id, op_span, CstBinaryOp::Add)
            },
            rhs: Box::new(rhs),
        },
        sub => CstExpr::Infix {
            lhs: Box::new(lhs),
            op: {
                let op_id = id_gen.next();
                Spanned::new(op_id, op_span, CstBinaryOp::Sub)
            },
            rhs: Box::new(rhs),
        },
        mul => CstExpr::Infix {
            lhs: Box::new(lhs),
            op: {
                let op_id = id_gen.next();
                Spanned::new(op_id, op_span, CstBinaryOp::Mul)
            },
            rhs: Box::new(rhs),
        },
        div => CstExpr::Infix {
            lhs: Box::new(lhs),
            op: {
                let op_id = id_gen.next();
                Spanned::new(op_id, op_span, CstBinaryOp::Div)
            },
            rhs: Box::new(rhs),
        },
        modulo => CstExpr::Infix {
            lhs: Box::new(lhs),
            op: {
                let op_id = id_gen.next();
                Spanned::new(op_id, op_span, CstBinaryOp::Mod)
            },
            rhs: Box::new(rhs),
        },
        pow => CstExpr::Infix {
            lhs: Box::new(lhs),
            op: {
                let op_id = id_gen.next();
                Spanned::new(op_id, op_span, CstBinaryOp::Pow)
            },
            rhs: Box::new(rhs),
        },
        and => CstExpr::Infix {
            lhs: Box::new(lhs),
            op: {
                let op_id = id_gen.next();
                Spanned::new(op_id, op_span, CstBinaryOp::And)
            },
            rhs: Box::new(rhs),
        },
        or => CstExpr::Infix {
            lhs: Box::new(lhs),
            op: {
                let op_id = id_gen.next();
                Spanned::new(op_id, op_span, CstBinaryOp::Or)
            },
            rhs: Box::new(rhs),
        },
        eq => CstExpr::Infix {
            lhs: Box::new(lhs),
            op: {
                let op_id = id_gen.next();
                Spanned::new(op_id, op_span, CstBinaryOp::Eq)
            },
            rhs: Box::new(rhs),
        },
        ne => CstExpr::Infix {
            lhs: Box::new(lhs),
            op: {
                let op_id = id_gen.next();
                Spanned::new(op_id, op_span, CstBinaryOp::Ne)
            },
            rhs: Box::new(rhs),
        },
        gt => CstExpr::Infix {
            lhs: Box::new(lhs),
            op: {
                let op_id = id_gen.next();
                Spanned::new(op_id, op_span, CstBinaryOp::Gt)
            },
            rhs: Box::new(rhs),
        },
        lt => CstExpr::Infix {
            lhs: Box::new(lhs),
            op: {
                let op_id = id_gen.next();
                Spanned::new(op_id, op_span, CstBinaryOp::Lt)
            },
            rhs: Box::new(rhs),
        },
        ge => CstExpr::Infix {
            lhs: Box::new(lhs),
            op: {
                let op_id = id_gen.next();
                Spanned::new(op_id, op_span, CstBinaryOp::Ge)
            },
            rhs: Box::new(rhs),
        },
        le => CstExpr::Infix {
            lhs: Box::new(lhs),
            op: {
                let op_id = id_gen.next();
                Spanned::new(op_id, op_span, CstBinaryOp::Le)
            },
            rhs: Box::new(rhs),
        },
        _ => {
            return Err(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: format!("Unexpected operator: {:?}", op_pair.as_rule()),
                },
                op_pair.as_span(),
            ));
        }
    };
    
    let id = id_gen.next();
    Ok(Spanned::new(id, combined_span, expr))
}

impl From<u8> for Precedence {
    fn from(value: u8) -> Self {
        match value {
            1 => Precedence::Pipe,
            2 => Precedence::Or,
            3 => Precedence::And,
            4 => Precedence::Comparison,
            5 => Precedence::Add,
            6 => Precedence::Mul,
            7 => Precedence::Pow,
            8 => Precedence::Prefix,
            9 => Precedence::Postfix,
            _ => Precedence::Pipe, // Default to lowest
        }
    }
}
