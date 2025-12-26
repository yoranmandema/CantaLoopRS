use crate::parser::{Statement, parse_program};

mod parser;

#[macro_use]
extern crate lazy_static;

fn main() {
    let input = std::fs::read_to_string("examples/helloworld.mln")
        .expect("Failed to read examples/helloworld.mln");

    let res = parse_program(&input).unwrap();

    println!("{:?}", res);

    for statement in res.statements {
        match statement {
            Statement::Assign {
                identifier,
                expression,
            } => {
                println!("Assigning {} to {:?}", identifier, expression);
            }
            Statement::AssignIncrement {
                identifier,
                expression,
            } => {
                println!("Incrementing {} by {:?}", identifier, expression);
            }

            Statement::AssignDecrement {
                identifier,
                expression,
            } => {
                println!("Decrementing {} by {:?}", identifier, expression);
            }
            Statement::Expression(expression) => {
                println!("Expression: {:?}", expression);
            }
        }
    }
}
