use std::sync::Arc;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::core::ast::Program;
use crate::core::compileSession::{CompileContext, CompileSession};
use crate::core::entity_id::EntityId;
use crate::core::hir_lowering::HirAst;
use crate::core::{
    bytecode::OpCode,
    hir_lowering::{CompilerState, FunctionSignature},
    parser::parse_program,
    vm::{Value, ValueHeap, VM},
};

/// Melon project descriptor.
///
/// This is pure metadata describing a melon project.
/// It does not mutate the Engine - it's compiler input, not runtime behavior.
pub struct MelonProject {
    pub name: String,
    pub entry: PathBuf,
    pub scripts: Vec<PathBuf>,
    pub dependencies: Vec<String>,
}

/// Cached bytecode for a function.
///
/// Functions are compiled once and their bytecode is cached with a static lifetime
/// to avoid cloning on each call.
#[derive(Clone)]
pub struct BytecodeFunction {
    /// The compiled bytecode instructions for this function.
    pub code: &'static [OpCode],
    /// Variable IDs for function parameters, in order.
    pub param_var_ids: Vec<u32>,
}

#[derive(Clone)]
pub enum Arity {
    Fixed(usize),
    Variadic { min: usize },
}

/// Standard library function descriptor.
///
/// This is pure metadata describing a standard library function.
/// It does not mutate the Engine - it's compiler input, not runtime behavior.
pub struct StdFunction {
    /// The name of the function (e.g., "round", "floor")
    pub name: &'static str,
    /// The function's type signature
    pub signature: FunctionSignature,
    /// The function's arity
    pub arity: Arity,
    /// The function implementation
    pub impl_fn: Arc<dyn Fn(Vec<Value>, &mut ValueHeap) -> Value + Send + Sync>,
    /// Documentation for this function
    pub docs: Option<crate::core::cst::DocBlock>,
}

