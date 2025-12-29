use CantaLoopRS::{
    Engine,
    FunctionSignature,
    ValueKind,
};
use CantaLoopRS::core::engine::Arity;

/// Helper function to create a test engine with basic functions
pub fn create_test_engine() -> Engine {
    let mut engine = Engine::new();

    // Add a print function that captures output for testing
    let print_sig = FunctionSignature {
        params: vec![ValueKind::String],
        return_type: Box::new(ValueKind::String),
    };
    engine.add_string_function("print", print_sig, Arity::Fixed(1), |args| {
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
    use CantaLoopRS::parse_program;
    let result = parse_program(code);
    assert!(result.is_ok(), "Expected parse to succeed, got: {:?}", result);
}

/// Assert that parsing fails
pub fn assert_parse_failure(code: &str) {
    use CantaLoopRS::parse_program;
    let result = parse_program(code);
    assert!(result.is_err(), "Expected parse to fail, but it succeeded");
}

/// Run code from a string using the engine
/// Creates a temporary file and runs it
pub fn run_code(engine: &mut Engine, code: &str) {
    use std::fs;
    use std::path::Path;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    
    // Create a temporary test file
    let test_dir = Path::new("target").join("test_temp");
    fs::create_dir_all(&test_dir).ok(); // Ignore errors if it exists
    
    // Use a unique filename based on the code hash to avoid collisions
    let mut hasher = DefaultHasher::new();
    code.hash(&mut hasher);
    let hash = hasher.finish();
    let test_file = test_dir.join(format!("test_{:x}.mln", hash));
    
    // Write code to file
    fs::write(&test_file, code).expect("Failed to write test file");
    
    // Run the file
    engine.run(test_file.to_str().unwrap());
    
    // Clean up (ignore errors)
    fs::remove_file(&test_file).ok();
}

