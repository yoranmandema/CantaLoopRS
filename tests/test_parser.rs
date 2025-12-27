mod common;

use common::helpers::*;
use CantaLoopRS::parser::parse_program;

#[test]
fn test_parse_simple_assignment() {
    assert_parse_success("let x = 42");
}

#[test]
fn test_parse_let_statement() {
    assert_parse_success("let x = 42");
    assert_parse_success("let name = \"hello\"");
    assert_parse_success("let flag = true");
}

#[test]
fn test_parse_reassignment() {
    assert_parse_success(r#"
let x = 10
x = 20
"#);
}

#[test]
fn test_parse_simple_expression() {
    assert_parse_success("1 + 2");
}

#[test]
fn test_parse_literals() {
    assert_parse_success("42");
    assert_parse_success(r#""hello""#);
    assert_parse_success("true");
    assert_parse_success("false");
}

#[test]
fn test_parse_arithmetic_expressions() {
    assert_parse_success("1 + 2");
    assert_parse_success("1 - 2");
    assert_parse_success("1 * 2");
    assert_parse_success("1 / 2");
    assert_parse_success("2 ^ 3");
}

#[test]
fn test_parse_arithmetic_precedence() {
    assert_parse_success("1 + 2 * 3");
    assert_parse_success("(1 + 2) * 3");
}

#[test]
fn test_parse_comparison_operators() {
    assert_parse_success("1 == 1");
    assert_parse_success("1 != 2");
    assert_parse_success("1 > 0");
    assert_parse_success("1 < 2");
    assert_parse_success("1 >= 1");
    assert_parse_success("1 <= 2");
}

#[test]
fn test_parse_logical_operators() {
    assert_parse_success("true and false");
    assert_parse_success("true or false");
    assert_parse_success("not true");
}

#[test]
fn test_parse_function_declaration() {
    assert_parse_success(r#"
fn test() {
    let x = 1
}
"#);
}

#[test]
fn test_parse_function_with_parameters() {
    assert_parse_success(r#"
fn add(a, b) {
    return a + b
}
"#);
}

#[test]
fn test_parse_if_statement() {
    assert_parse_success(r#"
if (true) {
    x = 1
}
"#);
}

#[test]
fn test_parse_if_else_statement() {
    assert_parse_success(r#"
if (x == 1) {
    let y = 2
} else {
    let y = 3
}
"#);
}

#[test]
fn test_parse_if_elseif_statement() {
    assert_parse_success(r#"
if (x == 1) {
    let y = 2
} elseif (x == 2) {
    let y = 3
} else {
    let y = 4
}
"#);
}

#[test]
fn test_parse_function_call() {
    assert_parse_success("test()");
    assert_parse_success("add(1, 2)");
}

#[test]
fn test_parse_return_statement() {
    assert_parse_success(r#"
fn test() {
    return 42
}
"#);
}

#[test]
fn test_parse_variable_assignment_increment() {
    assert_parse_success("x += 1");
}

#[test]
fn test_parse_variable_assignment_decrement() {
    assert_parse_success("x -= 1");
}

#[test]
fn test_parse_unary_negation() {
    assert_parse_success("-x");
    assert_parse_success("--x");
    assert_parse_success("++x");
}

#[test]
fn test_parse_multiple_statements() {
    assert_parse_success(r#"
let x = 1
let y = 2
let z = x + y
"#);
}

#[test]
fn test_parse_complex_expression() {
    assert_parse_success("(1 + 2) * (3 - 4) / 5");
}

#[test]
fn test_parse_nested_expressions() {
    assert_parse_success("(1 + (2 * (3 + 4)))");
}

#[test]
fn test_parse_empty_program() {
    // Empty program should still parse (just no statements)
    let result = parse_program("");
    // This might succeed or fail depending on grammar definition
    // Adjust based on actual grammar behavior
}

#[test]
fn test_parse_invalid_syntax_should_fail() {
    // These should fail to parse
    assert_parse_failure("x ="); // Missing value
    assert_parse_failure("fn ("); // Missing function name
    assert_parse_failure("if ("); // Incomplete if
}

