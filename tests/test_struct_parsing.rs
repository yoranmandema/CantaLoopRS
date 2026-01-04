use CantaLoopRS::parse_program;

#[test]
fn test_parse_struct() {
    let code = r#"
struct Point {
    x: num,
    y: num
}
"#;
    let result = parse_program(code);
    assert!(result.is_ok(), "Expected parse to succeed, got: {:?}", result);
}

#[test]
fn test_parse_struct_with_trailing_comma() {
    let code = r#"
struct Point {
    x: num,
    y: num,
}
"#;
    let result = parse_program(code);
    // This should fail because we don't support trailing commas yet
    // But let's see what happens
    println!("Result: {:?}", result);
}

