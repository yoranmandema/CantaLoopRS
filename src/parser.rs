use pest::Parser as PestParser;
use pest::iterators::{Pair, Pairs};
use pest::pratt_parser::Assoc;
use pest::pratt_parser::PrattParser;
use pest_derive::Parser;

lazy_static! {
    static ref PRATT_PARSER: PrattParser<Rule> = {
        use Rule::*;
        use pest::pratt_parser::{Assoc::*, Op};
        PrattParser::new()
            .op(Op::infix(Rule::add, Assoc::Left) | Op::infix(Rule::sub, Assoc::Left))
            .op(Op::infix(Rule::mul, Assoc::Left) | Op::infix(Rule::div, Assoc::Left))
            .op(Op::infix(Rule::pow, Assoc::Right))
            .op(Op::prefix(Rule::not) | Op::prefix(Rule::neg) | Op::prefix(Rule::increment) | Op::prefix(Rule::decrement))
    };
}


#[derive(Parser)]
#[grammar = "src/grammar/grammar.pest"]
pub struct CantaLoopParser;

#[derive(Debug, Clone)]
pub enum Expression {
    Literal(Literal),
    Identifier(String),
    FunctionCall {
        identifier: String,
        arguments: Vec<Expression>,
    },
    Prefix {
        op: UnaryOp,
        rhs: Box<Expression>,
    },
    Infix {
        lhs: Box<Expression>,
        op: BinaryOp,
        rhs: Box<Expression>,
    },
    Group(Box<Expression>),
}

#[derive(Debug, Clone, Copy)]
pub enum UnaryOp {
    Neg,
    Increment,
    Decrement,
    Not,
}

#[derive(Debug, Clone, Copy)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
}

#[derive(Debug)]
pub struct Program {
    pub statements: Vec<Statement>,
}

#[derive(Debug)]
pub enum Statement {
    Assign {
        identifier: String,
        expression: Expression,
    },
    AssignIncrement {
        identifier: String,
        expression: Expression,
    },
    AssignDecrement {
        identifier: String,
        expression: Expression,
    },
    Expression(Expression),
}

#[derive(Debug, Clone)]
pub enum Literal {
    String(String),
    Number(f64),
}

fn build_program(pair: Pair<Rule>) -> Result<Program, pest::error::Error<Rule>> {
    let mut statements = Vec::new();

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::statement => {
                statements.push(build_statement(inner)?);
            }
            Rule::EOI => {}
            _ => unreachable!("unexpected rule: {:?}", inner.as_rule()),
        }
    }

    Ok(Program { statements })
}

