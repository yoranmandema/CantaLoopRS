mod common;

use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;
use cantaloop::Engine;
use common::helpers::run_code;

fn compile_project_main(engine: &Engine, project_root: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let main_path = project_root.join("src").join("main.cl");
    engine.compile_with_project(main_path.to_str().unwrap(), Some(project_root.as_path()))?;
    Ok(())
}

/// Helper to create a temporary project directory with module files
fn create_test_project() -> (TempDir, PathBuf) {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let project_root = temp_dir.path().to_path_buf();
    
    // Create src directory
    let src_dir = project_root.join("src");
    fs::create_dir_all(&src_dir).expect("Failed to create src directory");
    
    // Create melon.json
    let melon_json = project_root.join("melon.json");
    fs::write(&melon_json, r#"
{
    "name": "test-project",
    "version": "1.0.0",
    "main": "main.cl"
}
"#).expect("Failed to write melon.json");
    
    (temp_dir, project_root)
}

/// Helper to create a module file
fn create_module_file(project_root: &PathBuf, filename: &str, content: &str) {
    let file_path = project_root.join("src").join(filename);
    fs::write(&file_path, content).expect(&format!("Failed to write module file: {}", filename));
}

#[test]
fn test_module_declaration() {
    let mut engine = common::helpers::create_test_engine();
    let code = r#"
mod utils;

pub fn add(a: num, b: num) -> num {
    return a + b;
}
"#;
    
    // Should parse and compile successfully
    run_code(&mut engine, code);
    // Module declaration should be processed without error
}

#[test]
fn test_module_public_function() {
    let (_temp_dir, project_root) = create_test_project();
    
    // Create a module with a public function
    create_module_file(&project_root, "utils.cl", r#"
mod utils;

pub fn add(a: num, b: num) -> num {
    return a + b;
}

fn private_fn() -> num {
    return 42;
}
"#);
    
    // Create main file that uses the module
    create_module_file(&project_root, "main.cl", r#"
use print from std;
use add from utils;

let result = add(5, 3)!!;
print(result)!;
"#);
    
    let mut engine = Engine::new();
    cantaloop::stdlib::load_stdlib_runtime(&mut engine);

    compile_project_main(&engine, &project_root).expect("Failed to compile project");
    
    // Should be able to import and use the public function
    // Note: This would require running the full compilation pipeline
    // For now, we test that modules load without error
}

#[test]
fn test_module_public_constant() {
    let (_temp_dir, project_root) = create_test_project();
    
    create_module_file(&project_root, "math.cl", r#"
mod math;

pub const PI = 3.14159;
pub const E = 2.71828;

const PRIVATE_CONST = 42;
"#);
    
    create_module_file(&project_root, "main.cl", r#"
use print from std;
use PI from math;

print(PI)!;
"#);
    
    let mut engine = Engine::new();
    cantaloop::stdlib::load_stdlib_runtime(&mut engine);
    compile_project_main(&engine, &project_root).expect("Failed to compile project");
}

#[test]
fn test_module_single_import() {
    let (_temp_dir, project_root) = create_test_project();
    
    create_module_file(&project_root, "utils.cl", r#"
mod utils;

pub fn add(a: num, b: num) -> num {
    return a + b;
}

pub fn multiply(a: num, b: num) -> num {
    return a * b;
}
"#);
    
    create_module_file(&project_root, "main.cl", r#"
use add from utils;

let result = add(10, 20)!!;
"#);
    
    let mut engine = Engine::new();
    cantaloop::stdlib::load_stdlib_runtime(&mut engine);
    compile_project_main(&engine, &project_root).expect("Failed to compile project");
}

#[test]
fn test_module_multiple_imports() {
    let (_temp_dir, project_root) = create_test_project();
    
    create_module_file(&project_root, "utils.cl", r#"
mod utils;

pub fn add(a: num, b: num) -> num {
    return a + b;
}

pub fn subtract(a: num, b: num) -> num {
    return a - b;
}

pub fn multiply(a: num, b: num) -> num {
    return a * b;
}
"#);
    
    create_module_file(&project_root, "main.cl", r#"
use add, multiply from utils;

let sum = add(5, 3)!!;
let product = multiply(5, 3)!!;
"#);
    
    let mut engine = Engine::new();
    cantaloop::stdlib::load_stdlib_runtime(&mut engine);
    compile_project_main(&engine, &project_root).expect("Failed to compile project");
}

#[test]
fn test_module_wildcard_import() {
    let (_temp_dir, project_root) = create_test_project();
    
    create_module_file(&project_root, "utils.cl", r#"
mod utils;

pub fn add(a: num, b: num) -> num {
    return a + b;
}

pub fn subtract(a: num, b: num) -> num {
    return a - b;
}
"#);
    
    create_module_file(&project_root, "main.cl", r#"
use * from utils;

let sum = add(10, 20)!!;
let diff = subtract(10, 5)!!;
"#);
    
    let mut engine = Engine::new();
    cantaloop::stdlib::load_stdlib_runtime(&mut engine);
    compile_project_main(&engine, &project_root).expect("Failed to compile project");
}

#[test]
fn test_module_member_access() {
    let (_temp_dir, project_root) = create_test_project();
    
    create_module_file(&project_root, "math.cl", r#"
mod math;

pub fn square(x: num) -> num {
    return x * x;
}
"#);
    
    create_module_file(&project_root, "main.cl", r#"
use print from std;

let result = math.square(5)!!;
print(result)!;
"#);
    
    let mut engine = Engine::new();
    cantaloop::stdlib::load_stdlib_runtime(&mut engine);
    compile_project_main(&engine, &project_root).expect("Failed to compile project");
}

#[test]
fn test_module_private_function_not_accessible() {
    let (_temp_dir, project_root) = create_test_project();
    
    create_module_file(&project_root, "utils.cl", r#"
mod utils;

fn private_helper() -> num {
    return 42;
}

pub fn public_fn() -> num {
    return private_helper();
}
"#);
    
    create_module_file(&project_root, "main.cl", r#"
use private_helper from utils;
"#);
    
    let mut engine = Engine::new();
    cantaloop::stdlib::load_stdlib_runtime(&mut engine);

    // Import should fail at compile time (private symbol)
    assert!(compile_project_main(&engine, &project_root).is_err());
}

#[test]
fn test_module_constant_with_expression() {
    let (_temp_dir, project_root) = create_test_project();
    
    create_module_file(&project_root, "constants.cl", r#"
mod constants;

pub const TWO = 2;
pub const FOUR = TWO * 2;
pub const HUNDRED = 100;
pub const DAY_IN_SECONDS = 60 * 60 * 24;
"#);
    
    create_module_file(&project_root, "main.cl", r#"
use print from std;
use TWO, FOUR, DAY_IN_SECONDS from constants;

print(TWO)!;
print(FOUR)!;
print(DAY_IN_SECONDS)!;
"#);
    
    let mut engine = Engine::new();
    cantaloop::stdlib::load_stdlib_runtime(&mut engine);
    compile_project_main(&engine, &project_root).expect("Failed to compile project");
}

#[test]
fn test_module_nested_imports() {
    let (_temp_dir, project_root) = create_test_project();
    
    // Create a module that imports from stdlib
    create_module_file(&project_root, "wrapper.cl", r#"
mod wrapper;

use print from std;

pub fn wrapped_print(msg: string) {
    print(msg)!;
}
"#);
    
    create_module_file(&project_root, "main.cl", r#"
use wrapped_print from wrapper;

wrapped_print("Hello from wrapped function")!;
"#);
    
    let mut engine = Engine::new();
    cantaloop::stdlib::load_stdlib_runtime(&mut engine);
    compile_project_main(&engine, &project_root).expect("Failed to compile project");
}

#[test]
fn test_module_import_same_name_twice_error() {
    let (_temp_dir, project_root) = create_test_project();
    
    create_module_file(&project_root, "utils.cl", r#"
mod utils;

pub fn add(a: num, b: num) -> num {
    return a + b;
}
"#);
    
    create_module_file(&project_root, "main.cl", r#"
use add from utils;
use add from utils;
"#);
    
    let mut engine = Engine::new();
    cantaloop::stdlib::load_stdlib_runtime(&mut engine);

    // Compilation should fail with duplicate import error
    assert!(compile_project_main(&engine, &project_root).is_err());
}

#[test]
fn test_module_function_calling_private_function() {
    let (_temp_dir, project_root) = create_test_project();
    
    create_module_file(&project_root, "calc.cl", r#"
mod calc;

fn helper(x: num) -> num {
    return x * 2;
}

pub fn double(x: num) -> num {
    return helper(x);
}
"#);
    
    create_module_file(&project_root, "main.cl", r#"
use print from std;
use double from calc;

let result = double(21)!!;
print(result)!;
"#);
    
    let mut engine = Engine::new();
    cantaloop::stdlib::load_stdlib_runtime(&mut engine);
    compile_project_main(&engine, &project_root).expect("Failed to compile project");
}

#[test]
fn test_module_with_stdlib_import() {
    let (_temp_dir, project_root) = create_test_project();
    
    // Provide a project module named `math` to import from.
    create_module_file(&project_root, "math.cl", r#"
mod math;

pub fn add(a: num, b: num) -> num {
    return a + b;
}

pub fn multiply(a: num, b: num) -> num {
    return a * b;
}
"#);

    create_module_file(&project_root, "main.cl", r#"
use print from std;
use add, multiply from math;

let sum = add(5, 3)!!;
let product = multiply(4, 7)!!;

print(sum)!;
print(product)!;
"#);
    
    let mut engine = Engine::new();
    cantaloop::stdlib::load_stdlib_runtime(&mut engine);
    compile_project_main(&engine, &project_root).expect("Failed to compile project");
}

#[test]
fn test_module_file_without_mod_declaration() {
    let (_temp_dir, project_root) = create_test_project();
    
    // Create a file that doesn't start with "mod"
    create_module_file(&project_root, "not_a_module.cl", r#"
let x = 42;
"#);
    
    let mut engine = Engine::new();
    cantaloop::stdlib::load_stdlib_runtime(&mut engine);

    // Should skip this file (not load as module) and still compile main if present.
    // Add an empty main.
    create_module_file(&project_root, "main.cl", "let x = 1;");
    compile_project_main(&engine, &project_root).expect("Failed to compile project");
}

#[test]
fn test_module_load_from_file() {
    let (_temp_dir, project_root) = create_test_project();
    
    create_module_file(&project_root, "utils.cl", r#"
mod utils;

pub fn greet(name: string) -> string {
    return "Hello, " + name;
}
"#);
    
    let mut engine = Engine::new();
    cantaloop::stdlib::load_stdlib_runtime(&mut engine);

    // Verify the module can be loaded by compiling a main that imports it.
    create_module_file(&project_root, "main.cl", r#"
use greet from utils;
let x = greet("world");
"#);
    compile_project_main(&engine, &project_root).expect("Failed to compile project");
}

#[test]
fn test_module_load_project_modules() {
    let (_temp_dir, project_root) = create_test_project();
    
    create_module_file(&project_root, "math.cl", r#"
mod math;

pub fn square(x: num) -> num {
    return x * x;
}
"#);
    
    create_module_file(&project_root, "utils.cl", r#"
mod utils;

pub fn double(x: num) -> num {
    return x * 2;
}
"#);
    
    // main.cl should be skipped (not a module)
    create_module_file(&project_root, "main.cl", r#"
use print from std;
"#);
    
    let mut engine = Engine::new();
    cantaloop::stdlib::load_stdlib_runtime(&mut engine);
    compile_project_main(&engine, &project_root).expect("Failed to compile project");
}

#[test]
fn test_module_import_non_existent_function() {
    let (_temp_dir, project_root) = create_test_project();
    
    create_module_file(&project_root, "utils.cl", r#"
mod utils;

pub fn add(a: num, b: num) -> num {
    return a + b;
}
"#);
    
    create_module_file(&project_root, "main.cl", r#"
use nonexistent from utils;
"#);
    
    let mut engine = Engine::new();
    cantaloop::stdlib::load_stdlib_runtime(&mut engine);

    // Compilation should fail (symbol doesn't exist in module)
    assert!(compile_project_main(&engine, &project_root).is_err());
}

#[test]
fn test_module_import_non_existent_module() {
    let (_temp_dir, project_root) = create_test_project();
    
    create_module_file(&project_root, "main.cl", r#"
use function from nonexistent;
"#);
    
    let mut engine = Engine::new();
    cantaloop::stdlib::load_stdlib_runtime(&mut engine);

    // Compilation should fail (module doesn't exist)
    assert!(compile_project_main(&engine, &project_root).is_err());
}

#[test]
fn test_module_with_type_annotations() {
    let (_temp_dir, project_root) = create_test_project();
    
    create_module_file(&project_root, "types.cl", r#"
mod types;

pub fn process_string(s: string) -> string {
    return "Processed: " + s;
}

pub fn process_number(n: num) -> num {
    return n * 2;
}
"#);
    
    create_module_file(&project_root, "main.cl", r#"
use process_string, process_number from types;

let str_result = process_string("test")!!;
let num_result = process_number(10)!!;
"#);
    
    let mut engine = Engine::new();
    cantaloop::stdlib::load_stdlib_runtime(&mut engine);
    compile_project_main(&engine, &project_root).expect("Failed to compile project");
}

#[test]
fn test_module_public_function_with_multiple_parameters() {
    let (_temp_dir, project_root) = create_test_project();
    
    create_module_file(&project_root, "math.cl", r#"
mod math;

pub fn calculate(a: num, b: num, c: num) -> num {
    return a + b * c;
}
"#);
    
    create_module_file(&project_root, "main.cl", r#"
use calculate from math;

let result = calculate(1, 2, 3)!!;
"#);
    
    let mut engine = Engine::new();
    cantaloop::stdlib::load_stdlib_runtime(&mut engine);
    compile_project_main(&engine, &project_root).expect("Failed to compile project");
}

#[test]
fn test_module_constants_in_expressions() {
    let (_temp_dir, project_root) = create_test_project();
    
    create_module_file(&project_root, "constants.cl", r#"
mod constants;

pub const ZERO = 0;
pub const ONE = 1;
pub const TWO = ONE + ONE;
pub const TEN = 10;
pub const TWENTY = TWO * TEN;
"#);
    
    create_module_file(&project_root, "main.cl", r#"
use ZERO, ONE, TWO, TEN, TWENTY from constants;
"#);
    
    let mut engine = Engine::new();
    cantaloop::stdlib::load_stdlib_runtime(&mut engine);
    compile_project_main(&engine, &project_root).expect("Failed to compile project");
}