/// Standard library struct descriptor.
///
/// This is pure metadata describing a standard library struct.
/// It does not mutate the Engine - it's compiler input, not runtime behavior.
pub struct StdStruct {
    /// The name of the struct (e.g., "Point", "Color")
    pub name: &'static str,
    /// The fields of the struct: (field_name, field_type)
    pub fields: Vec<(&'static str, crate::core::hir_lowering::ValueKind)>,
    /// Methods defined on this struct (for future use)
    /// Currently not exposed in syntax, but reserved for future member function support
    pub methods: Vec<StdFunction>,
    /// Documentation for this struct
    pub docs: Option<crate::core::cst::DocBlock>,
}

/// Standard library module descriptor.
///
/// This is pure metadata describing a standard library module.
/// Modules can contain functions, structs, and submodules, forming a tree structure.
pub struct StdModule {
    /// The name of the module (e.g., "math")
    pub name: &'static str,
    /// Functions defined in this module
    pub functions: Vec<StdFunction>,
    /// Structs defined in this module
    pub structs: Vec<StdStruct>,
    /// Submodules nested within this module
    pub submodules: Vec<StdModule>,
    /// Documentation for this module
    pub docs: Option<crate::core::cst::DocBlock>,
}

/// Unified native function representation.
///
/// All native functions are represented with a fixed arity and a function pointer
/// that takes a Vec<Value> and returns a Value. This allows for currying, partial
/// application, and composition without arity-specific wrappers.
pub struct NativeFunction {
    /// The number of arguments this function expects.
    pub arity: Arity,
    /// The function implementation.
    pub func: Box<dyn Fn(Vec<Value>, &mut ValueHeap) -> Value + Send + Sync>,
}

#[derive(Clone)]
pub struct NativeFunctionDescriptor {
    pub id: crate::core::EntityId,
    pub name: String,
    pub signature: FunctionSignature,
    pub arity: Arity,
    /// Documentation for this native function
    pub docs: Option<crate::core::cst::DocBlock>,
}

/// Macro to register a number-based function.
///
/// This macro simplifies the registration of number-based functions by automatically
/// generating the function signature and calling `add_number_function`.
///
/// # Usage:
/// ```ignore
/// add_number_fn!(engine, "round", 2, |args: &[f64]| {
///     args[0].round() + args[1].round()
/// });
/// ```
///
/// The closure receives a slice of f64 values and must return an f64.
#[macro_export]
macro_rules! add_number_fn {
    ($engine:expr, $name:expr, $arity:expr, $body:expr) => {{
        use crate::core::engine::Arity;
        use crate::core::hir_lowering::{FunctionSignature, ValueKind};

        let sig = FunctionSignature {
            params: vec![ValueKind::Number; $arity],
            return_type: Box::new(ValueKind::Number),
            is_effectful: false,
        };

        $engine.add_number_function($name, sig, Arity::Fixed($arity as usize), $body);
    }};
}

/// Macro to register a string-based function.
///
/// This macro simplifies the registration of string-based functions by automatically
/// generating the function signature and calling `add_string_function`.
///
/// # Usage:
/// ```ignore
/// add_string_fn!(engine, "print", 1, |args: &[String]| {
///     println!("{}", args[0]);
///     String::new()
/// });
/// ```
///
/// The closure receives a slice of String values and must return a String.
#[macro_export]
macro_rules! add_string_fn {
    ($engine:expr, $name:expr, $arity:expr, $body:expr) => {{
        use crate::core::engine::Arity;
        use crate::core::hir_lowering::{FunctionSignature, ValueKind};

        let sig = FunctionSignature {
            params: vec![ValueKind::String; $arity],
            return_type: Box::new(ValueKind::String),
            is_effectful: false,
        };

        $engine.add_string_function($name, sig, Arity::Fixed($arity as usize), $body);
    }};
}

/// Macro to register a variadic number-based function.
///
/// This macro simplifies the registration of variadic number-based functions.
/// The function accepts a minimum number of arguments and can take more.
///
/// # Usage:
/// ```ignore
/// add_variadic_number_fn!(engine, "add", 2, |args: &[f64]| {
///     args.iter().sum()
/// });
/// ```
///
/// The closure receives a slice of f64 values (all arguments) and must return an f64.
#[macro_export]
macro_rules! add_variadic_number_fn {
    ($engine:expr, $name:expr, $min:expr, $body:expr) => {{
        use crate::core::engine::Arity;
        use crate::core::hir_lowering::{FunctionSignature, ValueKind};
        use crate::core::vm::Value;

        let sig = FunctionSignature {
            params: vec![ValueKind::Number; $min],
            return_type: Box::new(ValueKind::Number),
            is_effectful: false,
        };

        $engine.add_native_function(
            $name,
            sig,
            Arity::Variadic { min: $min as usize },
            Box::new(move |args, _heap| {
                let nums: Vec<f64> = args
                    .iter()
                    .map(|v| v.as_number().expect("expected numbers"))
                    .collect();
                Value::number($body(&nums))
            }),
        );
    }};
}

/// Helper macro for type conversion (internal use only).
#[macro_export]
macro_rules! __melon_type_kind {
    (num) => { $crate::core::hir_lowering::ValueKind::Number };
    (str) => { $crate::core::hir_lowering::ValueKind::String };
    (bool) => { $crate::core::hir_lowering::ValueKind::Boolean };
    (any) => { $crate::core::hir_lowering::ValueKind::Any };
    (void) => { $crate::core::hir_lowering::ValueKind::Void };
    // Array types: [num], [str], [bool], [any]
    ([num]) => { $crate::core::hir_lowering::ValueKind::Array(Box::new($crate::core::hir_lowering::ValueKind::Number)) };
    ([str]) => { $crate::core::hir_lowering::ValueKind::Array(Box::new($crate::core::hir_lowering::ValueKind::String)) };
    ([bool]) => { $crate::core::hir_lowering::ValueKind::Array(Box::new($crate::core::hir_lowering::ValueKind::Boolean)) };
    ([any]) => { $crate::core::hir_lowering::ValueKind::Array(Box::new($crate::core::hir_lowering::ValueKind::Any)) };
}

/// Helper macro for counting parameters (internal use only).
#[macro_export]
macro_rules! __melon_count {
    () => { 0 };
    ($first:ident) => { 1 };
    ($first:ident $($rest:ident)*) => { 1 + $crate::__melon_count!($($rest)*) };
}

/// Macro to create a native module declaratively.
///
/// This macro reduces boilerplate when defining native modules by automatically
/// constructing StdModule, StdFunction, and FunctionSignature structures.
///
/// # Supported Types
/// - `num` - Number (f64)
/// - `str` - String
/// - `bool` - Boolean
/// - `any` - Any type
/// - `[num]`, `[str]`, `[bool]`, `[any]` - Array types
///
/// # Usage:
/// ```ignore
/// pub static NUMBER_MODULE: StdModule = melon_module! {
///     module number {
///         fn add(a: num, b: num) -> num {
///             |args, _heap| {
///                 let a = args[0].as_number().expect("expected number");
///                 let b = args[1].as_number().expect("expected number");
///                 Value::number(a + b)
///             }
///         }
///     }
/// };
/// ```
///
/// # Native Function Rules (Guardrails)
///
/// Native functions must follow these rules to ensure determinism and safety:
///
/// 1. **No State Storage**: Do not store `Value` outside the call scope
/// 2. **No Heap References**: Do not keep references to `ValueHeap` after the call
/// 3. **No VM Mutation**: Do not mutate VM state (only use the heap for temporary allocations)
/// 4. **Pure Effects**: All effects must happen via return values
///
/// These rules ensure:
/// - Determinism (same inputs = same outputs)
/// - Replayability (functions can be re-executed safely)
/// - Debuggability (no hidden state)
///
/// The function body should be a closure that takes `&[Value], &mut ValueHeap` and returns `Value`.
#[macro_export]
macro_rules! melon_module {
    (
        module $module_name:ident {
            $($functions:tt)*
        }
    ) => {{
        use $crate::core::engine::{Arity, StdFunction, StdModule};
        use $crate::core::hir_lowering::FunctionSignature;
        use std::sync::Arc;

        let mut functions_vec: Vec<StdFunction> = Vec::new();
        $crate::__melon_parse_functions!(functions_vec $($functions)*);
        
        StdModule {
            name: stringify!($module_name),
            functions: functions_vec,
            structs: vec![],
            submodules: vec![],
            docs: None,
        }
    }};
}

#[macro_export]
macro_rules! __melon_parse_functions {
    // Effectful function with ~>
    (
        $vec:ident
        fn $fn_name:ident($($param_name:ident: $param_type:ident),*) ~> $return_type:ident {
            $impl:expr
        }
        $($rest:tt)*
    ) => {
        $vec.push(StdFunction {
            name: stringify!($fn_name),
            signature: FunctionSignature {
                params: vec![
                    $(
                        $crate::__melon_type_kind!($param_type)
                    ),*
                ],
                return_type: Box::new($crate::__melon_type_kind!($return_type)),
                is_effectful: true,
            },
            arity: Arity::Fixed($crate::__melon_count!($($param_name)*)),
            impl_fn: Arc::new($impl),
            docs: None,
        });
        $crate::__melon_parse_functions!($vec $($rest)*);
    };
    
    // Pure function with ->
    (
        $vec:ident
        fn $fn_name:ident($($param_name:ident: $param_type:ident),*) -> $return_type:ident {
            $impl:expr
        }
        $($rest:tt)*
    ) => {
        $vec.push(StdFunction {
            name: stringify!($fn_name),
            signature: FunctionSignature {
                params: vec![
                    $(
                        $crate::__melon_type_kind!($param_type)
                    ),*
                ],
                return_type: Box::new($crate::__melon_type_kind!($return_type)),
                is_effectful: false,
            },
            arity: Arity::Fixed($crate::__melon_count!($($param_name)*)),
            impl_fn: Arc::new($impl),
            docs: None,
        });
        $crate::__melon_parse_functions!($vec $($rest)*);
    };
    
    // End of functions
    ($vec:ident) => {};
}

#[derive(Clone)]
pub struct RunArtifacts {
    pub ast: Program,
    pub hir: HirAst,
    pub functions: HashMap<u32, Vec<OpCode>>,
    pub main: Vec<OpCode>,
    /// Compiled bytecode functions (with static lifetimes for VM)
    pub bytecode_functions: HashMap<u32, BytecodeFunction>,
}

/// Main engine that orchestrates the compilation and execution pipeline.
///
/// Handles:
/// - Parsing source code
/// - Type checking and HIR generation
/// - Bytecode compilation
/// - VM execution
/// - Built-in function registration
/// Struct information for compile-time registration
struct ModuleStructInfo {
    name: String,
    fields: Vec<(String, crate::core::hir_lowering::ValueKind)>,
}

pub struct Engine {
    pub functions: HashMap<crate::core::EntityId, NativeFunction>,
    pub native_descriptors: Vec<NativeFunctionDescriptor>,
    /// Registered structs by module (for compile-time registration)
    registered_structs: HashMap<String, Vec<ModuleStructInfo>>,
    /// ID generator for native functions
    id_generator: crate::core::EntityIdGenerator,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            functions: HashMap::new(),
            native_descriptors: vec![],
            registered_structs: HashMap::new(),
            id_generator: crate::core::EntityIdGenerator::new(),
        }
    }

    pub fn native_function_descriptors(&self) -> &[NativeFunctionDescriptor] {
        &self.native_descriptors
    }

    /// Register a native function with the given arity and implementation.
    ///
    /// This is the primary method for registering native functions. The function
    /// receives a Vec<Value> of arguments (which should match the arity) and returns
    /// a Value result.
    ///
    /// Returns the function ID that was assigned.
    pub fn add_native_function(
        &mut self,
        name: &str,
        signature: FunctionSignature,
        arity: Arity,
        func: Box<dyn Fn(Vec<Value>, &mut ValueHeap) -> Value + Send + Sync>,
    ) -> crate::core::EntityId {
        // Use EntityIdGenerator to create a native function ID
        let id = self.id_generator.next_native_func();

        self.functions.insert(
            id,
            NativeFunction {
                arity: arity.clone(),
                func,
            },
        );

        self.native_descriptors.push(NativeFunctionDescriptor {
            id,
            name: name.to_string(),
            signature,
            arity: arity.clone(),
            docs: None,
        });

        id
    }

    /// Add a native function without registering it globally by name.
    /// Used for module functions that should only be accessible via their full path or imports.
    fn add_native_function_no_register(
        &mut self,
        _signature: FunctionSignature,
        arity: Arity,
        func: Box<dyn Fn(Vec<Value>, &mut ValueHeap) -> Value + Send + Sync>,
    ) -> EntityId {
        // Use EntityIdGenerator to create a native function ID
        let id = self.id_generator.next_native_func();

        self.functions.insert(id, NativeFunction { arity, func });

        // DO NOT register globally - only register with full path in load_stdlib
        id
    }

    /// Register a string-based function (for I/O functions like print).
    ///
    /// The function receives arguments as strings and returns a string.
    /// This is useful for functions that deal with text I/O.
    pub fn add_string_function<F>(
        &mut self,
        name: &str,
        signature: FunctionSignature,
        arity: Arity,
        func: F,
    ) where
        F: Fn(&[String]) -> String + 'static + Send + Sync,
    {
        let func_box = Box::new(move |args: Vec<Value>, heap: &mut ValueHeap| -> Value {
            let args_str: Vec<String> = args.iter().map(|v| v.value_to_string(heap)).collect();
            let result = func(&args_str);
            Value::string_with_heap(result, heap)
        });
        self.add_native_function(name, signature, arity, func_box);
    }

    /// Register a number-based function (for math functions with variable arity).
    ///
    /// The function receives arguments as f64 values and returns an f64.
    /// Panics if any argument is not a number.
    pub fn add_number_function<F>(
        &mut self,
        name: &str,
        signature: FunctionSignature,
        arity: Arity,
        func: F,
    ) where
        F: Fn(&[f64]) -> f64 + 'static + Send + Sync,
    {
        let func_box = Box::new(move |args: Vec<Value>, _heap: &mut ValueHeap| -> Value {
            let args_num: Vec<f64> = args
                .iter()
                .map(|v| {
                    v.as_number()
                        .expect("NumberFunction expects all arguments to be numbers")
                })
                .collect();
            let result = func(&args_num);
            Value::number(result)
        });
        self.add_native_function(name, signature, arity, func_box);
    }


    /// Register a native module into the engine.
    ///
    /// This is the unified entry point for registering native modules (stdlib, project-local, or extensions).
    /// It recursively registers the module and all its submodules, making functions available for compilation.
    ///
    /// # Arguments
    /// * `module` - The native module descriptor to register
    /// * `base_path` - The base path prefix for this module (empty for top-level)
    ///
    /// # Important
    /// - This happens before parsing user code
    /// - Modules are immutable afterward
    /// - HIR sees everything up front
    /// - Import resolution becomes deterministic
    pub fn register_module(&mut self, module: &StdModule, base_path: &str) {
        // Store struct information for compile-time registration
        let module_path = if base_path.is_empty() {
            module.name.to_string()
        } else {
            format!("{}.{}", base_path, module.name)
        };

        // Extract struct information
        let structs: Vec<ModuleStructInfo> = module.structs.iter().map(|s| {
            ModuleStructInfo {
                name: s.name.to_string(),
                fields: s.fields.iter().map(|(n, k)| (n.to_string(), k.clone())).collect(),
            }
        }).collect();
        if !structs.is_empty() {
            self.registered_structs.insert(module_path.clone(), structs);
        }

        // Delegate to the existing implementation for runtime registration
        self.load_stdlib(module, base_path);
    }

    /// Register multiple native modules at once.
    ///
    /// This is a convenience function for registering multiple modules,
    /// useful when loading project-native modules.
    ///
    /// # Example
    /// ```rust,ignore
    /// use cantaloop::core::engine::Engine;
    /// use my_project_native::{MYMATH_MODULE, MYUTILS_MODULE};
    ///
    /// let mut engine = Engine::new();
    /// engine.register_modules(&[&MYMATH_MODULE, &MYUTILS_MODULE]);
    /// ```
    pub fn register_modules(&mut self, modules: &[&StdModule]) {
        for module in modules {
            self.register_module(module, "");
        }
    }

    /// Load project-local native modules from a project directory.
    ///
    /// **Deprecated**: Use `ProjectLoader::load_native_modules` instead.
    /// This method is kept for backward compatibility and delegates to ProjectLoader.
    #[deprecated(note = "Use ProjectLoader::load_native_modules instead")]
    pub fn load_project_native_modules(&mut self, project_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
        crate::core::projectLoader::ProjectLoader::load_native_modules(self, project_root)
    }

    /// Register native modules (including project-native modules) in a CompileSession.
    ///
    /// This builds module maps from native_descriptors and registers them in the session,
    /// similar to how stdlib modules are registered. This ensures native modules are
    /// available for compile-time resolution.
    fn register_native_modules_for_compile(engine: &Engine, session: &mut CompileSession) {
        use std::collections::HashMap;
        use crate::core::hir_lowering::StructDef;

        // Build module maps from native_descriptors
        // Functions are registered with qualified names like "std.print", "math.round", "mymath.hypot"
        let mut native_modules: HashMap<String, (HashMap<String, EntityId>, HashMap<String, StructDef>)> = HashMap::new();

        for native in engine.native_function_descriptors() {
            // Look for qualified names (e.g., "std.print", "math.round", "mymath.hypot")
            if let Some(dot_pos) = native.name.find('.') {
                let module_name = &native.name[..dot_pos];
                let function_name = &native.name[dot_pos + 1..];

                let (functions, _) = native_modules
                    .entry(module_name.to_string())
                    .or_insert_with(|| (HashMap::new(), HashMap::new()));
                functions.insert(function_name.to_string(), native.id);
            }
        }
        
        // Add structs from registered modules
        for (module_name, struct_infos) in &engine.registered_structs {
            if let Some((_, structs_map)) = native_modules.get_mut(module_name) {
                for struct_info in struct_infos {
                    let struct_def = StructDef {
                        name: struct_info.name.clone(),
                        fields: struct_info.fields.clone(),
                    };
                    structs_map.insert(struct_info.name.clone(), struct_def);
                }
            }
        }
        
        // Register native modules with their functions and structs
        // Note: stdlib modules are already registered by load_stdlib_for_compile,
        // so we only need to register non-stdlib modules (like project-native modules)
        // We check if the module exists in HirBuilder's modules map to avoid duplicates
        for (module_name, (functions, structs)) in native_modules {
            // Check if module is already registered by checking HirBuilder's modules map
            if !session.has_module(&module_name) {
                session.register_module_with_structs(&module_name, functions, structs);
            }
        }
    }

    /// Load a standard library module into the engine.
    ///
    /// This method recursively loads a module and all its submodules,
    /// registering functions with their full module paths (e.g., "math.round").
    ///
    /// # Arguments
    /// * `module` - The standard library module descriptor to load
    /// * `base_path` - The base path prefix for this module (empty for top-level)
    ///
    /// # Deprecated
    /// Use `register_module` instead. This method is kept for backward compatibility.
    pub fn load_stdlib(&mut self, module: &StdModule, base_path: &str) {
        // Build the full module path
        let module_path = if base_path.is_empty() {
            module.name.to_string()
        } else {
            format!("{}.{}", base_path, module.name)
        };

        // Register all functions in this module
        let mut module_functions = HashMap::new();
        for func in &module.functions {
            // Register the function with the engine WITHOUT global registration
            // Clone the Arc to move it into the closure (Arc clone is cheap - just increments ref count)
            let func_impl = func.impl_fn.clone();
            let func_id = self.add_native_function_no_register(
                func.signature.clone(),
                func.arity.clone(),
                Box::new(move |args: Vec<Value>, heap: &mut ValueHeap| -> Value {
                    // Call the Arc-wrapped function directly - this is safe because Arc is Send + Sync
                    func_impl(args, heap)
                }),
            );

            // Also add to native_descriptors so it's available in CompileContext for compile-time resolution
            // Use module-qualified name (e.g., "std.print", "mymath.hypot") so it can be found during compilation
            let qualified_name = format!("{}.{}", module_path, func.name);
            self.native_descriptors.push(NativeFunctionDescriptor {
                id: func_id,
                name: qualified_name.clone(),
                signature: func.signature.clone(),
                arity: func.arity.clone(),
                docs: func.docs.clone(),
            });
            // Also add with just the function name for backward compatibility
            self.native_descriptors.push(NativeFunctionDescriptor {
                id: func_id,
                name: func.name.to_string(),
                signature: func.signature.clone(),
                arity: func.arity.clone(),
                docs: func.docs.clone(),
            });

            // Add to module's function map
            module_functions.insert(func.name.to_string(), func_id);

            // Note: Function registration with HirBuilder happens when CompileSession is created
            // The function is already registered with Engine via add_native_function_no_register
        }

        // Note: Module registration with HirBuilder happens when CompileSession is created
        // For now, we just track modules in Engine's native_descriptors

        // Recursively register submodules
        for submodule in &module.submodules {
            self.register_module(submodule, &module_path);
        }
    }


    pub fn get_function(&self, id: u32) -> crate::core::vm::Value {
        // Functions are now separate from constants
        crate::core::vm::Value::function(id)
    }

    pub fn compile_and_run(self: Arc<Self>, file_path: &str) {
        let artifacts = self.compile(file_path);
        self.run(artifacts.unwrap());
    }

    /// Runs a CantaLoop program from a RunArtifacts.
    pub fn run(self: Arc<Self>, artifacts: RunArtifacts) {
        let main = artifacts.main;
        let bytecode_functions = artifacts.bytecode_functions;
        let hir = artifacts.hir;
        let mut vm = VM::new(self.clone(), bytecode_functions, hir, main);
        vm.run();
    }

    /// Executes a CantaLoop program from a file.
    ///
    /// Performs the compilation pipeline: parse → type check → compile.
    pub fn compile(&self, file_path: &str) -> Result<RunArtifacts, Box<dyn std::error::Error>> {
        self.compile_with_project(file_path, None)
    }

    /// Compiles a CantaLoop program from a file, optionally loading project modules.
    ///
    /// If `project_root` is provided, loads all modules from the project's src/ directory
    /// before compiling, allowing imports to resolve correctly.
    ///
    /// **LIFECYCLE: Creates a new CompileSession for each compile.**
    /// This ensures clean state and prevents redeclaration bugs.
    pub fn compile_with_project(
        &self,
        file_path: &str,
        project_root: Option<&Path>,
    ) -> Result<RunArtifacts, Box<dyn std::error::Error>> {
        let ctx = CompileContext {
            native_functions: self.native_function_descriptors(),
            is_native_function: &|id| self.functions.contains_key(&id),
        };

        // Create a fresh CompileSession for this compile (single-use)
        let mut session = CompileSession::new(ctx);

        // Load stdlib modules for compile-time resolution
        crate::stdlib::load_stdlib_for_compile(&mut session);

        // Register native modules (including project-native modules) in CompileSession
        // Build module maps from native_descriptors (similar to compile_for_lsp)
        Self::register_native_modules_for_compile(self, &mut session);

        // Load project modules if project_root is provided
        if let Some(project_root) = project_root {
            // Load project Melon modules
            if let Err(e) = session.load_project_modules(project_root) {
                eprintln!("Warning: Failed to load project modules: {}", e);
            }
        }

        session.compile(file_path)
    }

    /// Compile source code for LSP, returning compiler state without mutating engine state.
    ///
    /// This method performs the full compilation pipeline (parse → semantic analysis)
    /// and returns a CompilerState containing AST, HIR, diagnostics, and symbol table.
    /// It uses a fresh HirBuilder and registers all built-in functions.
    ///
    /// Unlike `run()`, this method does not panic on errors but collects them in diagnostics.
    ///
    /// If `project_root` is provided, loads all modules from the project's src/ directory
    /// before compiling, allowing imports to resolve correctly.
    ///
    /// If `current_file` is provided, that file will be skipped when loading modules
    /// Extract the module name from AST (if present).
    fn extract_module_name_from_ast(ast: &Program) -> Option<String> {
        use crate::core::ast::Statement;
        for block in &ast.blocks {
            for stmt in &block.statements {
                if let Statement::Mod { identifier } = stmt {
                    return Some(identifier.name.clone());
                }
            }
        }
        None
    }


    /// Compile source code for LSP (Language Server Protocol) usage.
    /// 
    /// This method creates a CompilerState that the LSP can use for IDE features.
    /// Unlike regular compilation, this doesn't execute code - it only builds semantic information.
    /// 
    /// Returns the compiler state with AST, HIR, diagnostics, and symbol table.
    /// 
    /// If `project_root` is provided, it will attempt to load project modules.
    /// If `current_file` is provided, that file will be skipped during module loading
    /// to avoid duplicate declarations.
    pub fn compile_for_lsp(
        &self,
        src: &str,
        project_root: Option<&Path>,
        current_file: Option<&Path>,
    ) -> Result<CompilerState, pest::error::Error<crate::core::parser::Rule>> {
        use crate::core::compileSession::CompileContext;
        use crate::core::hir_lowering::HirBuilder;
    
        // 1. Parse current file to CST first (preserves spans for LSP)
        use crate::core::cst::{parse_cst_program, lower_cst_to_ast};
        let (cst, cst_docs) = parse_cst_program(src)?;
        
        // 2. Lower CST to AST (for semantic analysis)
        let (ast, lowering_docs) = lower_cst_to_ast(cst.clone(), cst_docs)
            .map_err(|e| pest::error::Error::new_from_pos(
                pest::error::ErrorVariant::CustomError { message: e },
                pest::Position::from_start("")
            ))?;
        let ast_for_hir = ast.clone();
    
        // 3. Build compile context from Engine
        let ctx = CompileContext {
            native_functions: self.native_function_descriptors(),
            is_native_function: &|id| self.functions.contains_key(&id),
        };
    
        // 4. Fresh HIR builder
        let mut hir_builder = HirBuilder::new();
        let mut diagnostics = Vec::new();
    
        /* ---------------------------------------------
         * Register stdlib (functions + modules)
         * --------------------------------------------- */
    
        // First, register all builtin functions
        for native in self.native_function_descriptors() {
            hir_builder.register_builtin_function(
                &native.name,
                native.signature.clone(),
                native.id,
            );
        }
    
        // Build stdlib module maps from native_descriptors
        // Functions are registered with qualified names like "std.print", "math.round", etc.
        let mut stdlib_modules: HashMap<String, HashMap<String, EntityId>> = HashMap::new();
        eprintln!("Building modules from {} native descriptors", self.native_function_descriptors().len());
        for native in self.native_function_descriptors() {
            // Look for qualified names (e.g., "std.print", "math.round", "mymath.hypot")
            if let Some(dot_pos) = native.name.find('.') {
                let module_name = &native.name[..dot_pos];
                let function_name = &native.name[dot_pos + 1..];
                eprintln!("  Found function: {}.{} -> module '{}'", module_name, function_name, module_name);
                stdlib_modules
                    .entry(module_name.to_string())
                    .or_insert_with(HashMap::new)
                    .insert(function_name.to_string(), native.id);
            }
        }

        // Debug: print registered modules
        if !stdlib_modules.is_empty() {
            eprintln!("Registered modules: {:?}", stdlib_modules.keys().collect::<Vec<_>>());
        }

        // Register stdlib modules with their functions
        for (module_name, functions) in stdlib_modules {
            eprintln!("Registering module '{}' with {} functions", module_name, functions.len());
            hir_builder.register_module(&module_name, functions, HashMap::new(), HashMap::new());
        }
    
        /* ---------------------------------------------
         * Load project modules (except current file)
         * --------------------------------------------- */
    
        if let Some(root) = project_root {
            let src_dir = root.join("src");
    
            if src_dir.exists() {
                for entry in walkdir::WalkDir::new(&src_dir)
                    .into_iter()
                    .filter_map(Result::ok)
                    .filter(|e| e.path().extension().map(|e| e == "mln").unwrap_or(false))
                {
                    let path = entry.path();
    
                    // Skip the file currently edited in the LSP (normalize paths for comparison)
                    if let Some(current) = current_file {
                        match (path.canonicalize(), current.canonicalize()) {
                            (Ok(path_canon), Ok(current_canon)) if path_canon == current_canon => continue,
                            _ => {
                                // If canonicalization fails, fall back to direct comparison
                                if path == current {
                                    continue;
                                }
                            }
                        }
                    }
    
                    let source = match std::fs::read_to_string(path) {
                        Ok(s) => s,
                        Err(_) => continue, // Skip files that can't be read
                    };
    
                    let module_ast = match parse_program(&source) {
                        Ok(ast) => ast,
                        Err(_) => continue, // syntax errors handled separately
                    };
    
                    let module_name =
                        Self::extract_module_name_from_ast(&module_ast)
                            .unwrap_or_else(|| "__unnamed__".to_string());
    
                    // Register module before building it
                    hir_builder.register_module(
                        &module_name,
                        HashMap::new(),
                        HashMap::new(),
                        HashMap::new(),
                    );
    
                    // Set current module and build
                    hir_builder.set_current_module(Some(module_name.clone()));
                    if let Err(err) = hir_builder.build_append(&ctx, module_ast) {
                        diagnostics.push(err);
                    }
                }
            }
        }
    
        /* ---------------------------------------------
         * Compile current file LAST
         * --------------------------------------------- */
    
        let current_module =
            Self::extract_module_name_from_ast(&ast)
                .unwrap_or_else(|| "__main__".to_string());
    
        // Register current module before building it
        hir_builder.register_module(
            &current_module,
            HashMap::new(),
            HashMap::new(),
            HashMap::new(),
        );
    
        hir_builder.set_current_module(Some(current_module.clone()));
        if let Err(err) = hir_builder.build_append(&ctx, ast_for_hir) {
            diagnostics.push(err);
        }
    
        let hir = hir_builder.take_ast();
        
        Ok(CompilerState::new(cst, ast, hir, diagnostics, Some(src), lowering_docs))
    }
    
}