fn build_identifier_expr(pair: Pair<Rule>) -> Result<Expression, pest::error::Error<Rule>> {
    Ok(Expression::Identifier(pair.as_str().to_string()))
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

fn build_value(pair: Pair<Rule>) -> Result<Expression, pest::error::Error<Rule>> {
    let mut inner = pair.into_inner();
    let inner_pair = inner.next().unwrap();
    match inner_pair.as_rule() {
        Rule::number => build_number(inner_pair),
        Rule::string => build_string(inner_pair),
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
        _ => unreachable!(),
    }
}

fn build_expression(pairs: Pairs<Rule>) -> Result<Expression, pest::error::Error<Rule>> {
    // Use Pratt parser - it handles operator precedence correctly
    let result = PRATT_PARSER
        .map_primary(|primary| {
            build_primary(primary).unwrap()
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
            _ => unreachable!(),
        })
        .parse(pairs);
    Ok(result)
}

fn build_statement(pair: Pair<Rule>) -> Result<Statement, pest::error::Error<Rule>> {
    let statement_inner = pair.into_inner().next().unwrap();

    match statement_inner.as_rule() {
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
        Rule::expression => {
            Ok(Statement::Expression(build_expression(statement_inner.into_inner())?))
        }
        _ => unreachable!(),
    }
}

pub fn parse_program(src: &str) -> Result<Program, pest::error::Error<Rule>> {
    let mut pairs = CantaLoopParser::parse(Rule::program, src)?;
    let program = pairs.next().unwrap();
    build_program(program)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_parses(input: &str) -> Program {
        parse_program(input).expect(&format!("Failed to parse: {}", input))
    }

    #[test]
    fn test_number_literal() {
        let program = assert_parses("42;");
        assert_eq!(program.statements.len(), 1);
        match &program.statements[0] {
            Statement::Expression(Expression::Literal(Literal::Number(n))) => {
                assert_eq!(*n, 42.0);
            }
            _ => panic!("Expected number literal"),
        }
    }

    #[test]
    fn test_decimal_number() {
        let program = assert_parses("3.14;");
        match &program.statements[0] {
            Statement::Expression(Expression::Literal(Literal::Number(n))) => {
                assert_eq!(*n, 3.14);
            }
            _ => panic!("Expected decimal number"),
        }
    }

    #[test]
    fn test_string_literal() {
        let program = assert_parses("\"hello\";");
        match &program.statements[0] {
            Statement::Expression(Expression::Literal(Literal::String(s))) => {
                assert_eq!(s, "hello");
            }
            _ => panic!("Expected string literal"),
        }
    }

    #[test]
    fn test_identifier() {
        let program = assert_parses("x;");
        match &program.statements[0] {
            Statement::Expression(Expression::Identifier(id)) => {
                assert_eq!(id, "x");
            }
            _ => panic!("Expected identifier"),
        }
    }

    #[test]
    fn test_assignment() {
        let program = assert_parses("x = 42;");
        match &program.statements[0] {
            Statement::Assign { identifier, expression } => {
                assert_eq!(identifier, "x");
                match expression {
                    Expression::Literal(Literal::Number(n)) => assert_eq!(*n, 42.0),
                    _ => panic!("Expected number in assignment"),
                }
            }
            _ => panic!("Expected assignment statement"),
        }
    }

    #[test]
    fn test_string_assignment() {
        let program = assert_parses("msg = \"hello\";");
        match &program.statements[0] {
            Statement::Assign { identifier, expression } => {
                assert_eq!(identifier, "msg");
                match expression {
                    Expression::Literal(Literal::String(s)) => assert_eq!(s, "hello"),
                    _ => panic!("Expected string in assignment"),
                }
            }
            _ => panic!("Expected assignment statement"),
        }
    }

    #[test]
    fn test_addition() {
        let program = assert_parses("1 + 2;");
        match &program.statements[0] {
            Statement::Expression(Expression::Infix { lhs, op, rhs }) => {
                assert!(matches!(op, BinaryOp::Add));
                match lhs.as_ref() {
                    Expression::Literal(Literal::Number(n)) => assert_eq!(*n, 1.0),
                    _ => panic!("Expected number on left"),
                }
                match rhs.as_ref() {
                    Expression::Literal(Literal::Number(n)) => assert_eq!(*n, 2.0),
                    _ => panic!("Expected number on right"),
                }
            }
            _ => panic!("Expected infix expression"),
        }
    }

    #[test]
    fn test_subtraction() {
        let program = assert_parses("5 - 3;");
        match &program.statements[0] {
            Statement::Expression(Expression::Infix { op, .. }) => {
                assert!(matches!(op, BinaryOp::Sub));
            }
            _ => panic!("Expected subtraction"),
        }
    }

    #[test]
    fn test_multiplication() {
        let program = assert_parses("2 * 3;");
        match &program.statements[0] {
            Statement::Expression(Expression::Infix { op, .. }) => {
                assert!(matches!(op, BinaryOp::Mul));
            }
            _ => panic!("Expected multiplication"),
        }
    }

    #[test]
    fn test_division() {
        let program = assert_parses("10 / 2;");
        match &program.statements[0] {
            Statement::Expression(Expression::Infix { op, .. }) => {
                assert!(matches!(op, BinaryOp::Div));
            }
            _ => panic!("Expected division"),
        }
    }

    #[test]
    fn test_exponentiation() {
        let program = assert_parses("2 ^ 3;");
        match &program.statements[0] {
            Statement::Expression(Expression::Infix { op, .. }) => {
                assert!(matches!(op, BinaryOp::Pow));
            }
            _ => panic!("Expected exponentiation"),
        }
    }

    #[test]
    fn test_operator_precedence() {
        let program = assert_parses("1 + 2 * 3;");
        // Should parse as 1 + (2 * 3)
        match &program.statements[0] {
            Statement::Expression(Expression::Infix { lhs, op, rhs }) => {
                assert!(matches!(op, BinaryOp::Add));
                match lhs.as_ref() {
                    Expression::Literal(Literal::Number(n)) => assert_eq!(*n, 1.0),
                    _ => panic!("Expected 1 on left"),
                }
                match rhs.as_ref() {
                    Expression::Infix { op, .. } => assert!(matches!(op, BinaryOp::Mul)),
                    _ => panic!("Expected multiplication on right"),
                }
            }
            _ => panic!("Expected addition with multiplication"),
        }
    }

    #[test]
    fn test_exponentiation_precedence() {
        let program = assert_parses("2 ^ 3 ^ 2;");
        // Exponentiation is right-associative
        match &program.statements[0] {
            Statement::Expression(Expression::Infix { lhs, op, rhs }) => {
                assert!(matches!(op, BinaryOp::Pow));
                match rhs.as_ref() {
                    Expression::Infix { op, .. } => assert!(matches!(op, BinaryOp::Pow)),
                    _ => panic!("Expected nested exponentiation"),
                }
            }
            _ => panic!("Expected exponentiation"),
        }
    }

    #[test]
    fn test_prefix_negation() {
        let program = assert_parses("-5;");
        match &program.statements[0] {
            Statement::Expression(Expression::Prefix { op, rhs }) => {
                assert!(matches!(op, UnaryOp::Neg));
                match rhs.as_ref() {
                    Expression::Literal(Literal::Number(n)) => assert_eq!(*n, 5.0),
                    _ => panic!("Expected number after negation"),
                }
            }
            _ => panic!("Expected prefix negation"),
        }
    }

    #[test]
    fn test_prefix_not() {
        let program = assert_parses("!true;");
        match &program.statements[0] {
            Statement::Expression(Expression::Prefix { op, .. }) => {
                assert!(matches!(op, UnaryOp::Not));
            }
            _ => panic!("Expected prefix not"),
        }
    }

    #[test]
    fn test_prefix_increment() {
        let program = assert_parses("++x;");
        match &program.statements[0] {
            Statement::Expression(Expression::Prefix { op, .. }) => {
                assert!(matches!(op, UnaryOp::Increment));
            }
            _ => panic!("Expected prefix increment"),
        }
    }

    #[test]
    fn test_prefix_decrement() {
        let program = assert_parses("--x;");
        match &program.statements[0] {
            Statement::Expression(Expression::Prefix { op, .. }) => {
                assert!(matches!(op, UnaryOp::Decrement));
            }
            _ => panic!("Expected prefix decrement"),
        }
    }


    #[test]
    fn test_grouped_expression() {
        let program = assert_parses("(1 + 2) * 3;");
        // Should parse as (1 + 2) * 3
        match &program.statements[0] {
            Statement::Expression(Expression::Infix { lhs, op, rhs }) => {
                assert!(matches!(op, BinaryOp::Mul));
                match lhs.as_ref() {
                    Expression::Group(expr) => {
                        match expr.as_ref() {
                            Expression::Infix { op, .. } => assert!(matches!(op, BinaryOp::Add)),
                            _ => panic!("Expected addition in group"),
                        }
                    }
                    _ => panic!("Expected grouped expression on left"),
                }
                match rhs.as_ref() {
                    Expression::Literal(Literal::Number(n)) => assert_eq!(*n, 3.0),
                    _ => panic!("Expected 3 on right"),
                }
            }
            _ => panic!("Expected multiplication"),
        }
    }

    #[test]
    fn test_function_call_no_args() {
        let program = assert_parses("print();");
        match &program.statements[0] {
            Statement::Expression(Expression::FunctionCall { identifier, arguments }) => {
                assert_eq!(identifier, "print");
                assert_eq!(arguments.len(), 0);
            }
            _ => panic!("Expected function call"),
        }
    }

    #[test]
    fn test_function_call_one_arg() {
        let program = assert_parses("print(42);");
        match &program.statements[0] {
            Statement::Expression(Expression::FunctionCall { identifier, arguments }) => {
                assert_eq!(identifier, "print");
                assert_eq!(arguments.len(), 1);
                match &arguments[0] {
                    Expression::Literal(Literal::Number(n)) => assert_eq!(*n, 42.0),
                    _ => panic!("Expected number argument"),
                }
            }
            _ => panic!("Expected function call"),
        }
    }

    #[test]
    fn test_function_call_multiple_args() {
        let program = assert_parses("add(1, 2, 3);");
        match &program.statements[0] {
            Statement::Expression(Expression::FunctionCall { identifier, arguments }) => {
                assert_eq!(identifier, "add");
                assert_eq!(arguments.len(), 3);
            }
            _ => panic!("Expected function call with multiple args"),
        }
    }

    #[test]
    fn test_function_call_with_expression() {
        let program = assert_parses("print(1 + 2);");
        match &program.statements[0] {
            Statement::Expression(Expression::FunctionCall { identifier, arguments }) => {
                assert_eq!(identifier, "print");
                assert_eq!(arguments.len(), 1);
                match &arguments[0] {
                    Expression::Infix { op, .. } => assert!(matches!(op, BinaryOp::Add)),
                    _ => panic!("Expected addition expression as argument"),
                }
            }
            _ => panic!("Expected function call"),
        }
    }

    #[test]
    fn test_complex_expression() {
        let program = assert_parses("x = -y + z * 2;");
        match &program.statements[0] {
            Statement::Assign { identifier, expression } => {
                assert_eq!(identifier, "x");
                match expression {
                    Expression::Infix { lhs, op, rhs } => {
                        assert!(matches!(op, BinaryOp::Add));
                        // Left side should be -y
                        match lhs.as_ref() {
                            Expression::Prefix { op, .. } => assert!(matches!(op, UnaryOp::Neg)),
                            _ => panic!("Expected negation on left"),
                        }
                        // Right side should be z * 2
                        match rhs.as_ref() {
                            Expression::Infix { op, .. } => assert!(matches!(op, BinaryOp::Mul)),
                            _ => panic!("Expected multiplication on right"),
                        }
                    }
                    _ => panic!("Expected addition expression"),
                }
            }
            _ => panic!("Expected assignment"),
        }
    }

    #[test]
    fn test_multiple_statements() {
        let program = assert_parses("x = 1; y = 2; x + y;");
        assert_eq!(program.statements.len(), 3);
        
        // First statement: x = 1
        match &program.statements[0] {
            Statement::Assign { identifier, .. } => assert_eq!(identifier, "x"),
            _ => panic!("Expected assignment"),
        }
        
        // Second statement: y = 2
        match &program.statements[1] {
            Statement::Assign { identifier, .. } => assert_eq!(identifier, "y"),
            _ => panic!("Expected assignment"),
        }
        
        // Third statement: x + y
        match &program.statements[2] {
            Statement::Expression(Expression::Infix { op, .. }) => {
                assert!(matches!(op, BinaryOp::Add));
            }
            _ => panic!("Expected addition expression"),
        }
    }

    #[test]
    fn test_identifier_with_underscore() {
        let program = assert_parses("my_var = 42;");
        match &program.statements[0] {
            Statement::Assign { identifier, .. } => {
                assert_eq!(identifier, "my_var");
            }
            _ => panic!("Expected assignment with underscore"),
        }
    }

    #[test]
    fn test_whitespace_handling() {
        // Test that whitespace is properly ignored
        let program1 = assert_parses("x=1;");
        let program2 = assert_parses("x = 1;");
        let program3 = assert_parses("x  =  1;");
        
        // All should parse the same
        match (&program1.statements[0], &program2.statements[0], &program3.statements[0]) {
            (
                Statement::Assign { identifier: id1, .. },
                Statement::Assign { identifier: id2, .. },
                Statement::Assign { identifier: id3, .. },
            ) => {
                assert_eq!(id1, id2);
                assert_eq!(id2, id3);
            }
            _ => panic!("All should be assignments"),
        }
    }

    #[test]
    fn test_nested_function_calls() {
        let program = assert_parses("print(add(1, 2));");
        match &program.statements[0] {
            Statement::Expression(Expression::FunctionCall { identifier, arguments }) => {
                assert_eq!(identifier, "print");
                assert_eq!(arguments.len(), 1);
                match &arguments[0] {
                    Expression::FunctionCall { identifier, .. } => {
                        assert_eq!(identifier, "add");
                    }
                    _ => panic!("Expected nested function call"),
                }
            }
            _ => panic!("Expected function call"),
        }
    }

    #[test]
    fn test_chained_operations() {
        let program = assert_parses("1 + 2 + 3;");
        // Left-associative, so should be (1 + 2) + 3
        match &program.statements[0] {
            Statement::Expression(Expression::Infix { lhs, op, rhs }) => {
                assert!(matches!(op, BinaryOp::Add));
                match lhs.as_ref() {
                    Expression::Infix { op, .. } => assert!(matches!(op, BinaryOp::Add)),
                    _ => panic!("Expected nested addition on left"),
                }
            }
            _ => panic!("Expected addition"),
        }
    }
}
