use CantaLoopRS::core::parser::parse_program;
use CantaLoopRS::core::ast::Statement;

/// Test parsing of function declarations with and without `pub`
#[test]
fn test_fn_without_pub_parses() {
    let src = r#"
mod test;

use std.print;

fn test_function() -> num {
    return 42;
}
"#;
    
    let result = parse_program(src);
    assert!(result.is_ok(), "Function without pub should parse: {:?}", result.err());
    
    let program = result.unwrap();
    let mut found_function = false;
    for block in &program.blocks {
        for stmt in &block.statements {
            if let Statement::FunctionDeclaration { identifier, .. } = stmt {
                if identifier == "test_function" {
                    found_function = true;
                }
            }
        }
    }
    assert!(found_function, "Should find test_function in AST");
}

/// Test parsing of function declarations with `pub`
#[test]
fn test_pub_fn_parses() {
    let src = r#"
mod test;

use std.print;

pub fn test_function() -> num {
    return 42;
}
"#;
    
    let result = parse_program(src);
    assert!(result.is_ok(), "Function with pub should parse: {:?}", result.err());
    
    let program = result.unwrap();
    let mut found_function = false;
    let mut is_public = false;
    for block in &program.blocks {
        for stmt in &block.statements {
            if let Statement::FunctionDeclaration { identifier, pub_visibility, .. } = stmt {
                if identifier == "test_function" {
                    found_function = true;
                    is_public = *pub_visibility;
                }
            }
        }
    }
    assert!(found_function, "Should find test_function in AST");
    assert!(is_public, "Function should be marked as public");
}

/// Test parsing of function declarations with `pub` after multiple use statements
/// This mimics the students.mln scenario
#[test]
fn test_pub_fn_after_multiple_use_statements() {
    let src = r#"
mod test;

use grades.calculate_average;
use grades.calculate_final_grade;
use grades.grade_statistics;
use grades.highest_grade;
use grades.lowest_grade;
use std.array_length;

pub fn letterGrade(grade: num) -> string {
    if grade >= 90 {
        return "A";
    }
    return "F";
}
"#;
    
    let result = parse_program(src);
    assert!(result.is_ok(), "pub fn after multiple use statements should parse: {:?}", result.err());
    
    let program = result.unwrap();
    let mut found_function = false;
    for block in &program.blocks {
        for stmt in &block.statements {
            if let Statement::FunctionDeclaration { identifier, .. } = stmt {
                if identifier == "letterGrade" {
                    found_function = true;
                }
            }
        }
    }
    assert!(found_function, "Should find letterGrade in AST");
}

/// Test parsing of function declarations without `pub` after multiple use statements
/// This mimics the grades.mln scenario (which works)
#[test]
fn test_fn_without_pub_after_multiple_use_statements() {
    let src = r#"
mod test;

use std.array_length;
use math.sum;

fn calculate_average(grades: [num]) -> num {
    let len = array_length(grades)!;
    if len == 0 {
        return 0;
    }
    let total = grades |> sum;
    return total / len;
}
"#;
    
    let result = parse_program(src);
    assert!(result.is_ok(), "fn without pub after use statements should parse: {:?}", result.err());
    
    let program = result.unwrap();
    let mut found_function = false;
    for block in &program.blocks {
        for stmt in &block.statements {
            if let Statement::FunctionDeclaration { identifier, .. } = stmt {
                if identifier == "calculate_average" {
                    found_function = true;
                }
            }
        }
    }
    assert!(found_function, "Should find calculate_average in AST");
}

/// Test the exact content from students.mln (first function)
#[test]
fn test_students_mln_exact_content() {
    let src = r#"
mod students;

use grades.calculate_average;
use grades.calculate_final_grade;
use grades.grade_statistics;
use grades.highest_grade;
use grades.lowest_grade;
use std.array_length;

// Convert numeric grade to letter grade
pub fn letterGrade(grade: num) -> string {
    if grade >= 90 {
        return "A";
    }
    if grade >= 80 {
        return "B";
    }
    if grade >= 70 {
        return "C";
    }
    if grade >= 60 {
        return "D";
    }
    return "F";
}
"#;
    
    let result = parse_program(src);
    assert!(result.is_ok(), "students.mln exact content should parse: {:?}", result.err());
    
    let program = result.unwrap();
    let mut found_function = false;
    for block in &program.blocks {
        for stmt in &block.statements {
            if let Statement::FunctionDeclaration { identifier, .. } = stmt {
                if identifier == "letterGrade" {
                    found_function = true;
                }
            }
        }
    }
    assert!(found_function, "Should find letterGrade in AST");
}

/// Test the exact content from grades.mln (first function)
#[test]
fn test_grades_mln_exact_content() {
    let src = r#"
mod grades;

use std.array_length;
use math.sum;

// Calculate the average of a list of grades
fn calculate_average(grades: [num]) -> num {
    let len = array_length(grades)!;
    if len == 0 {
        return 0;
    }
    let total = grades |> sum;
    return total / len;
}
"#;
    
    let result = parse_program(src);
    assert!(result.is_ok(), "grades.mln exact content should parse: {:?}", result.err());
    
    let program = result.unwrap();
    let mut found_function = false;
    for block in &program.blocks {
        for stmt in &block.statements {
            if let Statement::FunctionDeclaration { identifier, .. } = stmt {
                if identifier == "calculate_average" {
                    found_function = true;
                }
            }
        }
    }
    assert!(found_function, "Should find calculate_average in AST");
}

/// Compare AST structure between working and non-working cases
#[test]
fn test_compare_ast_structure() {
    let working_src = r#"
mod test;
use std.print;
fn test() -> num { return 42; }
"#;
    
    let non_working_src = r#"
mod test;
use std.print;
pub fn test() -> num { return 42; }
"#;
    
    let working_result = parse_program(working_src);
    let non_working_result = parse_program(non_working_src);
    
    assert!(working_result.is_ok(), "Working case should parse");
    assert!(non_working_result.is_ok(), "Non-working case should also parse in isolation: {:?}", non_working_result.err());
    
    let working_program = working_result.unwrap();
    let non_working_program = non_working_result.unwrap();
    
    // Compare structure
    assert_eq!(working_program.blocks.len(), non_working_program.blocks.len(), "Should have same number of blocks");
    
    let working_functions: Vec<&str> = working_program.blocks.iter()
        .flat_map(|b| &b.statements)
        .filter_map(|s| {
            if let Statement::FunctionDeclaration { identifier, .. } = s {
                Some(identifier.as_str())
            } else {
                None
            }
        })
        .collect();
    
    let non_working_functions: Vec<&str> = non_working_program.blocks.iter()
        .flat_map(|b| &b.statements)
        .filter_map(|s| {
            if let Statement::FunctionDeclaration { identifier, .. } = s {
                Some(identifier.as_str())
            } else {
                None
            }
        })
        .collect();
    
    assert_eq!(working_functions, non_working_functions, "Should have same functions");
}

