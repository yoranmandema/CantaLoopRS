use CantaLoopRS::core::parser::{CantaLoopParser, Rule};
use pest::Parser;

/// Test to understand why 'letter' cannot be parsed as an identifier
#[test]
fn test_letter_identifier_bug() {
    // Test various identifiers starting with 'l'
    let test_cases = vec![
        "l",
        "le",
        "let",
        "lett",
        "lette",
        "letter",
        "lettera",
        "letterA",
        "letterGrade",
        "lGrade",
        "l_grade",
        "list",
        "length",
        "loop", // This should fail (it's a keyword)
    ];
    
    println!("\n=== Testing identifiers starting with 'l' ===");
    for id in test_cases {
        let result = CantaLoopParser::parse(Rule::identifier, id);
        match result {
            Ok(_) => println!("  '{}': ✓", id),
            Err(e) => {
                println!("  '{}': ✗", id);
                // Check if it's because it's a keyword
                if id == "loop" {
                    println!("    (Expected - 'loop' is a keyword)");
                } else {
                    println!("    (Unexpected failure!)");
                }
            }
        }
    }
    
    // Test if the issue is with the negative lookahead for "let"
    println!("\n=== Hypothesis: 'letter' starts with 'let' which is a keyword ===");
    println!("The negative lookahead for 'let' might be too aggressive");
    
    // Check the grammar rule
    println!("\nThe identifier rule has: !(\"let\" ~ (ASCII_ALPHANUMERIC | \"_\" | WHITESPACE))");
    println!("This should prevent 'let' followed by alphanumeric, but 'letter' starts with 'let'");
    println!("The issue is that 'letter' starts with 'let' + 't' (alphanumeric), so the negative lookahead matches!");
}

