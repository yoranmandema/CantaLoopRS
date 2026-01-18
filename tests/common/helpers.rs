use cantaloop::Engine;

/// Helper function to create a test engine with basic functions
pub fn create_test_engine() -> Engine {
    let mut engine = Engine::new();

    // Load the full stdlib so compile-time stdlib registration can find native descriptors.
    // Many compilation paths panic if stdlib functions (e.g. math.round) are not present.
    cantaloop::stdlib::load_stdlib_runtime(&mut engine);

    engine
}

/// Helper function to create an engine without any built-in functions
pub fn create_empty_engine() -> Engine {
    Engine::new()
}

/// Assert that parsing succeeds
pub fn assert_parse_success(code: &str) {
    use cantaloop::parse_program;
    let result = parse_program(code);
    assert!(result.is_ok(), "Expected parse to succeed, got: {:?}", result);
}

/// Assert that parsing fails
pub fn assert_parse_failure(code: &str) {
    use cantaloop::parse_program;
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
    use std::sync::Arc;
    
    // Create a temporary test file
    let test_dir = Path::new("target").join("test_temp");
    fs::create_dir_all(&test_dir).ok(); // Ignore errors if it exists
    
    // Use a unique filename based on the code hash to avoid collisions
    let mut hasher = DefaultHasher::new();
    code.hash(&mut hasher);
    let hash = hasher.finish();
    let test_file = test_dir.join(format!("test_{:x}.cl", hash));
    
    // Write code to file
    fs::write(&test_file, code).expect("Failed to write test file");
    
    // Run the file
    // Engine execution now runs from compiled artifacts via an Arc<Engine>.
    // Move the current engine into an Arc temporarily (leaving an empty Engine behind).
    let engine_arc = Arc::new(std::mem::replace(engine, Engine::new()));
    engine_arc.compile_and_run(test_file.to_str().unwrap());
    
    // Clean up (ignore errors)
    fs::remove_file(&test_file).ok();
}

