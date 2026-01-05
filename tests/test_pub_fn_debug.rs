use cantaloop::core::parser::{CantaLoopParser, Rule};
use pest::Parser;

/// Debug test to see what Pest is actually trying to match
#[test]
fn test_debug_pest_parsing() {
    // Working case: fn without pub after use statements
    let working = r#"
mod test;
use std.print;
fn test() -> num { return 42; }
"#;
    
    // Non-working case: pub fn after use statements
    let non_working = r#"
mod test;
use std.print;
pub fn test() -> num { return 42; }
"#;
    
    println!("\n=== Testing WORKING case (fn without pub) ===");
    let working_parse = CantaLoopParser::parse(Rule::program, working);
    match working_parse {
        Ok(mut pairs) => {
            println!("✓ Parsed successfully");
            let program = pairs.next().unwrap();
            println!("Program rule matched: {:?}", program.as_rule());
            println!("Program span: {:?}", program.as_span());
        }
        Err(e) => {
            println!("✗ Parse failed: {:?}", e);
        }
    }
    
    println!("\n=== Testing NON-WORKING case (pub fn) ===");
    let non_working_parse = CantaLoopParser::parse(Rule::program, non_working);
    match non_working_parse {
        Ok(mut pairs) => {
            println!("✓ Parsed successfully");
            let program = pairs.next().unwrap();
            println!("Program rule matched: {:?}", program.as_rule());
            println!("Program span: {:?}", program.as_span());
        }
        Err(e) => {
            println!("✗ Parse failed: {:?}", e);
            println!("Error location: {:?}", e.location);
            println!("Error variant: {:?}", e.variant);
        }
    }
    
    // Try parsing just the block part
    println!("\n=== Testing block parsing directly ===");
    let block_working = r#"
use std.print;
fn test() -> num { return 42; }
"#;
    
    let block_non_working = r#"
use std.print;
pub fn test() -> num { return 42; }
"#;
    
    println!("\nBlock (working):");
    let block_parse_working = CantaLoopParser::parse(Rule::block, block_working);
    match block_parse_working {
        Ok(pairs) => {
            println!("✓ Block parsed successfully");
            for pair in pairs {
                println!("  Rule: {:?}, Span: {:?}", pair.as_rule(), pair.as_span());
            }
        }
        Err(e) => {
            println!("✗ Block parse failed: {:?}", e);
        }
    }
    
    println!("\nBlock (non-working):");
    let block_parse_non_working = CantaLoopParser::parse(Rule::block, block_non_working);
    match block_parse_non_working {
        Ok(pairs) => {
            println!("✓ Block parsed successfully");
            for pair in pairs {
                println!("  Rule: {:?}, Span: {:?}", pair.as_rule(), pair.as_span());
            }
        }
        Err(e) => {
            println!("✗ Block parse failed: {:?}", e);
            println!("  Error location: {:?}", e.location);
            println!("  Error variant: {:?}", e.variant);
        }
    }
}

/// Test to see if the issue is with statement_without_semicolon vs statement_with_semicolon
#[test]
fn test_debug_statement_parsing() {
    // Test statement_without_semicolon directly
    let fn_statement = "fn test() -> num { return 42; }";
    let pub_fn_statement = "pub fn test() -> num { return 42; }";
    
    println!("\n=== Testing statement_without_semicolon ===");
    println!("fn statement:");
    let fn_parse = CantaLoopParser::parse(Rule::statement_without_semicolon, fn_statement);
    match fn_parse {
        Ok(mut pairs) => {
            println!("✓ Parsed successfully");
            for pair in pairs {
                println!("  Rule: {:?}, Text: {:?}", pair.as_rule(), pair.as_str());
            }
        }
        Err(e) => {
            println!("✗ Parse failed: {:?}", e);
        }
    }
    
    println!("\npub fn statement:");
    let pub_fn_parse = CantaLoopParser::parse(Rule::statement_without_semicolon, pub_fn_statement);
    match pub_fn_parse {
        Ok(mut pairs) => {
            println!("✓ Parsed successfully");
            for pair in pairs {
                println!("  Rule: {:?}, Text: {:?}", pair.as_rule(), pair.as_str());
            }
        }
        Err(e) => {
            println!("✗ Parse failed: {:?}", e);
            println!("  Error location: {:?}", e.location);
        }
    }
    
    // Test function_statement directly
    println!("\n=== Testing function_statement directly ===");
    let fn_func = "fn test() -> num { return 42; }";
    let pub_fn_func = "pub fn test() -> num { return 42; }";
    
    println!("\nfunction_statement (fn):");
    let fn_func_parse = CantaLoopParser::parse(Rule::function_statement, fn_func);
    match fn_func_parse {
        Ok(mut pairs) => {
            println!("✓ Parsed successfully");
            for pair in pairs {
                println!("  Rule: {:?}, Text: {:?}", pair.as_rule(), pair.as_str());
            }
        }
        Err(e) => {
            println!("✗ Parse failed: {:?}", e);
        }
    }
    
    println!("\nfunction_statement (pub fn):");
    let pub_fn_func_parse = CantaLoopParser::parse(Rule::function_statement, pub_fn_func);
    match pub_fn_func_parse {
        Ok(mut pairs) => {
            println!("✓ Parsed successfully");
            for pair in pairs {
                println!("  Rule: {:?}, Text: {:?}", pair.as_rule(), pair.as_str());
            }
        }
        Err(e) => {
            println!("✗ Parse failed: {:?}", e);
            println!("  Error location: {:?}", e.location);
        }
    }
}

