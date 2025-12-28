use CantaLoopRS::{
    engine::Engine,
    hir_lowering::{FunctionSignature, ValueKind},
};

/// Helper function to create a test engine with basic functions
pub fn create_test_engine() -> Engine {
    let mut engine = Engine::new();

    // Add a print function that captures output for testing
    let print_sig = FunctionSignature {
        params: vec![ValueKind::String],
        return_type: Box::new(ValueKind::String),
    };
    engine.add_function("print", print_sig, |args| {
        println!("{}", args[0]);
        "".to_string()
    });

    engine
}

/// Helper function to create an engine without any built-in functions
pub fn create_empty_engine() -> Engine {
    Engine::new()
}

/// Assert that parsing succeeds
pub fn assert_parse_success(code: &str) {
    use CantaLoopRS::parser::parse_program;
    let result = parse_program(code);
    assert!(result.is_ok(), "Expected parse to succeed, got: {:?}", result);
}

/// Assert that parsing fails
pub fn assert_parse_failure(code: &str) {
    use CantaLoopRS::parser::parse_program;
    let result = parse_program(code);
    assert!(result.is_err(), "Expected parse to fail, but it succeeded");
}

