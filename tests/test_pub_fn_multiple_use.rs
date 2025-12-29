use CantaLoopRS::core::parser::{CantaLoopParser, Rule};
use pest::Parser;

/// Test with multiple use statements to reproduce the issue
#[test]
fn test_pub_fn_after_multiple_use_statements() {
    // Working: fn without pub after multiple use statements
    let working = r#"
mod test;
use grades.calculate_average;
use grades.calculate_final_grade;
use grades.grade_statistics;
use grades.highest_grade;
use grades.lowest_grade;
use std.array_length;

fn letterGrade(grade: num) -> string {
    return "A";
}
"#;
    
    // Non-working: pub fn after multiple use statements
    let non_working = r#"
mod test;
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
    
    println!("\n=== Testing WORKING case (fn without pub, multiple use) ===");
    let working_parse = CantaLoopParser::parse(Rule::program, working);
    match working_parse {
        Ok(mut pairs) => {
            println!("✓ Parsed successfully");
            let program = pairs.next().unwrap();
            println!("Program rule matched: {:?}", program.as_rule());
        }
        Err(e) => {
            println!("✗ Parse failed: {:?}", e);
            println!("Error location: {:?}", e.location);
        }
    }
    
    println!("\n=== Testing NON-WORKING case (pub fn, multiple use) ===");
    let non_working_parse = CantaLoopParser::parse(Rule::program, non_working);
    match non_working_parse {
        Ok(mut pairs) => {
            println!("✓ Parsed successfully");
            let program = pairs.next().unwrap();
            println!("Program rule matched: {:?}", program.as_rule());
        }
        Err(e) => {
            println!("✗ Parse failed: {:?}", e);
            println!("Error location: {:?}", e.location);
            println!("Error line/col: {:?}", e.line_col);
            if let pest::error::ErrorVariant::ParsingError { positives, negatives } = &e.variant {
                println!("Expected: {:?}", positives);
                println!("Not expected: {:?}", negatives);
            }
        }
    }
    
    // Test block parsing with multiple use statements
    println!("\n=== Testing block parsing with multiple use statements ===");
    let block_working = r#"
use grades.calculate_average;
use grades.calculate_final_grade;
use grades.grade_statistics;
use grades.highest_grade;
use grades.lowest_grade;
use std.array_length;

fn letterGrade(grade: num) -> string {
    return "A";
}
"#;
    
    let block_non_working = r#"
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
    
    println!("\nBlock (working, multiple use):");
    let block_parse_working = CantaLoopParser::parse(Rule::block, block_working);
    match block_parse_working {
        Ok(pairs) => {
            println!("✓ Block parsed successfully");
            for pair in pairs {
                println!("  Rule: {:?}", pair.as_rule());
            }
        }
        Err(e) => {
            println!("✗ Block parse failed: {:?}", e);
            println!("  Error location: {:?}", e.location);
        }
    }
    
    println!("\nBlock (non-working, multiple use):");
    let block_parse_non_working = CantaLoopParser::parse(Rule::block, block_non_working);
    match block_parse_non_working {
        Ok(pairs) => {
            println!("✓ Block parsed successfully");
            for pair in pairs {
                println!("  Rule: {:?}", pair.as_rule());
            }
        }
        Err(e) => {
            println!("✗ Block parse failed: {:?}", e);
            println!("  Error location: {:?}", e.location);
            println!("  Error line/col: {:?}", e.line_col);
            if let pest::error::ErrorVariant::ParsingError { positives, negatives } = &e.variant {
                println!("  Expected: {:?}", positives);
                println!("  Not expected: {:?}", negatives);
            }
        }
    }
}

