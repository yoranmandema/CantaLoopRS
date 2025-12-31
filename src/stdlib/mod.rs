/// Standard library math module.
pub mod math;
/// Standard library I/O module.
pub mod std;
pub mod string;

use crate::core::engine::{Engine, StdModule};
use crate::core::compileSession::CompileSession;
use ::std::collections::HashMap;

/// Load all standard library modules into the engine.
/// 
/// This function loads all available standard library modules,
/// making them available for import and use in CantaLoop programs.
pub fn load_stdlib_runtime(engine: &mut Engine) {
    engine.load_stdlib(&*math::MATH_MODULE, "");
    engine.load_stdlib(&*std::STD_MODULE, "");
    engine.load_stdlib(&*string::STRING_MODULE, "");
}

/// Load all standard library modules into the compile session.
/// 
/// This function registers stdlib modules for compile-time resolution,
/// allowing `use std.*`, `use math.*`, and `use string.*` to work during compilation.
/// 
/// It registers:
/// - Module names (e.g., "std", "math", "string")
/// - Function signatures (as builtin functions)
/// - Constants (if any)
/// 
/// Note: This does NOT include runtime closures - those are handled by `load_stdlib_runtime`.
pub fn load_stdlib_for_compile(session: &mut CompileSession) {
    load_stdlib_module_for_compile(session, &*math::MATH_MODULE, "");
    load_stdlib_module_for_compile(session, &*std::STD_MODULE, "");
    load_stdlib_module_for_compile(session, &*string::STRING_MODULE, "");
}

/// Load a single stdlib module into the compile session.
fn load_stdlib_module_for_compile(
    session: &mut CompileSession,
    module: &StdModule,
    base_path: &str,
) {
    // Build the full module path
    let module_path = if base_path.is_empty() {
        module.name.to_string()
    } else {
        format!("{}.{}", base_path, module.name)
    };

    // Register all functions in this module
    let mut module_functions = HashMap::new();
    for func in &module.functions {
        // Look up function ID from CompileSession by matching name
        // Functions are already registered as builtin functions when CompileSession is created
        // We need to find the function ID by matching the name
        let func_id = session.get_function_id_by_name(func.name)
            .unwrap_or_else(|| {
                // If not found, we need to register it
                // This shouldn't happen if runtime stdlib was loaded first
                panic!("Function '{}' not found in CompileSession. Make sure to call load_stdlib_runtime before compile_with_project.", func.name);
            });

        // Add to module's function map
        module_functions.insert(func.name.to_string(), func_id);
    }

    // Register the module with the CompileSession
    session.register_module(&module_path, module_functions);

    // Recursively load submodules
    for submodule in &module.submodules {
        load_stdlib_module_for_compile(session, submodule, &module_path);
    }
}

