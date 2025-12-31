use std::sync::Arc;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::core::ast::Program;
use crate::core::compileSession::{CompileContext, CompileSession};
use crate::core::hir_lowering::HirAst;
use crate::core::{
    ast::{Expression, Literal},
    bytecode::{ByteCodeEmitter, OpCode},
    hir_lowering::{
        CompilerState, ConstantValue, FunctionSignature, HirBuilder, HirError, ValueKind,
    },
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
}

/// Standard library module descriptor.
///
/// This is pure metadata describing a standard library module.
/// Modules can contain functions and submodules, forming a tree structure.
pub struct StdModule {
    /// The name of the module (e.g., "math")
    pub name: &'static str,
    /// Functions defined in this module
    pub functions: Vec<StdFunction>,
    /// Submodules nested within this module
    pub submodules: Vec<StdModule>,
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
    pub id: u32,
    pub name: String,
    pub signature: FunctionSignature,
    pub arity: Arity,
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
pub struct Engine {
    pub functions: HashMap<u32, NativeFunction>,
    pub native_descriptors: Vec<NativeFunctionDescriptor>,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            functions: HashMap::new(),
            native_descriptors: vec![],
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
    ) -> u32 {
        // Create a function ID - note: this should match the function registry
        // For built-in functions, we'll use a special ID range (e.g., starting from 10000)
        let id = 10000 + self.functions.len() as u32;

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
        });

        id
    }

    /// Add a native function without registering it globally by name.
    /// Used for module functions that should only be accessible via their full path or imports.
    fn add_native_function_no_register(
        &mut self,
        signature: FunctionSignature,
        arity: Arity,
        func: Box<dyn Fn(Vec<Value>, &mut ValueHeap) -> Value + Send + Sync>,
    ) -> u32 {
        // Create a function ID - note: this should match the function registry
        // For built-in functions, we'll use a special ID range (e.g., starting from 10000)
        let id = 10000 + self.functions.len() as u32;

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


    /// Extract constant value from an expression if it's a literal.
    /// Returns None for complex expressions.
    fn extract_constant_value_from_expr(expr: &Expression) -> Option<ConstantValue> {
        match expr {
            Expression::Literal(lit) => Some(match lit {
                Literal::Number(n) => ConstantValue::Number(*n),
                Literal::String(s) => ConstantValue::String(s.clone()),
                Literal::Boolean(b) => ConstantValue::Boolean(*b),
            }),
            _ => None, // Complex expressions - can't extract value statically
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
            // Use module-qualified name (e.g., "std.print") so it can be found during compilation
            let qualified_name = format!("{}.{}", module_path, func.name);
            self.native_descriptors.push(NativeFunctionDescriptor {
                id: func_id,
                name: qualified_name.clone(),
                signature: func.signature.clone(),
                arity: func.arity.clone(),
            });
            // Also add with just the function name for backward compatibility
            self.native_descriptors.push(NativeFunctionDescriptor {
                id: func_id,
                name: func.name.to_string(),
                signature: func.signature.clone(),
                arity: func.arity.clone(),
            });

            // Add to module's function map
            module_functions.insert(func.name.to_string(), func_id);

            // Note: Function registration with HirBuilder happens when CompileSession is created
            // The function is already registered with Engine via add_native_function_no_register
        }

        // Note: Module registration with HirBuilder happens when CompileSession is created
        // For now, we just track modules in Engine's native_descriptors

        // Recursively load submodules
        for submodule in &module.submodules {
            self.load_stdlib(submodule, &module_path);
        }
    }


    pub fn get_function(&self, id: u32) -> crate::core::vm::Value {
        // Functions are now separate from constants
        crate::core::vm::Value::function(id)
    }

    pub fn compile_and_run(&self, file_path: &str) {
        let artifacts = self.compile(file_path);
        self.run(artifacts.unwrap());
    }

    /// Runs a CantaLoop program from a RunArtifacts.
    pub fn run(&self, artifacts: RunArtifacts) {
        let main = artifacts.main;
        let bytecode_functions = artifacts.bytecode_functions;
        let hir = artifacts.hir;
        let mut vm = VM::new(self, bytecode_functions, hir, main);
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

        // Load project modules if project_root is provided
        if let Some(project_root) = project_root {
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
    /// to avoid duplicate declarations.
    pub fn compile_for_lsp(
        &self,
        src: &str,
        project_root: Option<&Path>,
        current_file: Option<&Path>,
    ) -> Result<CompilerState, pest::error::Error<crate::core::parser::Rule>> {
        use crate::core::compileSession::CompileContext;
        use crate::core::hir_lowering::HirBuilder;

        // Parse the source code twice - once to keep AST, once for HIR building
        // (This is acceptable for LSP where we need both AST and HIR)
        let ast = parse_program(src)?;
        let ast_for_hir = parse_program(src)?;

        // Create a CompileContext from Engine's native functions
        let ctx = CompileContext {
            native_functions: self.native_function_descriptors(),
            is_native_function: &|id| self.functions.contains_key(&id),
        };

        // Create a fresh HirBuilder and register built-in functions
        let mut hir_builder = HirBuilder::new();

        // Register all built-in functions from Engine
        for native in self.native_function_descriptors() {
            hir_builder.register_builtin_function(
                &native.name,
                native.signature.clone(),
                native.id,
            );
        }

        // If project_root is provided, load all project modules
        // Note: For now, we skip module loading in LSP mode since Engine doesn't maintain modules
        // This can be added later if needed

        // Build HIR (diagnostics are collected during build_append)
        // Set a default module for the file
        hir_builder.set_current_module(Some("__main__".to_string()));
        hir_builder.build_append(&ctx, ast_for_hir).ok();

        let hir = hir_builder.take_ast();

        Ok(CompilerState::new(ast, hir, Vec::new(), Some(src)))
    }
}
