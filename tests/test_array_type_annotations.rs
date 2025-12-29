mod common;

use common::helpers::run_code;

#[test]
fn test_array_type_annotation_simple() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
    fn test_array_sum(values: [num]) -> num {
        return values[0] + values[1];
    }
    
    let numbers = [1, 2, 3];
    let result = test_array_sum(numbers)!;
    "#;
    
    run_code(&mut engine, code);
}

#[test]
fn test_array_type_annotation_string() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
    fn process_names(names: [string]) -> string {
        return names[0];
    }
    
    let students = ["Alice", "Bob"];
    let first = process_names(students)!;
    "#;
    
    run_code(&mut engine, code);
}

#[test]
fn test_nested_array_type() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
    fn get_value(matrix: [[num]]) -> num {
        return 42;
    }
    
    let matrix = [[1, 2], [3, 4]];
    let val = get_value(matrix)!;
    "#;
    
    run_code(&mut engine, code);
}

