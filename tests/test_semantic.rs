mod common;

use CantaLoopRS::{
    Engine,
    parse_program,
    FunctionSignature,
    ValueKind,
};

#[test]
fn test_semantic_simple_program() {
    let mut engine = common::helpers::create_test_engine();
    let program = parse_program("let x = 42;").unwrap();
    
    // Test semantic analysis
    // Note: This requires access to HirBuilder, which might need to be made public
    // or we test through the Engine::run method
}

#[test]
fn test_semantic_function_definition() {
    let mut engine = common::helpers::create_test_engine();
    let program = parse_program(r#"
fn add(a: num, b: num) -> num {
    return a + b;
}
"#).unwrap();
    
    // Test that function is properly analyzed
}

#[test]
fn test_semantic_variable_reference() {
    let mut engine = common::helpers::create_test_engine();
    let program = parse_program(r#"
let x = 10;
let y = x + 5;
"#).unwrap();
    
    // Test that variable references are resolved
}

#[test]
fn test_semantic_type_checking() {
    let mut engine = common::helpers::create_test_engine();
    
    // Test type checking for arithmetic operations
    let program = parse_program("1 + 2;").unwrap();
    
    // Test type checking for string operations
    let program = parse_program(r#""hello" + "world";"#).unwrap();
}

#[test]
fn test_semantic_function_call() {
    let mut engine = common::helpers::create_test_engine();
    let program = parse_program(r#"
fn test() {
    let x = 1;
}

test();
"#).unwrap();
    
    // Test that function calls are properly resolved
}

#[test]
fn test_semantic_scope_handling() {
    let mut engine = common::helpers::create_test_engine();
    let program = parse_program(r#"
let x = 10;

fn test() {
    let y = 20;
    return x + y;
}
"#).unwrap();
    
    // Test that scopes are handled correctly
}

#[test]
fn test_semantic_error_undefined_variable() {
    let mut engine = common::helpers::create_test_engine();
    let program = parse_program("let x = undefined_var;").unwrap();
    
    // Test that semantic analysis catches undefined variables
    // This might be an error case
}

#[test]
fn test_semantic_error_wrong_argument_count() {
    let mut engine = common::helpers::create_test_engine();
    let program = parse_program(r#"
fn add(a: num, b: num) -> num {
    return a + b;
}

add(1);  // Wrong number of arguments
"#).unwrap();
    
    // Test that semantic analysis catches wrong argument counts
}

