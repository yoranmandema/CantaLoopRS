use std::{collections::HashMap, path::{Path, PathBuf}};
use std::sync::Arc;

use crate::core::{
    bytecode::{ByteCodeEmitter, OpCode},
    parser::parse_program,
    hir_lowering::{CompilerState, FunctionSignature, HirBuilder, HirError, ValueKind},
    vm::{VM, Value, ValueHeap},
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
        use crate::core::hir_lowering::{FunctionSignature, ValueKind};
        use crate::core::engine::Arity;

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
        use crate::core::hir_lowering::{FunctionSignature, ValueKind};
        use crate::core::engine::Arity;

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
        use crate::core::hir_lowering::{FunctionSignature, ValueKind};
        use crate::core::engine::Arity;
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

/// Main engine that orchestrates the compilation and execution pipeline.
/// 
/// Handles:
/// - Parsing source code
/// - Type checking and HIR generation
/// - Bytecode compilation
/// - VM execution
/// - Built-in function registration
pub struct Engine {
    emitter: ByteCodeEmitter,
    hir_builder: HirBuilder,
    pub functions: HashMap<u32, NativeFunction>,
    pub bytecode_functions: HashMap<u32, BytecodeFunction>, // Function constant ID -> bytecode
    loaded_modules: HashMap<String, crate::core::ast::Program>, // Module name -> AST for later compilation
}

impl Engine {
    pub fn new() -> Self {
        Self {
            emitter: ByteCodeEmitter::new(),
            hir_builder: HirBuilder::new(),
            functions: HashMap::new(),
            bytecode_functions: HashMap::new(),
            loaded_modules: HashMap::new(),
        }
    }

    /// Formats a ValueKind for error messages, handling function/thunk types specially.
    fn format_value_kind_for_error(kind: &ValueKind) -> String {
        match kind {
            ValueKind::Number => "Number".to_string(),
            ValueKind::String => "String".to_string(),
            ValueKind::Boolean => "Boolean".to_string(),
            ValueKind::Unknown => "Unknown".to_string(),
            ValueKind::Function(ty) => ty.clone(),
            ValueKind::Thunk(ty) => ty.clone(),
            ValueKind::Void => "Void".to_string(),
        }
    }

    /// Handles HIR errors by converting them to panic messages.
    fn handle_hir_error(error: HirError) -> ! {
        match error {
            HirError::TypeError(msg) => {
                panic!("Type error: {}", msg);
            }
            HirError::TypeMismatch { variable, expected, actual } => {
                let expected_str = Self::format_value_kind_for_error(&expected);
                let actual_str = Self::format_value_kind_for_error(&actual);
                panic!(
                    "Type mismatch error: Cannot assign {} to variable '{}' which is of type {}",
                    actual_str, variable, expected_str
                );
            }
            HirError::UnknownVariable(msg) => {
                panic!("Semantic error: {}", msg);
            }
            HirError::VariableAlreadyDeclared(msg) => {
                panic!("Semantic error: {}", msg);
            }
            HirError::NotImplemented => {
                panic!("Semantic error: Feature not implemented");
            }
            HirError::BinaryOpTypeError { operator, lhs_type, rhs_type, expected } => {
                let lhs_str = Self::format_value_kind_for_error(&lhs_type);
                let rhs_str = Self::format_value_kind_for_error(&rhs_type);
                panic!(
                    "Binary operation type error: Operator '{}' expects {}, but got {} and {}",
                    operator, expected, lhs_str, rhs_str
                );
            }
        }
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

        self.functions.insert(id, NativeFunction { arity, func });
        
        // Register the built-in function in the HIR builder's function registry
        self.hir_builder.register_builtin_function(name, signature, id);
        
        id
    }

    /// Register a string-based function (for I/O functions like print).
    /// 
    /// The function receives arguments as strings and returns a string.
    /// This is useful for functions that deal with text I/O.
    pub fn add_string_function<F>(&mut self, name: &str, signature: FunctionSignature, arity: Arity, func: F)
    where
        F: Fn(&[String]) -> String + 'static + Send + Sync,
    {
        let func_box = Box::new(move |args: Vec<Value>, heap: &mut ValueHeap| -> Value {
            let args_str: Vec<String> = args
                .iter()
                .map(|v| v.value_to_string(heap))
                .collect();
            let result = func(&args_str);
            Value::string_with_heap(result, heap)
        });
        self.add_native_function(name, signature, arity, func_box);
    }

    /// Register a number-based function (for math functions with variable arity).
    /// 
    /// The function receives arguments as f64 values and returns an f64.
    /// Panics if any argument is not a number.
    pub fn add_number_function<F>(&mut self, name: &str, signature: FunctionSignature, arity: Arity, func: F)
    where
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

    /// Register a module that can be imported.
    /// 
    /// # Arguments
    /// * `path` - Dot-separated module path (e.g., "math.utils")
    /// * `functions` - Map of function names to their function IDs
    /// 
    /// Function IDs should be obtained by registering functions first using
    /// `add_native_function`, `add_string_function`, or `add_number_function`.
    /// 
    /// # Example
    /// ```ignore
    /// let mut engine = Engine::new();
    /// add_number_fn!(engine, "square", 1, |args: &[f64]| args[0] * args[0]);
    /// add_number_fn!(engine, "cube", 1, |args: &[f64]| args[0] * args[0] * args[0]);
    /// 
    /// // Get function IDs by name (IDs start at 10000 and increment)
    /// let square_id = engine.get_function_id_by_name("square").unwrap();
    /// let cube_id = engine.get_function_id_by_name("cube").unwrap();
    /// 
    /// let mut math_utils = HashMap::new();
    /// math_utils.insert("square".to_string(), square_id);
    /// math_utils.insert("cube".to_string(), cube_id);
    /// engine.register_module("math.utils", math_utils);
    /// ```
    pub fn register_module(&mut self, path: &str, functions: HashMap<String, u32>) {
        self.hir_builder.register_module(path, functions, HashMap::new());
    }

    /// Get the function ID for a registered function by name.
    /// 
    /// Returns None if the function is not found.
    pub fn get_function_id_by_name(&self, name: &str) -> Option<u32> {
        self.hir_builder.resolve_function(name)
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
            // Register the function with the engine (using the base name)
            // This allows the function to be resolved by its simple name if imported
            // Clone the Arc to move it into the closure (Arc clone is cheap - just increments ref count)
            let func_impl = func.impl_fn.clone();
            let func_id = self.add_native_function(
                func.name,
                func.signature.clone(),
                func.arity.clone(),
                Box::new(move |args: Vec<Value>, heap: &mut ValueHeap| -> Value {
                    // Call the Arc-wrapped function directly - this is safe because Arc is Send + Sync
                    func_impl(args, heap)
                }),
            );

            // Add to module's function map
            module_functions.insert(func.name.to_string(), func_id);
            
            // Also register the function with its full path in the HIR builder
            // This allows both "math.round" and just "round" (if imported) to work
            let full_func_name = format!("{}.{}", module_path, func.name);
            self.hir_builder.register_builtin_function(&full_func_name, func.signature.clone(), func_id);
        }

        // Register this module with the HIR builder
        if !module_functions.is_empty() {
            self.hir_builder.register_module(&module_path, module_functions, HashMap::new());
        }

        // Recursively load submodules
        for submodule in &module.submodules {
            self.load_stdlib(submodule, &module_path);
        }
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
    pub fn compile_for_lsp(&self, src: &str, project_root: Option<&Path>) -> Result<CompilerState, pest::error::Error<crate::core::parser::Rule>> {
        // Parse the source code twice - once to keep AST, once for HIR building
        // (This is acceptable for LSP where we need both AST and HIR)
        let ast = parse_program(src)?;
        let ast_for_hir = parse_program(src)?;

        // Create a fresh HirBuilder and register built-in functions
        let mut hir_builder = HirBuilder::new();
        
        // Copy all modules from the engine's hir_builder (needed for import resolution)
        hir_builder.copy_modules_from(&self.hir_builder);
        
        // Copy built-in function registrations from the engine's hir_builder
        // Built-in functions have IDs >= 10000
        for (func_id, func) in &self.hir_builder.ast.functions {
            if *func_id >= 10000 {
                // This is a built-in function - register it in the new HirBuilder
                hir_builder.register_builtin_function(&func.name, func.signature.clone(), *func_id);
            }
        }
        
        // If project_root is provided, load all project modules
        if let Some(project_root) = project_root {
            Self::load_project_modules_for_lsp(&mut hir_builder, &self.hir_builder, project_root);
        }
        
        // Build HIR and collect errors
        let mut diagnostics = Vec::new();
        let hir_result = hir_builder.build(ast_for_hir);
        
        // Extract the HirAst from the builder (we own it)
        let hir = match hir_result {
            Ok(_) => hir_builder.ast,
            Err(e) => {
                diagnostics.push(e);
                // Return partial HIR even with errors
                hir_builder.ast
            }
        };

        Ok(CompilerState::new(ast, hir, diagnostics, Some(src)))
    }
    
    /// Load project modules for LSP compilation (non-mutating version).
    /// This loads modules into a HirBuilder without mutating the engine.
    fn load_project_modules_for_lsp(
        hir_builder: &mut HirBuilder,
        source_hir_builder: &HirBuilder,
        project_root: &Path,
    ) {
        let src_dir = project_root.join("src");
        
        if !src_dir.exists() {
            return; // No src directory, no modules to load
        }
        
        // Find all .mln files in src/
        let entries = match std::fs::read_dir(&src_dir) {
            Ok(entries) => entries,
            Err(_) => return,
        };
        
        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let path = entry.path();
            
            if path.extension().and_then(|s| s.to_str()) == Some("mln") {
                // Skip the main file (it's not a module)
                if path.file_name().and_then(|n| n.to_str()) == Some("main.mln") {
                    continue;
                }
                
                // Try to load this file as a module
                if let Err(_) = Self::load_module_for_lsp(hir_builder, source_hir_builder, &path) {
                    // Silently skip modules that fail to load (they might have errors)
                    continue;
                }
            }
        }
    }
    
    /// Load a single module file for LSP (non-mutating version).
    fn load_module_for_lsp(
        hir_builder: &mut HirBuilder,
        source_hir_builder: &HirBuilder,
        file_path: &Path,
    ) -> Result<String, Box<dyn std::error::Error>> {
        use crate::core::ast::Statement;
        
        // Read and parse the file
        let content = std::fs::read_to_string(file_path)?;
        
        // Quick check: if the file doesn't start with "mod", skip it
        if !content.trim_start().starts_with("mod") {
            return Err("File does not start with 'mod' declaration".into());
        }
        
        let ast = parse_program(&content)?;
        
        // Find the mod statement to get the module name
        let mut module_name = None;
        for block in &ast.blocks {
            for stmt in &block.statements {
                if let Statement::Mod { identifier } = stmt {
                    module_name = Some(identifier.clone());
                    break;
                }
            }
            if module_name.is_some() {
                break;
            }
        }
        
        let module_name = module_name.ok_or_else(|| "Module file missing 'mod' declaration")?;
        
        // Compile the module to get HIR (this processes all statements)
        let mut module_hir_builder = HirBuilder::new();
        // Copy stdlib modules so imports work
        module_hir_builder.copy_modules_from(source_hir_builder);
        // Copy built-in functions
        for (func_id, func) in &source_hir_builder.ast.functions {
            if *func_id >= 10000 {
                module_hir_builder.register_builtin_function(&func.name, func.signature.clone(), *func_id);
            }
        }
        
        // Parse again for HIR building
        let ast_for_hir = parse_program(&content)?;
        let hir_result = module_hir_builder.build(ast_for_hir);
        if let Err(_) = hir_result {
            // If module compilation fails, we still register it with placeholder IDs
            // so that the module name can be found (even if its members can't)
        }
        
        // Extract public function names and constant names from AST
        let mut pub_function_names = Vec::new();
        let mut pub_constant_names = Vec::new();
        for block in &ast.blocks {
            for stmt in &block.statements {
                match stmt {
                    Statement::FunctionDeclaration { identifier, pub_visibility, .. } => {
                        if *pub_visibility {
                            pub_function_names.push(identifier.clone());
                        }
                    }
                    Statement::Const { identifier, pub_visibility, .. } => {
                        if *pub_visibility {
                            pub_constant_names.push(identifier.clone());
                        }
                    }
                    _ => {}
                }
            }
        }
        
        // Register the module with placeholder function IDs (0) and constant IDs (0)
        // The actual IDs will be resolved when the module is actually compiled
        let mut module_functions = HashMap::new();
        for func_name in pub_function_names {
            module_functions.insert(func_name, 0); // Placeholder
        }
        
        let mut module_constants = HashMap::new();
        for const_name in pub_constant_names {
            module_constants.insert(const_name, 0); // Placeholder
        }
        
        // Register the module (even with placeholder IDs, so member access can find the module)
        if !module_functions.is_empty() || !module_constants.is_empty() {
            hir_builder.register_module(&module_name, module_functions, module_constants);
        }
        
        Ok(module_name)
    }   

    pub fn get_constant(&self, id: u32, heap: &mut crate::core::vm::ValueHeap) -> crate::core::vm::Value {
        use crate::core::hir_lowering::ConstantValue;

        let c = self
            .hir_builder
            .ast
            .constants
            .iter()
            .find(|c| c.id == id)
            .expect("Constant not found");

        // Constants no longer contain functions - only data
        match (&c.kind, &c.value) {
            (ValueKind::Number, ConstantValue::Number(n)) => crate::core::vm::Value::number(*n),
            (ValueKind::String, ConstantValue::String(s)) => {
                crate::core::vm::Value::string_with_heap(s.clone(), heap)
            }
            (ValueKind::Boolean, ConstantValue::Boolean(b)) => crate::core::vm::Value::boolean(*b),
            (ValueKind::Number, _) => panic!("Constant number should have a Number value"),
            (ValueKind::String, _) => panic!("Constant string should have a String value"),
            (ValueKind::Boolean, _) => panic!("Constant boolean should have a Boolean value"),
            (ValueKind::Unknown, _) => panic!("Constant should not have Unknown kind"),
            (ValueKind::Function(_), _) => panic!("Constant should not have Function kind"),
            (ValueKind::Thunk(_), _) => panic!("Constant should not have Thunk kind"),
            (ValueKind::Void, _) => panic!("Constant should not have Void kind"),
        }
    }
    
    pub fn get_function(&self, id: u32) -> crate::core::vm::Value {
        // Functions are now separate from constants
        crate::core::vm::Value::function(id)
    }

    pub fn load_project(project_path: &Path) -> Result<MelonProject, std::io::Error> {
        let config_path = project_path.join("melon.json");
        let config_data = std::fs::read_to_string(config_path)?;
        let config: serde_json::Value = serde_json::from_str(&config_data)?;

        let main_file = config["main"].as_str().unwrap_or("main.mln");
        let entry = project_path.join("src").join(main_file);
        
        let scripts: Vec<PathBuf> = config["scripts"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|v| project_path.join(v.as_str().unwrap()))
            .collect();

        let deps: Vec<String> = config["dependencies"]
            .as_object()
            .unwrap_or(&serde_json::Map::new())
            .keys()
            .cloned()
            .collect();

        Ok(MelonProject {
            name: config["name"].as_str().unwrap().to_string(),
            entry,
            scripts,
            dependencies: deps,
        })
    }

    /// Load a module from a .mln file and register its public items.
    /// 
    /// This function:
    /// 1. Finds the mod statement to get the module name
    /// 2. Loads and parses the file
    /// 3. Extracts only pub items (functions, constants, variables)
    /// 4. Registers them as a module that can be imported
    pub fn load_module_from_file(&mut self, file_path: &Path, _project_root: &Path) -> Result<String, Box<dyn std::error::Error>> {
        use crate::core::ast::Statement;
        
        // Read and parse the file
        let content = std::fs::read_to_string(file_path)?;
        
        // Quick check: if the file doesn't start with "mod", skip it
        if !content.trim_start().starts_with("mod") {
            return Err("File does not start with 'mod' declaration".into());
        }
        
        let ast = parse_program(&content)?;
        
        // Find the mod statement to get the module name
        let mut module_name = None;
        for block in &ast.blocks {
            for stmt in &block.statements {
                if let Statement::Mod { identifier } = stmt {
                    module_name = Some(identifier.clone());
                    break;
                }
            }
            if module_name.is_some() {
                break;
            }
        }
        
        let module_name = module_name.ok_or_else(|| "Module file missing 'mod' declaration")?;
        
        // Compile the module to get HIR (this processes all statements)
        let mut module_hir_builder = HirBuilder::new();
        // Copy stdlib modules so imports work
        module_hir_builder.copy_modules_from(&self.hir_builder);
        // Copy built-in functions
        for (func_id, func) in &self.hir_builder.ast.functions {
            if *func_id >= 10000 {
                module_hir_builder.register_builtin_function(&func.name, func.signature.clone(), *func_id);
            }
        }
        
        // Parse again for HIR building
        let ast_for_hir = parse_program(&content)?;
        let hir_result = module_hir_builder.build(ast_for_hir);
        if let Err(e) = hir_result {
            return Err(format!("Failed to compile module {}: {:?}", module_name, e).into());
        }
        
        // Store the module AST for later compilation with the main program
        self.loaded_modules.insert(module_name.clone(), ast.clone());
        
        // Extract public function names and constant names from AST (we'll register with real IDs after compilation)
        let mut pub_function_names = Vec::new();
        let mut pub_constant_names = Vec::new();
        for block in &ast.blocks {
            for stmt in &block.statements {
                match stmt {
                    Statement::FunctionDeclaration { identifier, pub_visibility, .. } => {
                        if *pub_visibility {
                            pub_function_names.push(identifier.clone());
                        }
                    }
                    Statement::Const { identifier, pub_visibility, .. } => {
                        if *pub_visibility {
                            pub_constant_names.push(identifier.clone());
                        }
                    }
                    _ => {}
                }
            }
        }
        
        // Register the module with placeholder function IDs (0) and constant IDs (0)
        // We'll update these after the main file compiles and we know the real IDs
        let mut module_functions = HashMap::new();
        for func_name in pub_function_names {
            // Use a placeholder ID - we'll resolve the real ID when the function is compiled
            module_functions.insert(func_name, 0); // Placeholder
        }
        
        let mut module_constants = HashMap::new();
        for const_name in pub_constant_names {
            // Use a placeholder ID - we'll resolve the real ID when the constant is compiled
            module_constants.insert(const_name, 0); // Placeholder
        }
        
        // Register the module (even with placeholder IDs, so member access can find the module)
        if !module_functions.is_empty() || !module_constants.is_empty() {
            self.hir_builder.register_module(&module_name, module_functions, module_constants);
        }
        
        Ok(module_name)
    }

    /// Load all modules from a project directory.
    /// 
    /// Scans the src/ directory for .mln files and loads them as modules.
    pub fn load_project_modules(&mut self, project_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let src_dir = project_root.join("src");
        
        if !src_dir.exists() {
            return Ok(()); // No src directory, no modules to load
        }
        
        // Find all .mln files in src/
        let entries = std::fs::read_dir(&src_dir)?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            
            if path.extension().and_then(|s| s.to_str()) == Some("mln") {
                // Skip the main file (it's not a module)
                if path.file_name().and_then(|n| n.to_str()) == Some("main.mln") {
                    continue;
                }
                
                // Try to load this file as a module
                if let Err(e) = self.load_module_from_file(&path, project_root) {
                    eprintln!("Warning: Failed to load module from {:?}: {}", path, e);
                }
            }
        }
        
        Ok(())
    }

    /// Extract module dependencies from AST (imports)
    fn extract_module_dependencies(ast: &crate::core::ast::Program) -> Vec<String> {
        let mut dependencies = Vec::new();
        for block in &ast.blocks {
            for stmt in &block.statements {
                if let crate::core::ast::Statement::Use { path, .. } = stmt {
                    // Extract the first component of the path as the module name
                    // For example, "utils.add" -> "utils"
                    if let Some(module_name) = path.first() {
                        dependencies.push(module_name.clone());
                    }
                }
            }
        }
        dependencies
    }

    /// Build a dependency graph from loaded modules
    fn build_module_dependency_graph(&self) -> HashMap<String, Vec<String>> {
        let mut graph = HashMap::new();
        for (module_name, module_ast) in &self.loaded_modules {
            let deps = Self::extract_module_dependencies(module_ast);
            graph.insert(module_name.clone(), deps);
        }
        graph
    }

    /// Detect circular dependencies using DFS
    fn detect_circular_dependencies(
        graph: &HashMap<String, Vec<String>>,
    ) -> Result<(), String> {
        let mut visited = std::collections::HashSet::new();
        let mut rec_stack = std::collections::HashSet::new();

        fn dfs(
            node: &str,
            graph: &HashMap<String, Vec<String>>,
            visited: &mut std::collections::HashSet<String>,
            rec_stack: &mut std::collections::HashSet<String>,
            path: &mut Vec<String>,
        ) -> Option<Vec<String>> {
            visited.insert(node.to_string());
            rec_stack.insert(node.to_string());
            path.push(node.to_string());

            if let Some(deps) = graph.get(node) {
                for dep in deps {
                    if !visited.contains(dep) {
                        if let Some(cycle) = dfs(dep, graph, visited, rec_stack, path) {
                            return Some(cycle);
                        }
                    } else if rec_stack.contains(dep) {
                        // Found a cycle
                        let cycle_start = path.iter().position(|x| x == dep).unwrap();
                        let mut cycle = path[cycle_start..].to_vec();
                        cycle.push(dep.clone());
                        return Some(cycle);
                    }
                }
            }

            rec_stack.remove(node);
            path.pop();
            None
        }

        for module_name in graph.keys() {
            if !visited.contains(module_name) {
                let mut path = Vec::new();
                if let Some(cycle) = dfs(module_name, graph, &mut visited, &mut rec_stack, &mut path) {
                    return Err(format!(
                        "Circular dependency detected: {}",
                        cycle.join(" -> ")
                    ));
                }
            }
        }

        Ok(())
    }

    /// Topologically sort modules to ensure proper initialization order
    fn topological_sort_modules(
        graph: &HashMap<String, Vec<String>>,
    ) -> Result<Vec<String>, String> {
        // Detect circular dependencies first
        Self::detect_circular_dependencies(graph)?;

        let mut in_degree = HashMap::new();
        let mut dependents: HashMap<String, Vec<String>> = HashMap::new();

        // Initialize in-degree and dependents map for all modules
        for module_name in graph.keys() {
            in_degree.insert(module_name.clone(), 0);
            dependents.insert(module_name.clone(), Vec::new());
        }

        // Calculate in-degrees and build dependents map
        // If module A depends on module B, then B has A as a dependent
        for (module, deps) in graph {
            for dep in deps {
                if graph.contains_key(dep) {
                    // This module depends on 'dep', so increment in-degree of 'module'
                    *in_degree.get_mut(module).unwrap() += 1;
                    // Add 'module' to the dependents list of 'dep'
                    dependents.get_mut(dep).unwrap().push(module.clone());
                }
            }
        }

        // Kahn's algorithm for topological sort
        let mut queue = std::collections::VecDeque::new();
        for (module, &degree) in &in_degree {
            if degree == 0 {
                queue.push_back(module.clone());
            }
        }

        let mut result = Vec::new();
        while let Some(module) = queue.pop_front() {
            result.push(module.clone());

            // Decrease in-degree of modules that depend on this one
            if let Some(module_dependents) = dependents.get(&module) {
                for dependent in module_dependents {
                    let degree = in_degree.get_mut(dependent).unwrap();
                    *degree -= 1;
                    if *degree == 0 {
                        queue.push_back(dependent.clone());
                    }
                }
            }
        }

        // Check if all modules were processed (should be, since we checked for cycles)
        if result.len() != graph.len() {
            return Err("Failed to sort all modules (this should not happen after cycle detection)".to_string());
        }

        Ok(result)
    }

    /// Executes a CantaLoop program from a file.
    /// 
    /// Performs the full pipeline: parse → type check → compile → execute.
    pub fn run(&mut self, file_path: &str) {
        // Build dependency graph and sort modules topologically
        let dependency_graph = self.build_module_dependency_graph();
        let sorted_modules = match Self::topological_sort_modules(&dependency_graph) {
            Ok(order) => order,
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        };

        // Compile modules in topological order to ensure dependencies are initialized first
        for module_name in &sorted_modules {
            let module_ast = self.loaded_modules.get(module_name)
                .expect("Module should exist in loaded_modules");
            
            // Compile the module AST into the main HirBuilder
            let module_ast_clone = module_ast.clone();
            if let Err(e) = self.hir_builder.build(module_ast_clone) {
                eprintln!("Warning: Failed to compile module {}: {:?}", module_name, e);
                continue;
            }
            
            // Update module registration with real function IDs and constant IDs
            let mut module_functions = HashMap::new();
            for (func_id, func) in &self.hir_builder.ast.functions {
                // Check if this function belongs to this module by checking if it's a pub function
                // We'll identify module functions by checking the AST
                for block in &module_ast.blocks {
                    for stmt in &block.statements {
                        if let crate::core::ast::Statement::FunctionDeclaration { identifier, pub_visibility, .. } = stmt {
                            if *pub_visibility && func.name == *identifier {
                                module_functions.insert(identifier.clone(), *func_id);
                            }
                        }
                    }
                }
            }
            
            // Extract public constants and map them to their variable IDs
            // Constants declared with `const` are stored as variables in the HIR
            let mut module_constants = HashMap::new();
            for block in &module_ast.blocks {
                for stmt in &block.statements {
                    if let crate::core::ast::Statement::Const { identifier, pub_visibility, .. } = stmt {
                        if *pub_visibility {
                            // Look up the variable ID for this constant
                            // Use resolve_var_from_root to search all scopes since module variables
                            // might be in different scopes
                            if let Some(var_id) = self.hir_builder.resolve_var_from_root(identifier) {
                                module_constants.insert(identifier.clone(), var_id);
                            }
                        }
                    }
                }
            }
            
            // Update the module registration with real function IDs and constant IDs
            if !module_functions.is_empty() || !module_constants.is_empty() {
                self.hir_builder.register_module(module_name, module_functions, module_constants);
            }
        }
        
        let input = std::fs::read_to_string(file_path)
            .unwrap_or_else(|_| panic!("Failed to read {}", file_path));

        let res = parse_program(&input).expect("Failed to parse program");
        println!("AST: {:#?}", res);

        let hir_ast = match self.hir_builder.build(res) {
            Ok(ast) => ast,
            Err(e) => Self::handle_hir_error(e),
        };
        println!("HIR AST: {:#?}", hir_ast);

        // Emit function bodies first
        let function_ids: Vec<u32> = hir_ast.functions.keys().cloned().collect();
        for func_id in &function_ids {
            let func = hir_ast.functions.get(func_id).unwrap();
            // Skip built-in functions (they have empty bodies and are in the native functions map)
            if self.functions.contains_key(func_id) {
                continue;
            }

            let mut func_code = Vec::new();
            self.emitter.emit_block(&mut func_code, &func.definition.body, &hir_ast);

            // Print function bytecode for inspection
            println!("\n[Function Bytecode] {} (id={}):", func.name, func_id);
            for (op_idx, op) in func_code.iter().enumerate() {
                println!("  {:04}: {:?}", op_idx, op);
            }

            // Leak the bytecode to get a 'static reference - this is acceptable since
            // bytecode is created once and lives for the entire program lifetime
            let code_box = Box::new(func_code);
            let code_slice: &'static [OpCode] = Box::leak(code_box);

            self.bytecode_functions.insert(
                *func_id,
                BytecodeFunction {
                    code: code_slice,
                    param_var_ids: func.definition.param_var_ids.clone(),
                },
            );
        }

        let emitted = self.emitter.emit_program(hir_ast);

        println!("\n[Program (main) Bytecode]:");
        for (op_idx, op) in emitted.iter().enumerate() {
            println!("  {:04}: {:?}", op_idx, op);
        }

        println!("\nOutput:\n");

        println!("Running melon program @ {}", file_path);


        let mut vm = VM::new(self, emitted);
        vm.run();

    }
}
