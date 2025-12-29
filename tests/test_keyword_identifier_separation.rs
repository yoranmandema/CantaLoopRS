use CantaLoopRS::core::parser::{CantaLoopParser, Rule};
use pest::Parser;

/// Test that keywords cannot be used as identifiers, but identifiers starting with keywords can
#[test]
fn test_keyword_identifier_separation() {
    // Keywords should NOT parse as identifiers
    let keywords = vec!["fn", "if", "else", "let", "return", "true", "false", "loop", "while", "for", "break", "continue", "use", "mod", "pub", "const"];
    
    println!("\n=== Testing that keywords cannot be identifiers ===");
    for keyword in &keywords {
        let result = CantaLoopParser::parse(Rule::identifier, keyword);
        match result {
            Ok(_) => {
                // This should fail - keywords shouldn't be identifiers
                panic!("Keyword '{}' was incorrectly parsed as an identifier!", keyword);
            }
            Err(_) => {
                println!("  '{}': ✓ Correctly rejected (keyword)", keyword);
            }
        }
    }
    
    // Identifiers starting with keywords SHOULD parse
    let valid_identifiers = vec![
        "letter",      // starts with "let"
        "letterGrade", // starts with "let"
        "function",    // starts with "fn" (if we had that)
        "ifelse",      // starts with "if"
        "returnValue", // starts with "return"
        "trueValue",   // starts with "true"
        "falsePositive", // starts with "false"
        "loopCounter", // starts with "loop"
        "whileLoop",   // starts with "while"
        "forEach",     // starts with "for"
        "breakPoint",  // starts with "break"
        "continueFlag", // starts with "continue"
        "userName",    // starts with "use"
        "moduleName",  // starts with "mod"
        "publicAPI",   // starts with "pub"
        "constantValue", // starts with "const"
    ];
    
    println!("\n=== Testing that identifiers starting with keywords ARE valid ===");
    for id in valid_identifiers {
        let result = CantaLoopParser::parse(Rule::identifier, id);
        match result {
            Ok(_) => {
                println!("  '{}': ✓ Valid identifier", id);
            }
            Err(e) => {
                panic!("Identifier '{}' starting with a keyword was incorrectly rejected: {:?}", id, e);
            }
        }
    }
    
    // Test that keywords followed by whitespace are still rejected
    println!("\n=== Testing keywords with whitespace (should be rejected) ===");
    for keyword in keywords {
        let with_space = format!("{} ", keyword);
        let result = CantaLoopParser::parse(Rule::identifier, &with_space);
        match result {
            Ok(_) => {
                // This should fail - keywords with whitespace shouldn't be identifiers
                panic!("Keyword '{} ' was incorrectly parsed as an identifier!", keyword);
            }
            Err(_) => {
                println!("  '{} ': ✓ Correctly rejected", keyword);
            }
        }
    }
}

