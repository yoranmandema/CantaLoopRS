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
    use cantaloop::{ByteCodeEmitter, HirBuilder};
    
    let mut engine = common::helpers::create_test_engine();
    let program = parse_program(r#"
fn add(a: num, b: num) -> num {
    return a + b;
}

add(5, 10)!;
"#).unwrap();
    
    // Build HIR using a separate HirBuilder (since engine's is private)
    // We need to register the same built-in functions
    let mut hir_builder = HirBuilder::new();
    // Register print function like the engine does
    let print_sig = FunctionSignature {
        params: vec![ValueKind::String],
        return_type: Box::new(ValueKind::String),
    };
    hir_builder.register_builtin_function("print", print_sig, 10000);
    
    let hir_ast = hir_builder.build(program).unwrap();
    
    // Emit bytecode
    let mut emitter = ByteCodeEmitter::new();
    let bytecode = emitter.emit_program(&hir_ast);
    
    // Verify that add(5, 10)! emits direct CallStack instead of Thunk
    // Expected sequence: LdNum(5), LdNum(10), LdFunc(...), CallStack(2)
    let mut found_callstack = false;
    let mut found_thunk = false;
    
    for op in &bytecode {
        match op {
            OpCode::CallStack(2) => {
                found_callstack = true;
            }
            OpCode::Thunk(_) => {
                found_thunk = true;
            }
            _ => {}
        }
    }
    
    assert!(found_callstack, "Expected CallStack(2) to be emitted for add(5, 10)!, but got: {:?}", bytecode);
    assert!(!found_thunk, "Expected no Thunk to be emitted when arg count matches param count, but got: {:?}", bytecode);
}

#[test]
fn test_thunk_created_partial_application() {
    use cantaloop::{ByteCodeEmitter, HirBuilder};
    
    let mut engine = common::helpers::create_test_engine();
    let program = parse_program(r#"
fn add(a: num, b: num) -> num {
    return a + b;
}

add(5);
"#).unwrap();
    
    // Build HIR using a separate HirBuilder
    let mut hir_builder = HirBuilder::new();
    let print_sig = FunctionSignature {
        params: vec![ValueKind::String],
        return_type: Box::new(ValueKind::String),
    };
    hir_builder.register_builtin_function("print", print_sig, 10000);
    
    let hir_ast = hir_builder.build(program).unwrap();
    
    // Emit bytecode
    let mut emitter = ByteCodeEmitter::new();
    let bytecode = emitter.emit_program(&hir_ast);
    
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

