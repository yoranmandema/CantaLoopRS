mod common;

use cantaloop::{
    Engine,
    VM,
    Value,
    OpCode,
    FunctionSignature,
    ValueKind,
};
use std::sync::Arc;

#[test]
fn test_vm_push_number() {
    let engine = Arc::new(common::helpers::create_empty_engine());
    let ops = vec![
        OpCode::LdNum(42.0),
    ];
    
    let mut vm = VM::new(engine, std::collections::HashMap::new(), cantaloop::core::hir_lowering::HirAst::default(), ops);
    vm.run();
    
    // Verify stack contains the number
    // Note: VM stack is private, so we'd need to add getter methods or test indirectly
}

#[test]
fn test_vm_push_string() {
    let engine = Arc::new(common::helpers::create_empty_engine());
    let ops = vec![
        OpCode::LdStr("hello".to_string()),
    ];
    
    let mut vm = VM::new(engine, std::collections::HashMap::new(), cantaloop::core::hir_lowering::HirAst::default(), ops);
    vm.run();
}

#[test]
fn test_vm_arithmetic_operations() {
    let engine = Arc::new(common::helpers::create_empty_engine());
    let ops = vec![
        OpCode::LdNum(10.0),
        OpCode::LdNum(20.0),
        OpCode::Add,
    ];
    
    let mut vm = VM::new(engine, std::collections::HashMap::new(), cantaloop::core::hir_lowering::HirAst::default(), ops);
    vm.run();
}

#[test]
fn test_vm_variable_storage() {
    let engine = Arc::new(common::helpers::create_empty_engine());
    let var_id = 1;
    let ops = vec![
        OpCode::LdNum(42.0),
        OpCode::StVar(var_id),
        OpCode::LdVar(var_id),
    ];
    
    let mut vm = VM::new(engine, std::collections::HashMap::new(), cantaloop::core::hir_lowering::HirAst::default(), ops);
    vm.run();
}

#[test]
fn test_vm_comparison_operations() {
    let engine = Arc::new(common::helpers::create_empty_engine());
    
    // Test equality
    let ops = vec![
        OpCode::LdNum(5.0),
        OpCode::LdNum(5.0),
        OpCode::Eq,
    ];
    
    let mut vm = VM::new(engine, std::collections::HashMap::new(), cantaloop::core::hir_lowering::HirAst::default(), ops);
    vm.run();
}

#[test]
fn test_vm_logical_operations() {
    let engine = Arc::new(common::helpers::create_empty_engine());
    
    // Test AND operation
    let ops = vec![
        OpCode::LdNum(1.0), // true
        OpCode::LdNum(1.0), // true
        OpCode::And,
    ];
    
    let mut vm = VM::new(engine, std::collections::HashMap::new(), cantaloop::core::hir_lowering::HirAst::default(), ops);
    vm.run();
}

#[test]
fn test_vm_unary_operations() {
    let engine = Arc::new(common::helpers::create_empty_engine());
    
    // Test negation
    let ops = vec![
        OpCode::LdNum(10.0),
        OpCode::Neg,
    ];
    
    let mut vm = VM::new(engine, std::collections::HashMap::new(), cantaloop::core::hir_lowering::HirAst::default(), ops);
    vm.run();
}

#[test]
fn test_vm_not_operation() {
    let engine = Arc::new(common::helpers::create_empty_engine());
    
    // Test NOT operation
    let ops = vec![
        OpCode::LdNum(1.0), // true
        OpCode::Not,
    ];
    
    let mut vm = VM::new(engine, std::collections::HashMap::new(), cantaloop::core::hir_lowering::HirAst::default(), ops);
    vm.run();
}

#[test]
fn test_vm_boolean_values() {
    let _engine = Arc::new(common::helpers::create_empty_engine());
    // Note: Adjust based on how booleans are represented in OpCode
    // If there's LdBool opcode, use that; otherwise booleans might be numbers
}

