mod common;

use CantaLoopRS::{
    engine::Engine,
    semantic_analyser::{FunctionSignature, ValueKind},
};

/// Integration tests that test the full pipeline from parsing to execution

#[test]
fn test_integration_simple_assignment() {
    let mut engine = common::helpers::create_test_engine();
    let code = "let x = 42;";
    
    // Should complete without panicking
    engine.run(code);
}

#[test]
fn test_integration_simple_expression() {
    let mut engine = common::helpers::create_test_engine();
    let code = "1 + 2;";
    
    engine.run(code);
}

#[test]
fn test_integration_function_declaration_and_call() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
fn greet() {
    print("Hello");
}

greet();
"#;
    
    engine.run(code);
}

#[test]
fn test_integration_function_with_parameters() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
fn add(a, b) {
    return a + b;
}

let result = add(10, 20);
"#;
    
    engine.run(code);
}

#[test]
fn test_integration_if_statement() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
let x = 10;
if (x > 5) {
    print("x is greater than 5");
}
"#;
    
    engine.run(code);
}

#[test]
fn test_integration_if_else_statement() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
let x = 3;
if (x > 5) {
    print("x is greater than 5");
} else {
    print("x is not greater than 5");
}
"#;
    
    engine.run(code);
}

#[test]
fn test_integration_variable_operations() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
let x = 10;
let y = 20;
let z = x + y;
"#;
    
    engine.run(code);
}

#[test]
fn test_integration_arithmetic_expressions() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
let x = 1 + 2 * 3;
let y = (1 + 2) * 3;
let z = 2 ^ 3;
"#;
    
    engine.run(code);
}

#[test]
fn test_integration_comparison_operations() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
let x = 5 == 5;
let y = 5 != 10;
let z = 10 > 5;
let w = 5 < 10;
"#;
    
    engine.run(code);
}

#[test]
fn test_integration_logical_operations() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
let x = true and false;
let y = true or false;
let z = not true;
"#;
    
    engine.run(code);
}

#[test]
fn test_integration_complex_program() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
let x = 10;

fn calculate(a, b) {
    return a * b + x;
}

let result = calculate(5, 3);

if (result > 20) {
    print("Result is large");
} else {
    print("Result is small");
}
"#;
    
    engine.run(code);
}

#[test]
fn test_integration_multiple_functions() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
fn add(a, b) {
    return a + b;
}

fn multiply(a, b) {
    return a * b;
}

let result1 = add(5, 3);
let result2 = multiply(4, 2);
let final = add(result1, result2);
"#;
    
    engine.run(code);
}

#[test]
fn test_integration_variable_increment() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
let x = 10;
x += 5;
"#;
    
    engine.run(code);
}

#[test]
fn test_integration_variable_decrement() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
let x = 10;
x -= 3;
"#;
    
    engine.run(code);
}

#[test]
fn test_integration_nested_conditions() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
let x = 10;
let y = 20;

if (x > 5) {
    if (y > 15) {
        print("Both conditions met");
    }
}
"#;
    
    engine.run(code);
}

#[test]
fn test_integration_return_statement() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
fn get_value() {
    return 42;
}

let value = get_value();
"#;
    
    engine.run(code);
}

