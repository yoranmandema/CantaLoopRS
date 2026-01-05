use cantaloop::core::parser::{CantaLoopParser, Rule};
use pest::Parser;

/// Detailed test to see exactly what Pest is matching
#[test]
fn test_detailed_parsing_analysis() {
    // Test with exact students.mln structure
    let students_mln = r#"mod students;

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
    return "F";
}
"#;
    
    // Test with exact grades.mln structure (working)
    let grades_mln = r#"mod grades;

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
    
    println!("\n=== Parsing students.mln (should fail) ===");
    let students_parse = CantaLoopParser::parse(Rule::program, students_mln);
    match students_parse {
        Ok(mut pairs) => {
            println!("✓ Parsed successfully (unexpected!)");
            let program = pairs.next().unwrap();
            println!("  Program span: {:?}", program.as_span());
            // Walk through inner pairs
            for inner in program.into_inner() {
                println!("  Inner rule: {:?}, span: {:?}", inner.as_rule(), inner.as_span());
                if inner.as_rule() == Rule::block {
                    for block_inner in inner.into_inner() {
                        println!("    Block inner: {:?}, span: {:?}", block_inner.as_rule(), block_inner.as_span());
                    }
                }
            }
        }
        Err(e) => {
            println!("✗ Parse failed (expected): {:?}", e);
            println!("  Location: {:?}", e.location);
            println!("  Line/col: {:?}", e.line_col);
            if let pest::error::ErrorVariant::ParsingError { positives, negatives } = &e.variant {
                println!("  Expected: {:?}", positives);
            }
        }
    }
    
    println!("\n=== Parsing grades.mln (should work) ===");
    let grades_parse = CantaLoopParser::parse(Rule::program, grades_mln);
    match grades_parse {
        Ok(mut pairs) => {
            println!("✓ Parsed successfully");
            let program = pairs.next().unwrap();
            println!("  Program span: {:?}", program.as_span());
            // Walk through inner pairs
            for inner in program.into_inner() {
                println!("  Inner rule: {:?}, span: {:?}", inner.as_rule(), inner.as_span());
                if inner.as_rule() == Rule::block {
                    for block_inner in inner.into_inner() {
                        println!("    Block inner: {:?}, span: {:?}", block_inner.as_rule(), block_inner.as_span());
                    }
                }
            }
        }
        Err(e) => {
            println!("✗ Parse failed (unexpected!): {:?}", e);
            println!("  Location: {:?}", e.location);
        }
    }
    
    // Test just the difference: pub fn vs fn after multiple use statements
    println!("\n=== Testing minimal difference ===");
    let with_pub = r#"mod test;
use a.b;
use c.d;
use e.f;
pub fn test() -> num { return 42; }
"#;
    
    let without_pub = r#"mod test;
use a.b;
use c.d;
use e.f;
fn test() -> num { return 42; }
"#;
    
    println!("\nWith pub:");
    let with_pub_parse = CantaLoopParser::parse(Rule::program, with_pub);
    match with_pub_parse {
        Ok(_) => println!("✓ Parsed"),
        Err(e) => {
            println!("✗ Failed: {:?}", e);
            println!("  Location: {:?}", e.location);
        }
    }
    
    println!("\nWithout pub:");
    let without_pub_parse = CantaLoopParser::parse(Rule::program, without_pub);
    match without_pub_parse {
        Ok(_) => println!("✓ Parsed"),
        Err(e) => {
            println!("✗ Failed: {:?}", e);
            println!("  Location: {:?}", e.location);
        }
    }
}

