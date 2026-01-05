use cantaloop::core::parser::{CantaLoopParser, Rule};
use pest::Parser;

/// Test if the specific import paths are causing the issue
#[test]
fn test_import_paths_matter() {
    // Test with simple import paths (works)
    let simple = r#"mod test;
use a.b;
use c.d;
use e.f;
pub fn test() -> num { return 42; }
"#;
    
    // Test with grades.* import paths (might fail?)
    let grades_paths = r#"mod test;
use grades.calculate_average;
use grades.calculate_final_grade;
use grades.grade_statistics;
use grades.highest_grade;
use grades.lowest_grade;
use std.array_length;
pub fn test() -> num { return 42; }
"#;
    
    // Test with comment before function
    let with_comment = r#"mod test;
use grades.calculate_average;
use grades.calculate_final_grade;
use grades.grade_statistics;
use grades.highest_grade;
use grades.lowest_grade;
use std.array_length;

// Convert numeric grade to letter grade
pub fn test() -> num { return 42; }
"#;
    
    println!("\n=== Testing simple import paths ===");
    let simple_parse = CantaLoopParser::parse(Rule::program, simple);
    match simple_parse {
        Ok(_) => println!("✓ Parsed"),
        Err(e) => println!("✗ Failed: {:?}", e),
    }
    
    println!("\n=== Testing grades.* import paths ===");
    let grades_parse = CantaLoopParser::parse(Rule::program, grades_paths);
    match grades_parse {
        Ok(_) => println!("✓ Parsed"),
        Err(e) => {
            println!("✗ Failed: {:?}", e);
            println!("  Location: {:?}", e.location);
        }
    }
    
    println!("\n=== Testing with comment before function ===");
    let comment_parse = CantaLoopParser::parse(Rule::program, with_comment);
    match comment_parse {
        Ok(_) => println!("✓ Parsed"),
        Err(e) => {
            println!("✗ Failed: {:?}", e);
            println!("  Location: {:?}", e.location);
        }
    }
    
    // Test parsing just the use statements to see if they parse correctly
    println!("\n=== Testing use statement parsing ===");
    let use_statement = "use grades.calculate_average;";
    let use_parse = CantaLoopParser::parse(Rule::use_statement, use_statement);
    match use_parse {
        Ok(pairs) => {
            println!("✓ use_statement parsed");
            for pair in pairs {
                println!("  Rule: {:?}, Text: {:?}", pair.as_rule(), pair.as_str());
            }
        }
        Err(e) => {
            println!("✗ use_statement failed: {:?}", e);
        }
    }
    
    // Test if the issue is with the specific identifier "letterGrade"
    println!("\n=== Testing with letterGrade identifier ===");
    let with_letter_grade = r#"mod test;
use grades.calculate_average;
use grades.calculate_final_grade;
use grades.grade_statistics;
use grades.highest_grade;
use grades.lowest_grade;
use std.array_length;

pub fn letterGrade(grade: num) -> string {
    return "A";
}
"#;
    
    let letter_grade_parse = CantaLoopParser::parse(Rule::program, with_letter_grade);
    match letter_grade_parse {
        Ok(_) => println!("✓ Parsed"),
        Err(e) => {
            println!("✗ Failed: {:?}", e);
            println!("  Location: {:?}", e.location);
        }
    }
}

