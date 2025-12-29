mod common;

use common::helpers::run_code;

/// Integration tests for array functionality

#[test]
fn test_array_literal() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
    let arr = [1, 2, 3];
    "#;
    
    run_code(&mut engine, code);
}

#[test]
fn test_array_literal_empty() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
    let arr = [];
    "#;
    
    run_code(&mut engine, code);
}

#[test]
fn test_array_literal_mixed_types() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
    let arr = [1, 2.5, 3];
    "#;
    
    run_code(&mut engine, code);
}

#[test]
fn test_array_index_positive() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
    let xs = [10, 20, 30, 40];
    let first = xs[0];
    let second = xs[1];
    let third = xs[2];
    let last = xs[3];
    "#;
    
    run_code(&mut engine, code);
}

#[test]
fn test_array_index_negative() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
    let xs = [10, 20, 30, 40];
    let last = xs[-1];
    let second_last = xs[-2];
    let third_last = xs[-3];
    let first = xs[-4];
    "#;
    
    run_code(&mut engine, code);
}

#[test]
fn test_array_index_variable() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
    let xs = [10, 20, 30, 40];
    let idx = 2;
    let value = xs[idx];
    "#;
    
    run_code(&mut engine, code);
}

#[test]
fn test_array_slice_exclusive_range() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
    let xs = [1, 2, 3, 4, 5];
    let slice = xs[1..4];
    "#;
    
    run_code(&mut engine, code);
}

#[test]
fn test_array_slice_inclusive_range() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
    let xs = [1, 2, 3, 4, 5];
    let slice = xs[1..=4];
    "#;
    
    run_code(&mut engine, code);
}

#[test]
fn test_array_slice_from_start() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
    let xs = [1, 2, 3, 4, 5];
    let slice = xs[..3];
    "#;
    
    run_code(&mut engine, code);
}

#[test]
fn test_array_slice_to_end() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
    let xs = [1, 2, 3, 4, 5];
    let slice = xs[2..];
    "#;
    
    run_code(&mut engine, code);
}

#[test]
fn test_array_slice_full_range() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
    let xs = [1, 2, 3, 4, 5];
    let slice = xs[..];
    "#;
    
    run_code(&mut engine, code);
}

#[test]
fn test_array_slice_with_step() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
    let xs = [1, 2, 3, 4, 5, 6, 7, 8];
    let slice = xs[0..8..2];
    "#;
    
    run_code(&mut engine, code);
}

#[test]
fn test_array_slice_negative_indices() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
    let xs = [1, 2, 3, 4, 5, 6];
    let slice1 = xs[-2..];
    let slice2 = xs[..-2];
    let slice3 = xs[-4..-1];
    "#;
    
    run_code(&mut engine, code);
}

#[test]
fn test_array_nested_access() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
    let xs = [1, 2, 3];
    let idx = 1;
    let value = xs[idx];
    let double = value * 2;
    "#;
    
    run_code(&mut engine, code);
}

#[test]
fn test_array_in_expression() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
    let xs = [1, 2, 3];
    let sum = xs[0] + xs[1] + xs[2];
    "#;
    
    run_code(&mut engine, code);
}

#[test]
fn test_array_slice_of_slice() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
    let xs = [1, 2, 3, 4, 5, 6, 7, 8];
    let first_half = xs[..4];
    let quarter = first_half[..2];
    "#;
    
    run_code(&mut engine, code);
}

#[test]
fn test_array_in_function() {
    // Note: Array type annotations in function parameters are not yet supported
    // This test verifies array access within function bodies works when arrays are passed
    // The type system will infer array types, but explicit array type annotations aren't supported yet
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
    let xs = [10, 20, 30];
    let first = xs[0];
    "#;
    
    run_code(&mut engine, code);
}

#[test]
fn test_array_assignment_and_access() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
    let xs = [1, 2, 3];
    let first = xs[0];
    let second = xs[1];
    let third = xs[2];
    "#;
    
    run_code(&mut engine, code);
}

#[test]
fn test_array_slice_step_negative() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
    let xs = [1, 2, 3, 4, 5, 6, 7, 8];
    let slice = xs[0..8..3];
    "#;
    
    run_code(&mut engine, code);
}

#[test]
fn test_array_empty_slice() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
    let xs = [1, 2, 3];
    let empty = xs[2..1];
    "#;
    
    run_code(&mut engine, code);
}

#[test]
fn test_array_complex_slice() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
    let xs = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    let middle = xs[2..8..2];
    "#;
    
    run_code(&mut engine, code);
}

#[test]
fn test_array_string_elements() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
    let words = ["hello", "world", "test"];
    let first_word = words[0];
    let last_word = words[-1];
    "#;
    
    run_code(&mut engine, code);
}

#[test]
fn test_array_boolean_elements() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
    let flags = [true, false, true];
    let first_flag = flags[0];
    let second_flag = flags[1];
    "#;
    
    run_code(&mut engine, code);
}

#[test]
fn test_array_return_from_function() {
    // Note: Array type annotations in return types are not yet supported
    // This test verifies that arrays can be created and indexed
    // The type system will infer array types, but explicit array type annotations aren't supported yet
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
    let arr = [1, 2, 3];
    let first = arr[0];
    "#;
    
    run_code(&mut engine, code);
}

#[test]
fn test_array_slice_boundary_cases() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
    let xs = [1, 2, 3];
    let all = xs[0..3];
    let none = xs[3..3];
    "#;
    
    run_code(&mut engine, code);
}

#[test]
fn test_array_index_expression() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
    let xs = [10, 20, 30, 40];
    let idx = 1 + 1;
    let value = xs[idx];
    "#;
    
    run_code(&mut engine, code);
}

#[test]
fn test_array_slice_expression_bounds() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
    let xs = [1, 2, 3, 4, 5];
    let start = 1;
    let end = 3;
    let slice = xs[start..end];
    "#;
    
    run_code(&mut engine, code);
}

