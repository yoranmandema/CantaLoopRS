mod common;

use CantaLoopRS::{
    ast::{Expression, Statement, Literal, BinaryOp, UnaryOp},
    parser::parse_program,
};

#[test]
fn test_ast_simple_literal() {
    let program = parse_program("42;").unwrap();
    assert!(!program.blocks.is_empty());
    // Verify AST structure based on actual implementation
}

#[test]
fn test_ast_assignment_statement() {
    let program = parse_program("let x = 42;").unwrap();
    assert!(!program.blocks.is_empty());
    
    if let Some(block) = program.blocks.first() {
        assert!(!block.statements.is_empty());
        // Verify assignment statement structure
    }
}

#[test]
fn test_ast_binary_expression() {
    let program = parse_program("1 + 2;").unwrap();
    // Verify binary expression structure
}

#[test]
fn test_ast_function_declaration() {
    let program = parse_program(r#"
fn test() {
    let x = 1;
}
"#).unwrap();
    // Verify function declaration structure
}

#[test]
fn test_ast_if_statement() {
    let program = parse_program(r#"
if (true) {
    x = 1;
}
"#).unwrap();
    // Verify if statement structure
}

#[test]
fn test_ast_nested_expressions() {
    let program = parse_program("(1 + 2) * 3;").unwrap();
    // Verify nested expression structure
}

#[test]
fn test_ast_string_literal() {
    let program = parse_program(r#""hello world";"#).unwrap();
    // Verify string literal structure
}

#[test]
fn test_ast_boolean_literal() {
    let program = parse_program("true;").unwrap();
    // Verify boolean literal structure
}

#[test]
fn test_ast_function_with_parameters() {
    let program = parse_program(r#"
fn add(a, b) {
    return a + b;
}
"#).unwrap();
    // Verify function with parameters structure
}

#[test]
fn test_ast_complex_program() {
    let program = parse_program(r#"
let x = 10;
let y = 20;

fn add(a, b) {
    return a + b;
}

let result = add(x, y);
"#).unwrap();
    
    // Verify complex program structure
    assert!(!program.blocks.is_empty());
}

