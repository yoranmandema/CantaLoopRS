use std::{collections::HashMap, path::Path};

use crate::{
    ByteCodeEmitter, CompilerState, HirBuilder, OpCode, ValueKind, core::{
        ast::Program,
        engine::{BytecodeFunction, NativeFunctionDescriptor, RunArtifacts},
        hir_lowering::{HirAst, HirError},
    }, parse_program
};

pub struct BytecodeArtifacts {
    /// Bytecode for the main program
    pub main: Vec<OpCode>,

    /// Bytecode for user-defined functions
    pub functions: HashMap<u32, BytecodeFunction>,
}

pub struct CompileContext<'a> {
    pub is_native_function: &'a dyn Fn(u32) -> bool,
    pub native_functions: &'a [NativeFunctionDescriptor],
}

pub struct CompileArtifacts {
    pub ast: Program,
    pub hir: HirAst,
    pub bytecode: Option<BytecodeArtifacts>,
    pub diagnostics: Vec<HirError>,
}

/// Compile session for a single compilation.
///
/// **LIFECYCLE RULE: One CompileSession = one compile.**
///
/// - Create a new CompileSession for every compile attempt
/// - Do NOT reuse across watch rebuilds or multiple compile() calls
/// - Session is consumed after compile() returns
///
/// This prevents redeclaration bugs and ensures clean state.
pub struct CompileSession<'a> {
    context: CompileContext<'a>,
    bytecode_functions: HashMap<u32, BytecodeFunction>, // Function constant ID -> bytecode

    emitter: ByteCodeEmitter,
    hir_builder: HirBuilder,
    final_hir: Option<crate::core::hir_lowering::HirAst>, // Final HIR after take_ast() - used for runtime constant lookups
    loaded_modules: HashMap<String, crate::core::ast::Program>, // Module name -> AST for later compilation
}

impl<'a> CompileSession<'a> {
    pub fn new(ctx: CompileContext<'a>) -> Self {
        let mut hir_builder = HirBuilder::new();

        for native in ctx.native_functions {
            hir_builder.register_builtin_function(
                &native.name,
                native.signature.clone(),
                native.id,
            );
        }

        Self {
            context: ctx,
            emitter: ByteCodeEmitter::new(),
            hir_builder,
            final_hir: None,
            bytecode_functions: HashMap::new(),
            loaded_modules: HashMap::new(),
        }
    }

    pub fn hir(&self) -> Option<&crate::core::hir_lowering::HirAst> {
        Some(
            self.final_hir
                .as_ref()
                .expect("HIR requested before compilation finished"),
        )
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
        self.hir_builder
            .register_module(path, functions, HashMap::new(), HashMap::new());
    }

    /// Register a module with functions and structs.
    pub fn register_module_with_structs(
        &mut self,
        path: &str,
        functions: HashMap<String, u32>,
        structs: HashMap<String, crate::core::hir_lowering::StructDef>,
    ) {
        self.hir_builder
            .register_module(path, functions, HashMap::new(), structs);
    }

    /// Check if a module is already registered in the HirBuilder.
    pub fn has_module(&self, module_name: &str) -> bool {
        self.hir_builder.modules.contains_key(module_name)
    }

    /// Get the function ID for a registered function by name.
    ///
    /// Returns None if the function is not found.
    pub fn get_function_id_by_name(&self, name: &str) -> Option<u32> {
        self.hir_builder.resolve_function(&self.context, name)
    }

    /// Get the function ID for a native function by name from the context.
    ///
    /// This looks up the function directly from native_functions, which is useful
    /// for stdlib module registration where functions are registered with both
    /// qualified (e.g., "matrix.add") and unqualified (e.g., "add") names.
    ///
    /// Returns None if the function is not found.
    pub fn get_native_function_id_by_name(&self, name: &str) -> Option<u32> {
        self.context.native_functions
            .iter()
            .find(|native| native.name == name)
            .map(|native| native.id)
    }

