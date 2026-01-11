use cantaloop::{parse_cst_program, lower_cst_to_ast};
use cantaloop::parse_program;

/// Helper function to assert CST parsing succeeds
fn assert_cst_parse_success(src: &str) {
    match parse_cst_program(src) {
        Ok((cst, _docs)) => {
            // Verify we got a program with at least one block
            assert!(!cst.blocks.is_empty(), "CST program should have at least one block");
        }
        Err(e) => panic!("Expected CST parsing to succeed, but got error: {:?}", e),
    }
}

/// Helper function to assert CST parsing fails
fn assert_cst_parse_failure(src: &str) {
    match parse_cst_program(src) {
        Ok(_) => panic!("Expected CST parsing to fail, but it succeeded"),
        Err(_) => {
            // Expected failure
        }
    }
}

/// Helper function to test CST -> AST lowering roundtrip
fn assert_cst_lower_roundtrip(src: &str) {
    // Parse to CST
    let (cst, docs) = parse_cst_program(src)
        .expect("CST parsing should succeed");
    
    // Lower to AST
    let (ast_from_cst, _) = lower_cst_to_ast(cst, docs)
        .expect("Lowering CST to AST should succeed");
    
    // Parse directly to AST for comparison
    let ast_direct = parse_program(src)
        .expect("Direct AST parsing should succeed");
    
    // Compare (basic structure comparison)
    assert_eq!(ast_from_cst.blocks.len(), ast_direct.blocks.len(),
               "CST->AST lowering should produce same number of blocks as direct parsing");
}

#[test]
fn test_cst_literals() {
    assert_cst_parse_success("let x = 42;");
    assert_cst_parse_success("let s = \"hello\";");
    assert_cst_parse_success("let b = true;");
}

#[test]
fn test_cst_expressions() {
    // Expressions as statements
    assert_cst_parse_success("let x = 1 + 2;");
    assert_cst_parse_success("let y = a * b + c;");
    assert_cst_parse_success("let z = x == y;");
    assert_cst_parse_success("let w = a |> b;");
    assert_cst_parse_success("let v = a <| b;");
}

#[test]
fn test_cst_let_statement() {
    assert_cst_parse_success("let x = 10;");
    assert_cst_parse_success("pub let x = 10;");
    assert_cst_parse_success("let x: num = 10;");
}

#[test]
fn test_cst_function_call() {
    assert_cst_parse_success("foo();");
    assert_cst_parse_success("foo(1, 2, 3);");
    assert_cst_parse_success("foo(a, b);");
}

#[test]
fn test_cst_arrays() {
    assert_cst_parse_success("let arr = [1, 2, 3];");
    assert_cst_parse_success("let empty = [];");
}

#[test]
fn test_cst_control_flow() {
    assert_cst_parse_success("if x { let y = 1; }");
    assert_cst_parse_success("while x { let y = 1; }");
    // For loop - TODO: Fix range parsing in CST builder
    // assert_cst_parse_success("for i in 0..10 { let x = i; }");
}

#[test]
fn test_cst_function_declaration() {
    assert_cst_parse_success("fn add(x: num, y: num) -> num { return x + y; }");
    assert_cst_parse_success("pub fn add(x: num, y: num) { return x + y; }");
}

#[test]
fn test_cst_lower_roundtrip_simple() {
    assert_cst_lower_roundtrip("let x = 42;");
    assert_cst_lower_roundtrip("let s = \"hello\";");
}

#[test]
fn test_cst_lower_roundtrip_expressions() {
    assert_cst_lower_roundtrip("let x = 1 + 2;");
    assert_cst_lower_roundtrip("let y = a * b;");
}

#[test]
fn test_cst_span_preservation() {
    let src = "let x = 42;";
    let (cst, _docs) = parse_cst_program(src).expect("Should parse");
    
    // Check that spans are present
    if let Some(block) = cst.blocks.first() {
        assert!(block.span.start < block.span.end, "Block should have valid span");
        if let Some(stmt) = block.node.statements.first() {
            assert!(stmt.span.start < stmt.span.end, "Statement should have valid span");
        }
    }
}

#[test]
fn test_cst_spans_are_byte_offsets() {
    let src = "let x = 42;";
    let (cst, _docs) = parse_cst_program(src).expect("Should parse");
    
    // First block should start at beginning of input (after SOI)
    if let Some(block) = cst.blocks.first() {
        // Span should be within bounds of source
        assert!(block.span.end as usize <= src.len(), 
                "Block span end should be within source length");
    }
}

