pub mod array;
pub mod comparison;
pub mod functional;
pub mod logic;
/// Standard library math module.
pub mod math;
pub mod matrix;
pub mod number;
/// Standard library I/O module.
pub mod std;
pub mod string;

use crate::core::compileSession::CompileSession;
use crate::core::engine::{Engine, StdModule};
use crate::core::hir_lowering::StructDef;
use ::std::collections::HashMap;

/// Load all standard library modules into the engine.
///
/// This function loads all available standard library modules,
/// making them available for import and use in CantaLoop programs.
pub fn load_stdlib_runtime(engine: &mut Engine) {
    engine.load_stdlib(&*math::MATH_MODULE, "");
    engine.load_stdlib(&*std::STD_MODULE, "");
    engine.load_stdlib(&*string::STRING_MODULE, "");
    engine.load_stdlib(&*matrix::MATRIX_MODULE, "");
    engine.load_stdlib(&*array::ARRAY_MODULE, "");
    engine.load_stdlib(&*number::NUMBER_MODULE, "");
    engine.load_stdlib(&*comparison::COMPARISON_MODULE, "");
    engine.load_stdlib(&*logic::LOGIC_MODULE, "");
    engine.load_stdlib(&*functional::FUNCTIONAL_MODULE, "");
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
    load_stdlib_module_for_compile(session, &*matrix::MATRIX_MODULE, "");
    load_stdlib_module_for_compile(session, &*array::ARRAY_MODULE, "");
    load_stdlib_module_for_compile(session, &*number::NUMBER_MODULE, "");
    load_stdlib_module_for_compile(session, &*comparison::COMPARISON_MODULE, "");
    load_stdlib_module_for_compile(session, &*logic::LOGIC_MODULE, "");
    load_stdlib_module_for_compile(session, &*functional::FUNCTIONAL_MODULE, "");
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
        // Look up function ID from the context's native_functions
        // Functions are registered with both qualified names (e.g., "matrix.add")
        // and unqualified names (e.g., "add") in native_descriptors
        // Try qualified name first, then unqualified name
        let qualified_name = format!("{}.{}", module_path, func.name);
        let func_id = session.get_native_function_id_by_name(&qualified_name)
            .or_else(|| session.get_native_function_id_by_name(func.name))
            .unwrap_or_else(|| {
                panic!("Function '{}' (or '{}') not found in native_functions. Make sure to call load_stdlib_runtime before compile_with_project.", func.name, qualified_name);
            });

        // Add to module's function map
        module_functions.insert(func.name.to_string(), func_id);
    }

    // Register all structs in this module
    let mut module_structs = HashMap::new();
    for std_struct in &module.structs {
        let struct_def = StructDef {
            name: std_struct.name.to_string(),
            fields: std_struct
                .fields
                .iter()
                .map(|(name, kind)| (name.to_string(), kind.clone()))
                .collect(),
        };
        module_structs.insert(std_struct.name.to_string(), struct_def);
    }

    // Register the module with the CompileSession (functions and structs)
    session.register_module_with_structs(&module_path, module_functions, module_structs);

    // Recursively load submodules
    for submodule in &module.submodules {
        load_stdlib_module_for_compile(session, submodule, &module_path);
    }
}