    /// Formats a ValueKind for error messages, handling function/thunk types specially.
    fn format_value_kind_for_error(kind: &ValueKind) -> String {
        match kind {
            ValueKind::Any => "Any".to_string(),
            ValueKind::Number => "Number".to_string(),
            ValueKind::String => "String".to_string(),
            ValueKind::Boolean => "Boolean".to_string(),
            ValueKind::Unknown => "Unknown".to_string(),
            ValueKind::Function(ty) => ty.clone(),
            ValueKind::Thunk(ty) => ty.clone(),
            ValueKind::Callable => "Callable".to_string(),
            ValueKind::Void => "Void".to_string(),
            ValueKind::Struct(name) => name.clone(),
            ValueKind::Array(inner) => {
                let inner_str = Self::format_value_kind_for_error(inner);
                format!("Array<{}>", inner_str)
            }
        }
    }

    /// Handles HIR errors by converting them to panic messages.
    fn handle_hir_error(error: HirError) -> ! {
        match error {
            HirError::TypeError(msg) => {
                panic!("Type error: {}", msg);
            }
            HirError::TypeMismatch {
                variable,
                expected,
                actual,
            } => {
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
            HirError::BinaryOpTypeError {
                operator,
                lhs_type,
                rhs_type,
                expected,
            } => {
                let lhs_str = Self::format_value_kind_for_error(&lhs_type);
                let rhs_str = Self::format_value_kind_for_error(&rhs_type);
                panic!(
                    "Binary operation type error: Operator '{}' expects {}, but got {} and {}",
                    operator, expected, lhs_str, rhs_str
                );
            }
        }
    }

    /// Get constant value from HIR (helper function for VM)
    pub fn get_constant_from_hir(
        hir: &HirAst,
        id: u32,
        heap: &mut crate::core::vm::ValueHeap,
    ) -> crate::core::vm::Value {
        use crate::core::hir_lowering::ConstantValue;

        // This is called from VM at runtime - if constant is missing, it's a serious bug
        // (invalid bytecode), so panic is acceptable here
        let c = hir
            .constants
            .iter()
            .find(|c| c.id == id)
            .expect(&format!("Constant {} not found in HIR - this indicates invalid bytecode", id));

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
            (ValueKind::Any, _) => panic!("Constant should not have Any kind"),
            (ValueKind::Unknown, _) => panic!("Constant should not have Unknown kind"),
            (ValueKind::Function(_), _) => panic!("Constant should not have Function kind"),
            (ValueKind::Thunk(_), _) => panic!("Constant should not have Thunk kind"),
            (ValueKind::Callable, _) => panic!("Constant should not have Callable kind"),
            (ValueKind::Void, _) => panic!("Constant should not have Void kind"),
            (ValueKind::Struct(_), _) => panic!("Constant should not have Struct kind"),
            (ValueKind::Array(_), _) => panic!("Constant should not have Array kind"),
        }
    }

    /// Load a module from a .mln file and register its public items.
    ///
    /// This function:
    /// 1. Finds the mod statement to get the module name
    /// 2. Loads and parses the file
    /// 3. Extracts only pub items (functions, constants, variables)
    /// 4. Registers them as a module that can be imported
    pub fn load_module_from_file(
        &mut self,
        file_path: &Path,
        _project_root: &Path,
    ) -> Result<String, Box<dyn std::error::Error>> {
        use crate::core::ast::Statement;

        let content = std::fs::read_to_string(file_path)?;
        let ast = parse_program(&content)?;

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

        let module_name = module_name.ok_or("Missing mod declaration")?;

        // Store AST only
        self.loaded_modules.insert(module_name.clone(), ast);

        // Register module shell – real IDs are filled during unified build
        self.hir_builder
            .register_module(&module_name, HashMap::new(), HashMap::new(), HashMap::new());

        Ok(module_name)
    }

