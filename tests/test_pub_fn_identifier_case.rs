use cantaloop::core::parser::{CantaLoopParser, Rule};
use pest::Parser;

/// Test if the issue is with mixed-case identifiers
#[test]
fn test_identifier_case_matters() {
    // Test with camelCase identifier (letterGrade)
    let camel_case = r#"mod test;
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
    
    // Test with snake_case identifier (letter_grade)
    let snake_case = r#"mod test;
use grades.calculate_average;
use grades.calculate_final_grade;
use grades.grade_statistics;
use grades.highest_grade;
use grades.lowest_grade;
use std.array_length;

pub fn letter_grade(grade: num) -> string {
    return "A";
}
"#;
    
    // Test with all lowercase (lettergrade)
    let all_lower = r#"mod test;
use grades.calculate_average;
use grades.calculate_final_grade;
use grades.grade_statistics;
use grades.highest_grade;
use grades.lowest_grade;
use std.array_length;

pub fn lettergrade(grade: num) -> string {
    return "A";
}
"#;
    
    // Test with simple identifier (test)
    let simple = r#"mod test;
use grades.calculate_average;
use grades.calculate_final_grade;
use grades.grade_statistics;
use grades.highest_grade;
use grades.lowest_grade;
use std.array_length;

pub fn test(grade: num) -> string {
    return "A";
}
"#;
    
    println!("\n=== Testing camelCase (letterGrade) ===");
    let camel_parse = CantaLoopParser::parse(Rule::program, camel_case);
    match camel_parse {
        Ok(_) => println!("✓ Parsed"),
        Err(e) => {
            println!("✗ Failed: {:?}", e);
            println!("  Location: {:?}", e.location);
        }
    }
    
    println!("\n=== Testing snake_case (letter_grade) ===");
    let snake_parse = CantaLoopParser::parse(Rule::program, snake_case);
    match snake_parse {
        Ok(_) => println!("✓ Parsed"),
        Err(e) => {
            println!("✗ Failed: {:?}", e);
        }
    }
    
    println!("\n=== Testing all lowercase (lettergrade) ===");
    let lower_parse = CantaLoopParser::parse(Rule::program, all_lower);
    match lower_parse {
        Ok(_) => println!("✓ Parsed"),
        Err(e) => {
            println!("✗ Failed: {:?}", e);
        }
    }
    
    println!("\n=== Testing simple (test) ===");
    let simple_parse = CantaLoopParser::parse(Rule::program, simple);
    match simple_parse {
        Ok(_) => println!("✓ Parsed"),
        Err(e) => {
            println!("✗ Failed: {:?}", e);
        }
    }
    
    // Test parsing just the identifier
    println!("\n=== Testing identifier parsing directly ===");
    let identifiers = vec!["letterGrade", "letter_grade", "lettergrade", "test"];
    for id in identifiers {
        let id_parse = CantaLoopParser::parse(Rule::identifier, id);
        match id_parse {
            Ok(pairs) => {
                println!("  {}: ✓ Parsed as identifier", id);
                for pair in pairs {
                    println!("    Rule: {:?}, Text: {:?}", pair.as_rule(), pair.as_str());
                }
            }
            Err(e) => {
                println!("  {}: ✗ Failed: {:?}", id, e);
            }
        }
    }
}

