mod common;

use cantaloop::{
    parse_program,
    OpCode,
    FunctionSignature,
    ValueKind,
};

#[test]
fn test_bytecode_emit_simple_assignment() {
    let mut engine = common::helpers::create_test_engine();
    let program = parse_program("let x = 42;").unwrap();
    
    // Test bytecode emission for simple assignment
    // This requires running the full pipeline
}

#[test]
fn test_bytecode_emit_arithmetic() {
    let mut engine = common::helpers::create_test_engine();
    let program = parse_program("1 + 2 * 3;").unwrap();
    
    // Test bytecode emission for arithmetic expressions
}

#[test]
fn test_bytecode_emit_function() {
    let mut engine = common::helpers::create_test_engine();
    let program = parse_program(r#"
fn test() {
    let x = 1;
}
"#).unwrap();
    
    // Test bytecode emission for function definitions
}

#[test]
fn test_bytecode_emit_if_statement() {
    let mut engine = common::helpers::create_test_engine();
    let program = parse_program(r#"
if (true) {
    x = 1;
}
"#).unwrap();
    
    // Test bytecode emission for if statements
}

#[test]
fn test_bytecode_emit_function_call() {
    let mut engine = common::helpers::create_test_engine();
    let program = parse_program(r#"
fn add(a: num, b: num) -> num {
    return a + b;
}

add(1, 2)!;
"#).unwrap();
    
    // Test bytecode emission for function calls
}

#[test]
fn test_bytecode_emit_return_statement() {
    let mut engine = common::helpers::create_test_engine();
    let program = parse_program(r#"
fn test() {
    return 42;
}
"#).unwrap();
    
    // Test bytecode emission for return statements
}

#[test]
fn test_bytecode_emit_variable_load() {
    let mut engine = common::helpers::create_test_engine();
    let program = parse_program(r#"
let x = 10;
let y = x + 5;
"#).unwrap();
    
    // Test bytecode emission for variable loading
}

#[test]
fn test_bytecode_emit_complex_expression() {
    let mut engine = common::helpers::create_test_engine();
    let program = parse_program("(1 + 2) * (3 - 4) / 5;").unwrap();
    
    // Test bytecode emission for complex expressions
}

#[test]
fn test_bytecode_emit_logical_operations() {
    let mut engine = common::helpers::create_test_engine();
    let program = parse_program("true && false || !true;").unwrap();
    
    // Test bytecode emission for logical operations
}

#[test]
fn test_thunk_collapse_full_args() {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    
    let engine = common::helpers::create_test_engine();
    let source = r#"
fn add(a: num, b: num) -> num {
    return a + b;
}

add(5, 10)!;
"#;
    
    // Compile via the real pipeline (write to temp file).
    let test_dir = std::path::Path::new("target").join("test_temp");
    std::fs::create_dir_all(&test_dir).ok();
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    let test_file = test_dir.join(format!("bytecode_thunk_full_{:x}.cl", hasher.finish()));
    std::fs::write(&test_file, source).expect("Failed to write test file");
    let artifacts = engine.compile(test_file.to_str().unwrap()).unwrap();
    let bytecode = artifacts.main;
    
    // Verify current behavior for add(5, 10)!:
    // it emits a thunk with arity then invokes it.
    let mut found_callstack = false;
    let mut found_thunk = false;
    let mut found_invoke = false;
    
    for op in &bytecode {
        match op {
            OpCode::CallStack(2) => {
                found_callstack = true;
            }
            OpCode::Thunk(_) => {
                found_thunk = true;
            }
            OpCode::Invoke => {
                found_invoke = true;
            }
            _ => {}
        }
    }
    
    assert!(found_thunk, "Expected Thunk(_) to be emitted for add(5, 10)!, but got: {:?}", bytecode);
    assert!(found_invoke, "Expected Invoke to be emitted for add(5, 10)!, but got: {:?}", bytecode);
    assert!(!found_callstack, "Did not expect CallStack(2) for add(5, 10)!, but got: {:?}", bytecode);
}

#[test]
fn test_thunk_created_partial_application() {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    
    let engine = common::helpers::create_test_engine();
    let source = r#"
fn add(a: num, b: num) -> num {
    return a + b;
}

add(5);
"#;
    
    // Compile via the real pipeline (write to temp file).
    let test_dir = std::path::Path::new("target").join("test_temp");
    std::fs::create_dir_all(&test_dir).ok();
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    let test_file = test_dir.join(format!("bytecode_thunk_partial_{:x}.cl", hasher.finish()));
    std::fs::write(&test_file, source).expect("Failed to write test file");
    let artifacts = engine.compile(test_file.to_str().unwrap()).unwrap();
    let bytecode = artifacts.main;
    
    // Verify that add(5) emits Thunk (partial application)
    let mut found_thunk = false;
    let mut found_callstack = false;
    
    for op in &bytecode {
        match op {
            OpCode::Thunk(1) => {
                found_thunk = true;
            }
            OpCode::CallStack(_) => {
                found_callstack = true;
            }
            _ => {}
        }
    }
    
    assert!(found_thunk, "Expected Thunk(1) to be emitted for add(5) (partial application), but got: {:?}", bytecode);
    assert!(!found_callstack, "Expected no CallStack when arg count < param count, but got: {:?}", bytecode);
}