    /// Load all modules from a project directory.
    ///
    /// Scans the src/ directory for .mln files and loads them as modules.
    pub fn load_project_modules(
        &mut self,
        project_root: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
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

    

    /// Load project modules for LSP compilation (non-mutating version).
    /// This loads modules into a HirBuilder without mutating the engine.
    ///
    /// If `current_file` is provided, that file will be skipped to avoid duplicate declarations.
    fn load_project_modules_for_lsp(
        hir_builder: &mut HirBuilder,
        source_hir_builder: &HirBuilder,
        project_root: &Path,
        current_file: Option<&Path>,
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

                // Skip the current file being compiled to avoid duplicate declarations
                if let Some(current) = current_file {
                    if path == current {
                        continue;
                    }
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
    /// Just parses AST and registers an empty module shell.
    /// No compilation - pub items are registered during build_append().
    fn load_module_for_lsp(
        hir_builder: &mut HirBuilder,
        _source_hir_builder: &HirBuilder,
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

        // Register empty module shell - pub items will be registered during build_append()
        hir_builder.register_module(&module_name, HashMap::new(), HashMap::new(), HashMap::new());

        Ok(module_name)
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
    fn detect_circular_dependencies(graph: &HashMap<String, Vec<String>>) -> Result<(), String> {
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
                if let Some(cycle) =
                    dfs(module_name, graph, &mut visited, &mut rec_stack, &mut path)
                {
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
            return Err(
                "Failed to sort all modules (this should not happen after cycle detection)"
                    .to_string(),
            );
        }

        Ok(result)
    }

    /// Compile a file into runnable artifacts.
    ///
    /// **IMPORTANT: This method should only be called once per CompileSession.**
    /// Create a new CompileSession for each compile attempt to avoid redeclaration bugs.
    ///
    /// Each AST is lowered exactly once - no re-parsing or re-lowering.
    pub fn compile(&mut self, file_path: &str) -> Result<RunArtifacts, Box<dyn std::error::Error>> {
        // Debug assertion: ensure we're starting from a clean state
        // This will catch reuse bugs immediately
        debug_assert!(
            !self.hir_builder.has_active_scope(),
            "HIR builder still has active scopes at compile start - possible reuse bug"
        );

        // Reset derived data (HIR output, but NOT registered modules or builtin functions)
        self.hir_builder.reset_hir_only();
        self.final_hir = None;
        self.bytecode_functions.clear();

        // Build dependency graph and sort modules topologically
        let dependency_graph = self.build_module_dependency_graph();

        let sorted_modules = Self::topological_sort_modules(&dependency_graph)
            .map_err(|e| format!("Module dependency error: {}", e))?;

        // Compile modules in topological order to ensure dependencies are initialized first
        // Compile all modules into ONE HIR
        for module in &sorted_modules {
            let ast = self.loaded_modules.get(module)
                .ok_or_else(|| format!("Module '{}' not found in loaded_modules", module))?
                .clone();
            self.hir_builder.set_current_module(Some(module.clone()));
            self.hir_builder.build_append(&self.context, ast)
                .map_err(|e| format!("Failed to compile module '{}': {:?}", module, e))?;
        }

        let input = std::fs::read_to_string(file_path)
            .map_err(|e| format!("Failed to read file '{}': {}", file_path, e))?;

        let main_module = "__main__".to_string();

        self.hir_builder
            .register_module(&main_module, HashMap::new(), HashMap::new(), HashMap::new());
        self.hir_builder.set_current_module(Some(main_module));

        let ast = parse_program(&input)
            .map_err(|e| format!("Parse error in '{}': {}", file_path, e))?;

        self.hir_builder.build_append(&self.context, ast.clone())
            .map_err(|e| format!("Failed to compile main file '{}': {:?}", file_path, e))?;

        let hir_ast = self.hir_builder.take_ast();

        // Store the final HIR for runtime constant lookups
        // After take_ast(), hir_builder.ast is empty, so we must use final_hir
        self.final_hir = Some(hir_ast.clone());

        // Emit function bodies first
        let function_ids: Vec<u32> = hir_ast.functions.keys().cloned().collect();
        let mut functions: HashMap<u32, Vec<OpCode>> = HashMap::new();

        for func_id in &function_ids {
            let func = hir_ast.functions.get(func_id)
                .ok_or_else(|| format!("Function {} not found in HIR", func_id))?;
            // Skip built-in functions (they have empty bodies and are in the native functions map)
            if (self.context.is_native_function)(*func_id) {
                continue;
            }

            let mut func_code = Vec::new();
            self.emitter
                .emit_block(&mut func_code, &func.definition.body, &hir_ast);

            functions.insert(*func_id, func_code.clone());

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

        let main = self.emitter.emit_program(&hir_ast);

        Ok(RunArtifacts {
            ast,
            hir: hir_ast,
            functions,
            main,
            bytecode_functions: self.bytecode_functions.clone(),
        })
    }
}
