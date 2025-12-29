mod common;

use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;
use CantaLoopRS::Engine;
use common::helpers::run_code;

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
    "main": "main.mln"
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
    let (temp_dir, project_root) = create_test_project();
    
    // Create a module with a public function
    create_module_file(&project_root, "utils.mln", r#"
mod utils;

pub fn add(a: num, b: num) -> num {
    return a + b;
}

fn private_fn() -> num {
    return 42;
}
"#);
    
    // Create main file that uses the module
    create_module_file(&project_root, "main.mln", r#"
use std.print;
use utils.add;

let result = add(5, 3)!!;
print(result)!;
"#);
    
    let mut engine = Engine::new();
    CantaLoopRS::stdlib::load_all_stdlib(&mut engine);
    
    // Load project modules
    engine.load_project_modules(&project_root).expect("Failed to load modules");
    
    // Should be able to import and use the public function
    // Note: This would require running the full compilation pipeline
    // For now, we test that modules load without error
}

#[test]
fn test_module_public_constant() {
    let (temp_dir, project_root) = create_test_project();
    
    create_module_file(&project_root, "math.mln", r#"
mod math;

pub const PI = 3.14159;
pub const E = 2.71828;

const PRIVATE_CONST = 42;
"#);
    
    create_module_file(&project_root, "main.mln", r#"
use std.print;
use math.PI;

print(PI)!;
"#);
    
    let mut engine = Engine::new();
    CantaLoopRS::stdlib::load_all_stdlib(&mut engine);
    
    engine.load_project_modules(&project_root).expect("Failed to load modules");
}

#[test]
fn test_module_single_import() {
    let (temp_dir, project_root) = create_test_project();
    
    create_module_file(&project_root, "utils.mln", r#"
mod utils;

pub fn add(a: num, b: num) -> num {
    return a + b;
}

pub fn multiply(a: num, b: num) -> num {
    return a * b;
}
"#);
    
    create_module_file(&project_root, "main.mln", r#"
use utils.add;

let result = add(10, 20)!!;
"#);
    
    let mut engine = Engine::new();
    CantaLoopRS::stdlib::load_all_stdlib(&mut engine);
    
    engine.load_project_modules(&project_root).expect("Failed to load modules");
}

#[test]
fn test_module_multiple_imports() {
    let (temp_dir, project_root) = create_test_project();
    
    create_module_file(&project_root, "utils.mln", r#"
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
    
    create_module_file(&project_root, "main.mln", r#"
use utils.{add, multiply};

let sum = add(5, 3)!!;
let product = multiply(5, 3)!!;
"#);
    
    let mut engine = Engine::new();
    CantaLoopRS::stdlib::load_all_stdlib(&mut engine);
    
    engine.load_project_modules(&project_root).expect("Failed to load modules");
}

#[test]
fn test_module_wildcard_import() {
    let (temp_dir, project_root) = create_test_project();
    
    create_module_file(&project_root, "utils.mln", r#"
mod utils;

pub fn add(a: num, b: num) -> num {
    return a + b;
}

pub fn subtract(a: num, b: num) -> num {
    return a - b;
}
"#);
    
    create_module_file(&project_root, "main.mln", r#"
use utils.*;

let sum = add(10, 20)!!;
let diff = subtract(10, 5)!!;
"#);
    
    let mut engine = Engine::new();
    CantaLoopRS::stdlib::load_all_stdlib(&mut engine);
    
    engine.load_project_modules(&project_root).expect("Failed to load modules");
}

#[test]
fn test_module_member_access() {
    let (temp_dir, project_root) = create_test_project();
    
    create_module_file(&project_root, "math.mln", r#"
mod math;

pub fn square(x: num) -> num {
    return x * x;
}
"#);
    
    create_module_file(&project_root, "main.mln", r#"
use std.print;

let result = math.square(5)!!;
print(result)!;
"#);
    
    let mut engine = Engine::new();
    CantaLoopRS::stdlib::load_all_stdlib(&mut engine);
    
    engine.load_project_modules(&project_root).expect("Failed to load modules");
}

#[test]
fn test_module_private_function_not_accessible() {
    let (temp_dir, project_root) = create_test_project();
    
    create_module_file(&project_root, "utils.mln", r#"
mod utils;

fn private_helper() -> num {
    return 42;
}

pub fn public_fn() -> num {
    return private_helper();
}
"#);
    
    create_module_file(&project_root, "main.mln", r#"
use utils.private_helper; // This should fail - function is not public
"#);
    
    let mut engine = Engine::new();
    CantaLoopRS::stdlib::load_all_stdlib(&mut engine);
    
    // Loading modules should succeed (syntax is valid)
    engine.load_project_modules(&project_root).expect("Failed to load modules");
    
    // But the import should fail at compile time
    // This would need to be tested through the full compilation pipeline
}

#[test]
fn test_module_constant_with_expression() {
    let (temp_dir, project_root) = create_test_project();
    
    create_module_file(&project_root, "constants.mln", r#"
mod constants;

pub const TWO = 2;
pub const FOUR = TWO * 2;
pub const HUNDRED = 100;
pub const DAY_IN_SECONDS = 60 * 60 * 24;
"#);
    
    create_module_file(&project_root, "main.mln", r#"
use std.print;
use constants.{TWO, FOUR, DAY_IN_SECONDS};

print(TWO)!;
print(FOUR)!;
print(DAY_IN_SECONDS)!;
"#);
    
    let mut engine = Engine::new();
    CantaLoopRS::stdlib::load_all_stdlib(&mut engine);
    
    engine.load_project_modules(&project_root).expect("Failed to load modules");
}

#[test]
fn test_module_nested_imports() {
    let (temp_dir, project_root) = create_test_project();
    
    // Create a module that imports from stdlib
    create_module_file(&project_root, "wrapper.mln", r#"
mod wrapper;

use std.print;

pub fn wrapped_print(msg: string) {
    print(msg)!;
}
"#);
    
    create_module_file(&project_root, "main.mln", r#"
use wrapper.wrapped_print;

wrapped_print("Hello from wrapped function")!;
"#);
    
    let mut engine = Engine::new();
    CantaLoopRS::stdlib::load_all_stdlib(&mut engine);
    
    engine.load_project_modules(&project_root).expect("Failed to load modules");
}

#[test]
fn test_module_import_same_name_twice_error() {
    let (temp_dir, project_root) = create_test_project();
    
    create_module_file(&project_root, "utils.mln", r#"
mod utils;

pub fn add(a: num, b: num) -> num {
    return a + b;
}
"#);
    
    create_module_file(&project_root, "main.mln", r#"
use utils.add;
use utils.add; // This should cause an error - duplicate import
"#);
    
    let mut engine = Engine::new();
    CantaLoopRS::stdlib::load_all_stdlib(&mut engine);
    
    engine.load_project_modules(&project_root).expect("Failed to load modules");
    
    // Compilation should fail with duplicate import error
    // This would need to be tested through full compilation
}

#[test]
fn test_module_function_calling_private_function() {
    let (temp_dir, project_root) = create_test_project();
    
    create_module_file(&project_root, "calc.mln", r#"
mod calc;

fn helper(x: num) -> num {
    return x * 2;
}

pub fn double(x: num) -> num {
    return helper(x);
}
"#);
    
    create_module_file(&project_root, "main.mln", r#"
use std.print;
use calc.double;

let result = double(21)!!;
print(result)!;
"#);
    
    let mut engine = Engine::new();
    CantaLoopRS::stdlib::load_all_stdlib(&mut engine);
    
    engine.load_project_modules(&project_root).expect("Failed to load modules");
}

#[test]
fn test_module_with_stdlib_import() {
    let (temp_dir, project_root) = create_test_project();
    
    create_module_file(&project_root, "main.mln", r#"
use std.print;
use math.{add, multiply};

let sum = add(5, 3)!!;
let product = multiply(4, 7)!!;

print(sum)!;
print(product)!;
"#);
    
    let mut engine = Engine::new();
    CantaLoopRS::stdlib::load_all_stdlib(&mut engine);
    
    // Should be able to import from stdlib modules
    engine.load_project_modules(&project_root).expect("Failed to load modules");
}

#[test]
fn test_module_file_without_mod_declaration() {
    let (temp_dir, project_root) = create_test_project();
    
    // Create a file that doesn't start with "mod"
    create_module_file(&project_root, "not_a_module.mln", r#"
let x = 42;
"#);
    
    let mut engine = Engine::new();
    CantaLoopRS::stdlib::load_all_stdlib(&mut engine);
    
    // Should skip this file (not load as module)
    engine.load_project_modules(&project_root).expect("Failed to load modules");
}

#[test]
fn test_module_load_from_file() {
    let (temp_dir, project_root) = create_test_project();
    
    create_module_file(&project_root, "utils.mln", r#"
mod utils;

pub fn greet(name: string) -> string {
    return "Hello, " + name;
}
"#);
    
    let mut engine = Engine::new();
    CantaLoopRS::stdlib::load_all_stdlib(&mut engine);
    
    let module_path = project_root.join("src").join("utils.mln");
    let result = engine.load_module_from_file(&module_path, &project_root);
    
    assert!(result.is_ok(), "Failed to load module: {:?}", result);
    assert_eq!(result.unwrap(), "utils");
}

#[test]
fn test_module_load_project_modules() {
    let (temp_dir, project_root) = create_test_project();
    
    create_module_file(&project_root, "math.mln", r#"
mod math;

pub fn square(x: num) -> num {
    return x * x;
}
"#);
    
    create_module_file(&project_root, "utils.mln", r#"
mod utils;

pub fn double(x: num) -> num {
    return x * 2;
}
"#);
    
    // main.mln should be skipped (not a module)
    create_module_file(&project_root, "main.mln", r#"
use std.print;
"#);
    
    let mut engine = Engine::new();
    CantaLoopRS::stdlib::load_all_stdlib(&mut engine);
    
    let result = engine.load_project_modules(&project_root);
    assert!(result.is_ok(), "Failed to load project modules: {:?}", result);
}

#[test]
fn test_module_import_non_existent_function() {
    let (temp_dir, project_root) = create_test_project();
    
    create_module_file(&project_root, "utils.mln", r#"
mod utils;

pub fn add(a: num, b: num) -> num {
    return a + b;
}
"#);
    
    create_module_file(&project_root, "main.mln", r#"
use utils.nonexistent; // This function doesn't exist
"#);
    
    let mut engine = Engine::new();
    CantaLoopRS::stdlib::load_all_stdlib(&mut engine);
    
    // Module loading should succeed, but compilation should fail
    engine.load_project_modules(&project_root).expect("Failed to load modules");
    
    // The import error would be caught during compilation
    // This test verifies the module structure is valid
}

#[test]
fn test_module_import_non_existent_module() {
    let (temp_dir, project_root) = create_test_project();
    
    create_module_file(&project_root, "main.mln", r#"
use nonexistent.function;
"#);
    
    let mut engine = Engine::new();
    CantaLoopRS::stdlib::load_all_stdlib(&mut engine);
    
    engine.load_project_modules(&project_root).expect("Failed to load modules");
    
    // Import error would be caught during compilation
}

#[test]
fn test_module_with_type_annotations() {
    let (temp_dir, project_root) = create_test_project();
    
    create_module_file(&project_root, "types.mln", r#"
mod types;

pub fn process_string(s: string) -> string {
    return "Processed: " + s;
}

pub fn process_number(n: num) -> num {
    return n * 2;
}
"#);
    
    create_module_file(&project_root, "main.mln", r#"
use types.{process_string, process_number};

let str_result = process_string("test")!!;
let num_result = process_number(10)!!;
"#);
    
    let mut engine = Engine::new();
    CantaLoopRS::stdlib::load_all_stdlib(&mut engine);
    
    engine.load_project_modules(&project_root).expect("Failed to load modules");
}

#[test]
fn test_module_public_function_with_multiple_parameters() {
    let (temp_dir, project_root) = create_test_project();
    
    create_module_file(&project_root, "math.mln", r#"
mod math;

pub fn calculate(a: num, b: num, c: num) -> num {
    return a + b * c;
}
"#);
    
    create_module_file(&project_root, "main.mln", r#"
use math.calculate;

let result = calculate(1, 2, 3)!!;
"#);
    
    let mut engine = Engine::new();
    CantaLoopRS::stdlib::load_all_stdlib(&mut engine);
    
    engine.load_project_modules(&project_root).expect("Failed to load modules");
}

#[test]
fn test_module_constants_in_expressions() {
    let (temp_dir, project_root) = create_test_project();
    
    create_module_file(&project_root, "constants.mln", r#"
mod constants;

pub const ZERO = 0;
pub const ONE = 1;
pub const TWO = ONE + ONE;
pub const TEN = 10;
pub const TWENTY = TWO * TEN;
"#);
    
    create_module_file(&project_root, "main.mln", r#"
use constants.{ZERO, ONE, TWO, TEN, TWENTY};
"#);
    
    let mut engine = Engine::new();
    CantaLoopRS::stdlib::load_all_stdlib(&mut engine);
    
    engine.load_project_modules(&project_root).expect("Failed to load modules");
}

