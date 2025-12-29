use CantaLoopRS::core::parser::{CantaLoopParser, Rule};
use pest::Parser;

/// Test identifier parsing to understand the issue
#[test]
fn test_identifier_parsing_edge_cases() {
    let test_cases = vec![
        "test",
        "letter",
        "letterGrade",
        "letter_grade",
        "lettergrade",
        "a",
        "A",
        "calculate_average",
        "student_performance",
        "generate_student_report",
    ];
    
    println!("\n=== Testing identifier parsing ===");
    for id in test_cases {
        let result = CantaLoopParser::parse(Rule::identifier, id);
        match result {
            Ok(pairs) => {
                println!("  '{}': ✓", id);
            }
            Err(e) => {
                println!("  '{}': ✗ Failed", id);
                println!("    Error: {:?}", e.variant);
                println!("    Location: {:?}", e.location);
            }
        }
    }
    
    // Test if the issue is with the negative lookahead for "letter"
    println!("\n=== Testing if 'letter' conflicts with something ===");
    let letter_test = "letter";
    let result = CantaLoopParser::parse(Rule::identifier, letter_test);
    match result {
        Ok(_) => println!("  'letter' parses as identifier"),
        Err(e) => {
            println!("  'letter' does NOT parse as identifier");
            println!("    Error: {:?}", e.variant);
        }
    }
}

