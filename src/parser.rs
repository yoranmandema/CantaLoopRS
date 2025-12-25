use pest::Parser as PestParser;
use pest::iterators::Pair;
use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "src/grammar/grammar.pest"]
pub struct CantaLoopParser;

#[derive(Debug)]
pub struct Program {
    pub statements: Vec<Statement>,
}

#[derive(Debug)]
pub enum Statement {
    Assign { identifier: String },
}

#[derive(Debug)]
pub enum Expression {
    Literal(Literal)
}

#[derive(Debug)]
pub enum Literal {
    String(String),
    Number(f64)
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

fn build_expression (pair: Pair<Rule>) -> Result<Expression, pest::error::Error<Rule>> {
    let expression_inner = pair.into_inner().next().unwrap();

    match expression_inner.as_rule() {
        Rule::
        _ => unreachable!(),
    }
}

fn build_statement(pair: Pair<Rule>) -> Result<Statement, pest::error::Error<Rule>> {
    let statement_inner = pair.into_inner().next().unwrap();

    match statement_inner.as_rule() {
        Rule::assign_statement => {
            let mut inner = statement_inner.into_inner();
            let identifier = inner.next().unwrap();
            let expression = inner.next().unwrap();

            println!("identifier: {}", identifier);
            println!("expression: {}", expression);

            Ok(Statement::Assign {
                identifier: identifier.to_string(),
            })
        }
        _ => unreachable!(),
    }
}

pub fn parse_program(src: &str) -> Result<Program, pest::error::Error<Rule>> {
    let mut pairs = CantaLoopParser::parse(Rule::program, src)?;
    let program = pairs.next().unwrap();
    build_program(program)
}
