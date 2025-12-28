//! Statement lowering: AST Statement → HIR Statement
//! 
//! This module handles lowering AST statements to HIR statements and blocks,
//! including the main HirBuilder implementation.

use std::collections::HashMap;

use crate::core::ast::{
    Argument, BinaryOp, Block, CallArgument, Expression, Literal, PostfixOp, Program, Statement, UnaryOp,
};

use super::{
    HirAst, HirExpression, ValueKind, Variable, Constant, ConstantValue,
    Function, FunctionDefinition, FunctionSignature, HirError, ImportTable, Module,
    scopes::{ScopeId, ScopeArena, HirBlockContext},
};

// Hashable key for constant deduplication
#[derive(Hash, PartialEq, Eq, Clone)]
enum ConstantKey {
    Number(u64), // f64 bit representation
    String(String),
    Boolean(bool),
}

impl ConstantKey {
    fn from_constant_value(value: &ConstantValue) -> Self {
        match value {
            ConstantValue::Number(n) => ConstantKey::Number(n.to_bits()),
            ConstantValue::String(s) => ConstantKey::String(s.clone()),
            ConstantValue::Boolean(b) => ConstantKey::Boolean(*b),
            ConstantValue::None => panic!("Cannot create key from None constant"),
        }
    }
}

/// High-level Intermediate Representation of a statement.
#[derive(Debug, Clone)]
pub enum HirStmt {
    Assign {
        slot: u32, // VarId
        value: HirExpression,
    },
    AssignIncrement {
        slot: u32, // VarId
        value: HirExpression,
    },
    AssignDecrement {
        slot: u32, // VarId
        value: HirExpression,
    },
    If {
        arms: Vec<(HirExpression, HirBlock)>,
        else_block: Box<HirBlock>,
    },
    Match {
        expression: HirExpression,
        cases: Vec<(Option<HirExpression>, HirBlock)>, // (pattern expression, block) - None for wildcard
    },
    Return {
        value: HirExpression,
    },
    Loop {
        init_vars: Vec<(u32, HirExpression)>, // (variable_id, initial_value) for loop initialization variables
        body: HirBlock,
        break_slot: Option<u32>, // Variable slot for break value (None for statement loops, Some(slot) for expression loops)
    },
    Break {
        value: Option<HirExpression>,
    },
    Continue,
    Expression(HirExpression),
    Nop, // No-op statement (e.g., for use statements that are compile-time only)
}

/// High-level Intermediate Representation of a block.
#[derive(Debug, Clone)]
pub struct HirBlock {
    #[allow(dead_code)]
    pub scope: ScopeId,
    pub statements: Vec<HirStmt>,
}

/// Builder for converting AST to HIR.
/// 
/// This is the main entry point for the lowering pass.
pub struct HirBuilder {
    pub ast: HirAst,
    current_scope: ScopeId,
    next_var_id: u32,
    next_function_id: u32,
    constant_map: HashMap<ConstantKey, u32>, // Maps constant value to constant ID
    /// Maps module paths (e.g., "math.utils") to their modules
    modules: HashMap<String, Module>,
    /// Maps imported symbol names to function IDs (compile-time only)
    import_table: ImportTable,
}

impl HirBuilder {
    pub fn new() -> Self {
        let mut scopes = ScopeArena { scopes: Vec::new() };

        let root = ScopeId(0);
        scopes.scopes.push(HirBlockContext {
            vars: Vec::new(),
            parent: None,
        });

        Self {
            ast: HirAst {
                constants: Vec::new(),
                blocks: Vec::new(),
                scopes,
                functions: std::collections::HashMap::new(),
                import_table: HashMap::new(),
            },
            current_scope: root,
            next_var_id: 0,
            next_function_id: 0,
            constant_map: HashMap::new(),
            modules: HashMap::new(),
            import_table: HashMap::new(),
        }
    }

    pub fn resolve_var(&self, name: &str) -> Option<u32> {
        let mut scope = Some(self.current_scope);

        while let Some(id) = scope {
            let ctx = &self.ast.scopes.scopes[id.as_usize()];
            if let Some(v) = ctx.vars.iter().find(|v| v.name == name) {
                return Some(v.id);
            }
            scope = ctx.parent;
        }

        None
    }

    /// Resolve a variable by searching from the root scope (for LSP queries)
    pub fn resolve_var_from_root(&self, name: &str) -> Option<u32> {
        // Search all scopes starting from root (scope 0)
        for scope_idx in 0..self.ast.scopes.scopes.len() {
            let ctx = &self.ast.scopes.scopes[scope_idx];
            if let Some(v) = ctx.vars.iter().find(|v| v.name == name) {
                return Some(v.id);
            }
        }
        None
    }

    /// Resolve a variable by searching from current scope, then all scopes
    /// This is more aggressive than resolve_var and ensures we find function parameters
    pub fn resolve_var_aggressive(&self, name: &str) -> Option<u32> {
        // First try current scope chain (fast path)
        if let Some(var_id) = self.resolve_var(name) {
            return Some(var_id);
        }
        // Then try all scopes (slower but more thorough)
        self.resolve_var_from_root(name)
    }

    pub fn get_var_kind(&self, var_id: u32) -> Option<ValueKind> {
        let mut scope = Some(self.current_scope);
        while let Some(scope_id) = scope {
            let ctx = &self.ast.scopes.scopes[scope_id.as_usize()];
            if let Some(v) = ctx.vars.iter().find(|v| v.id == var_id) {
                return Some(v.kind.clone());
            }
            scope = ctx.parent;
        }
        None
    }

    /// Get variable kind by searching all scopes (for LSP queries)
    pub fn get_var_kind_from_id(&self, var_id: u32) -> Option<ValueKind> {
        // Search all scopes
        for scope_idx in 0..self.ast.scopes.scopes.len() {
            let ctx = &self.ast.scopes.scopes[scope_idx];
            if let Some(v) = ctx.vars.iter().find(|v| v.id == var_id) {
                return Some(v.kind.clone());
            }
        }
        None
    }

    fn format_value_kind_for_type(kind: &ValueKind) -> String {
        match kind {
            ValueKind::Number => "num".to_string(),
            ValueKind::String => "string".to_string(),
            ValueKind::Boolean => "bool".to_string(),
            ValueKind::Unknown => "unknown".to_string(),
            ValueKind::Function(ty) => ty.clone(),
            ValueKind::Thunk(ty) => ty.clone(),
            ValueKind::Void => "void".to_string(),
        }
    }

    /// Extract input and output types from a function or thunk type string
    /// Returns (input_type, output_type) where input_type can be a single type or a tuple
    /// Examples:
    ///   "num -> num" -> ("num", "num")
    ///   "(num, string) -> bool" -> ("(num,string)", "bool")
    ///   "num ~> num" -> ("num", "num")
    fn parse_function_type_string(type_str: &str) -> Option<(String, String)> {
        let trimmed = type_str.trim();

        // Find the arrow (-> or ~>)
        let arrow_pos = trimmed.find("->").or_else(|| trimmed.find("~>"))?;

        let input_part = trimmed[..arrow_pos].trim();
        let output_part = trimmed[arrow_pos + 2..].trim();

        Some((input_part.to_string(), output_part.to_string()))
    }

    /// Get the input type from a ValueKind that represents a function or thunk
    /// Returns None if the kind is not a function or thunk
    fn get_function_input_type(kind: &ValueKind) -> Option<String> {
        match kind {
            ValueKind::Function(ty) | ValueKind::Thunk(ty) => {
                Self::parse_function_type_string(ty).map(|(input, _)| input)
            }
            _ => None,
        }
    }

    /// Get the output type from a ValueKind that represents a function or thunk
    /// Returns None if the kind is not a function or thunk
    fn get_function_output_type(kind: &ValueKind) -> Option<String> {
        match kind {
            ValueKind::Function(ty) | ValueKind::Thunk(ty) => {
                Self::parse_function_type_string(ty).map(|(_, output)| output)
            }
            _ => None,
        }
    }

    fn format_value_kind(kind: &ValueKind) -> String {
        match kind {
            ValueKind::Number => "Number".to_string(),
            ValueKind::String => "String".to_string(),
            ValueKind::Boolean => "Boolean".to_string(),
            ValueKind::Unknown => "Unknown".to_string(),
            ValueKind::Function(ty) => ty.clone(),
            ValueKind::Thunk(ty) => ty.clone(),
            ValueKind::Void => "void".to_string(),
        }
    }

    fn check_type_compatibility(expected: &ValueKind, actual: &ValueKind) -> bool {
        // Types must match exactly, except Unknown (any) accepts any type
        match (expected, actual) {
            (ValueKind::Number, ValueKind::Number) => true,
            (ValueKind::String, ValueKind::String) => true,
            (ValueKind::Boolean, ValueKind::Boolean) => true,
            (ValueKind::Function(expected_ty), ValueKind::Function(actual_ty)) => {
                // Use structural comparison for function types
                Self::check_callable_type_compatibility(expected_ty, actual_ty)
            }
            (ValueKind::Thunk(expected_ty), ValueKind::Thunk(actual_ty)) => {
                // Use structural comparison for thunk types
                Self::check_callable_type_compatibility(expected_ty, actual_ty)
            }
            (ValueKind::Unknown, _) => true, // Unknown (any) accepts any type
            _ => false,
        }
    }

    /// Check if a variable exists only in the current scope (not parent scopes)
    /// Used to allow shadowing: variables can be redeclared in nested scopes
    fn var_exists_in_current_scope(&self, name: &str) -> bool {
        let ctx = &self.ast.scopes.scopes[self.current_scope.as_usize()];
        ctx.vars.iter().any(|v| v.name == name)
    }

    pub fn init_var(&mut self, name: &str, kind: ValueKind) -> u32 {
        let id = self.next_var_id;
        self.next_var_id += 1;

        let ctx = &mut self.ast.scopes.scopes[self.current_scope.as_usize()];
        ctx.vars.push(Variable {
            id,
            name: name.to_string(),
            kind,
        });

        id
    }

    /// Extract return type from a callable type string (function or thunk)
    fn extract_return_type_from_callable(type_str: &str) -> ValueKind {
        if let Some((_, return_type, _)) = Self::parse_callable_type(type_str) {
            let trimmed_return = return_type.trim();
            let parsed = Self::parse_type_string_static(trimmed_return);

            // Fallback: if parsing failed, try to extract the type directly
            // For "X ~> X" format, we can extract X directly
            if matches!(parsed, ValueKind::Unknown) {
                if let Some(pos) = type_str.find("~>") {
                    let before = type_str[..pos].trim();
                    let after = type_str[pos + 2..].trim();
                    if before == after {
                        // It's "X ~> X" format, use X
                        return Self::parse_type_string_static(before);
                    }
                }
            }
            parsed
        } else if let Some(pos) = type_str.find("~>") {
            // Fallback: extract type directly from "X ~> Y" format
            let after_arrow = type_str[pos + 2..].trim();
            Self::parse_type_string_static(after_arrow)
        } else {
            ValueKind::Unknown
        }
    }

    /// Infer type for a PostfixInvoke expression
    fn infer_postfix_invoke_kind(&self, operand: &HirExpression) -> ValueKind {
        match operand {
            HirExpression::FunctionCall { function_id, .. } => {
                // Direct function call thunk
                if let Some(func) = self.ast.functions.get(function_id) {
                    *func.signature.return_type.clone()
                } else {
                    ValueKind::Unknown
                }
            }
            HirExpression::Identifier(var_id) => {
                // Variable containing a thunk - check if it's a thunk type or a function type
                let var_kind = self.get_var_kind(*var_id).unwrap_or(ValueKind::Unknown);
                match &var_kind {
                    ValueKind::Thunk(type_str) | ValueKind::Function(type_str) => {
                        Self::extract_return_type_from_callable(type_str)
                    }
                    _ => var_kind,
                }
            }
            HirExpression::PostfixInvoke { .. } => {
                // Nested invocation: first infer the inner invocation's type
                let inner_type = self.infer_variable_kind(operand);
                match &inner_type {
                    ValueKind::Thunk(type_str) | ValueKind::Function(type_str) => {
                        Self::extract_return_type_from_callable(type_str)
                    }
                    _ => inner_type,
                }
            }
            _ => {
                // Fallback: try to infer from operand
                let operand_type = self.infer_variable_kind(operand);
                match &operand_type {
                    ValueKind::Thunk(type_str) | ValueKind::Function(type_str) => {
                        Self::extract_return_type_from_callable(type_str)
                    }
                    _ => operand_type,
                }
            }
        }
    }

    /// Infer type for a binary expression
    fn infer_binary_kind(
        &self,
        operator: &BinaryOp,
        lhs: &HirExpression,
        rhs: &HirExpression,
    ) -> ValueKind {
        match operator {
            BinaryOp::And | BinaryOp::Or => ValueKind::Boolean,
            BinaryOp::Eq
            | BinaryOp::Ne
            | BinaryOp::Gt
            | BinaryOp::Lt
            | BinaryOp::Ge
            | BinaryOp::Le => ValueKind::Boolean,
            BinaryOp::Add => {
                // Addition returns String if either operand is String, otherwise Number
                let lhs_type = self.infer_variable_kind(lhs);
                let rhs_type = self.infer_variable_kind(rhs);
                if matches!(lhs_type, ValueKind::String) || matches!(rhs_type, ValueKind::String) {
                    ValueKind::String
                } else {
                    ValueKind::Number
                }
            }
            _ => ValueKind::Number, // Other binary ops produce numbers
        }
    }

    /// Infer type for a FunctionCall expression
    fn infer_function_call_kind(&self, function_id: u32, invoke: bool) -> ValueKind {
        if let Some(func) = self.ast.functions.get(&function_id) {
            if !invoke {
                // This is a thunk (prepared call) - return a Thunk type
                let return_type_str = Self::format_value_kind_for_type(&func.signature.return_type);
                let thunk_type = format!("{} ~> {}", return_type_str, return_type_str);
                ValueKind::Thunk(thunk_type)
            } else {
                // Invoked immediately - return the function's return type
                *func.signature.return_type.clone()
            }
        } else {
            ValueKind::Unknown
        }
    }

    fn infer_compose_thunk_kind(&self, first: &HirExpression, second: &HirExpression) -> ValueKind {
        // Composition f |> g means g(f(x))
        // Input type = input type of f
        // Output type = output type of g

        // Infer types of both expressions
        let first_kind = self.infer_variable_kind(first);
        let second_kind = self.infer_variable_kind(second);

        // Try to extract input/output types
        let first_input = Self::get_function_input_type(&first_kind);
        let second_output = Self::get_function_output_type(&second_kind);

        match (first_input, second_output) {
            (Some(f_in), Some(g_out)) => {
                // Both are functions/thunks - compose them
                // f |> g means g(f(x)), so:
                // - Input type = input type of f
                // - Output type = output type of g
                let thunk_type = format!("{} ~> {}", f_in, g_out);
                ValueKind::Thunk(thunk_type)
            }
            _ => {
                // If we can't infer, return Unknown
                // This could happen if one of the expressions isn't a function/thunk
                ValueKind::Unknown
            }
        }
    }


    fn infer_partial_call_kind(&self, func_id: &u32, bound: &Vec<Option<HirExpression>>) -> ValueKind {
        if let Some(func) = self.ast.functions.get(func_id) {
            let return_type_str =
                Self::format_value_kind_for_type(&func.signature.return_type);

            // Find which parameters are holes (unbound)
            let mut hole_types = Vec::new();
            for (index, arg) in bound.iter().enumerate() {
                if arg.is_none() {
                    // This is a hole - get the type from the function signature
                    if let Some(param_type) = func.signature.params.get(index) {
                        hole_types.push(Self::format_value_kind_for_type(param_type));
                    } else {
                        // If we have more holes than parameters, use Unknown
                        hole_types.push("unknown".to_string());
                    }
                }
            }

            // Format the parameter types
            let param_types = if hole_types.is_empty() {
                // No holes - this shouldn't happen, but handle it
                "unknown".to_string()
            } else if hole_types.len() == 1 {
                hole_types[0].clone()
            } else {
                format!("({})", hole_types.join(","))
            };

            let thunk_type = format!("{} ~> {}", param_types, return_type_str);
            ValueKind::Thunk(thunk_type)
        } else {
            ValueKind::Unknown
        }
    }

    fn infer_variable_kind(&self, expr: &HirExpression) -> ValueKind {
        match expr {
            HirExpression::Number(_) => ValueKind::Number,
            HirExpression::String(_) => ValueKind::String,
            HirExpression::Constant(id) => self
                .ast
                .constants
                .iter()
                .find(|c| c.id == *id)
                .map(|c| c.kind.clone())
                .unwrap_or(ValueKind::Unknown),
            HirExpression::Identifier(id) => self
                .ast
                .scopes
                .scopes
                .iter()
                .find_map(|scope| scope.vars.iter().find(|v| v.id == *id))
                .map(|v| v.kind.clone())
                .unwrap_or(ValueKind::Unknown),
            HirExpression::Binary { operator, lhs, rhs } => {
                self.infer_binary_kind(operator, lhs, rhs)
            }
            HirExpression::Unary { .. } => ValueKind::Number,
            HirExpression::FunctionCall {
                function_id,
                invoke,
                ..
            } => self.infer_function_call_kind(*function_id, *invoke),
            HirExpression::PostfixInvoke { operand, .. } => self.infer_postfix_invoke_kind(operand),
            HirExpression::ComposeThunk { first, second } => {
                self.infer_compose_thunk_kind(first, second)
            }
            HirExpression::PartialCall { func_id, bound } => {
               self.infer_partial_call_kind(func_id, bound)
            }
            HirExpression::Loop { break_slot, .. } => {
                // Loop expression returns the type of the break value
                // For now, we can't infer it statically
                if break_slot.is_some() {
                    ValueKind::Unknown
                } else {
                    ValueKind::Unknown
                }
            }
        }
    }


    fn resolve_const(&self, name: &str) -> Option<u32> {
        // Only resolve data constants (not functions)
        self.ast
            .constants
            .iter()
            .rev()
            .find(|c| c.name == name)
            .map(|c| c.id)
    }

    pub fn resolve_function(&self, name: &str) -> Option<u32> {
        // Look up function by name in the functions registry
        self.ast
            .functions
            .iter()
            .find(|(_, func)| func.name == name)
            .map(|(&id, _)| id)
    }

    pub fn register_builtin_function(&mut self, name: &str, signature: FunctionSignature, id: u32) {
        // Register a built-in function (from Engine) in the HIR function registry
        // Create a dummy function definition since built-ins are handled in the VM
        let dummy_def = FunctionDefinition {
            body: HirBlock {
                scope: ScopeId(0),
                statements: vec![],
            },
            param_var_ids: vec![],
            scope_id: ScopeId(0),
        };

        let function = Function {
            id,
            name: name.to_string(),
            signature,
            definition: dummy_def,
        };

        self.ast.functions.insert(id, function);
    }

    /// Register a module that can be imported.
    /// 
    /// # Arguments
    /// * `path` - Dot-separated module path (e.g., "math.utils")
    /// * `functions` - Map of function names to their function IDs
    /// * `constants` - Map of constant names to their constant IDs
    pub fn register_module(&mut self, path: &str, functions: HashMap<String, u32>, constants: HashMap<String, u32>) {
        self.modules.insert(path.to_string(), Module { functions, constants });
    }

    /// Copy all modules from another HirBuilder.
    /// Used when creating a fresh HirBuilder for LSP compilation.
    pub fn copy_modules_from(&mut self, other: &HirBuilder) {
        for (path, module) in &other.modules {
            self.modules.insert(path.clone(), Module {
                functions: module.functions.clone(),
                constants: module.constants.clone(),
            });
        }
    }

    /// Resolve an import path and selector to function IDs.
    /// Returns a map of imported symbol names to function IDs.
    /// Note: Constants are also imported as "function IDs" (actually variable IDs) for compatibility.
    fn resolve_import(&self, path: &[String], selector: &crate::core::ast::ImportSelector) -> Result<ImportTable, HirError> {
        let module_path = path.join(".");
        
        let module = self.modules.get(&module_path)
            .ok_or_else(|| HirError::TypeError(format!("Module '{}' not found", module_path)))?;

        let mut imports = ImportTable::new();

        match selector {
            crate::core::ast::ImportSelector::Single(name) => {
                // Check functions first, then constants
                if let Some(func_id) = module.functions.get(name) {
                    imports.insert(name.clone(), *func_id);
                } else if let Some(const_id) = module.constants.get(name) {
                    // Constants are stored as variable IDs, import them as such
                    imports.insert(name.clone(), *const_id);
                } else {
                    return Err(HirError::TypeError(format!("Function or constant '{}' not found in module '{}'", name, module_path)));
                }
            }
            crate::core::ast::ImportSelector::Multiple(names) => {
                for name in names {
                    // Check functions first, then constants
                    if let Some(func_id) = module.functions.get(name) {
                        imports.insert(name.clone(), *func_id);
                    } else if let Some(const_id) = module.constants.get(name) {
                        // Constants are stored as variable IDs, import them as such
                        imports.insert(name.clone(), *const_id);
                    } else {
                        return Err(HirError::TypeError(format!("Function or constant '{}' not found in module '{}'", name, module_path)));
                    }
                }
            }
            crate::core::ast::ImportSelector::Wildcard => {
                // Import all functions and constants from the module
                for (name, func_id) in &module.functions {
                    imports.insert(name.clone(), *func_id);
                }
                for (name, const_id) in &module.constants {
                    imports.insert(name.clone(), *const_id);
                }
            }
        }

        Ok(imports)
    }

    /// Parse a type string into a structured ValueKind
    /// Supports: simple types (num, str, bool), function types (num -> num), and thunk types (num ~> num)
    fn parse_type_string(&self, type_str: &str) -> ValueKind {
        let trimmed = type_str.trim();

        // Check for thunk type (contains "~>") - check this first since it's more specific
        if trimmed.contains("~>") {
            // Normalize the type string (remove extra whitespace, normalize parentheses)
            let normalized = Self::normalize_type_string(trimmed);
            return ValueKind::Thunk(normalized);
        }

        // Check for function type (contains "->")
        if trimmed.contains("->") {
            // Normalize the type string (remove extra whitespace, normalize parentheses)
            let normalized = Self::normalize_type_string(trimmed);
            return ValueKind::Function(normalized);
        }

        // Simple types
        match trimmed.to_lowercase().as_str() {
            "number" | "num" | "int" | "float" => ValueKind::Number,
            "string" | "str" => ValueKind::String,
            "boolean" | "bool" => ValueKind::Boolean,
            "void" => ValueKind::Void,
            "any" | "" => ValueKind::Unknown,
            _ => ValueKind::Unknown,
        }
    }

    /// Normalize a type string by removing extra whitespace and normalizing parentheses
    /// Examples:
    ///   "num -> num" -> "num -> num"
    ///   "(num, num) -> num" -> "(num,num) -> num"
    ///   "num ~> num" -> "num ~> num"
    fn normalize_type_string(type_str: &str) -> String {
        let trimmed = type_str.trim();
        let mut result = String::new();
        let mut in_parens = false;
        let mut paren_depth = 0;

        for ch in trimmed.chars() {
            match ch {
                '(' => {
                    in_parens = true;
                    paren_depth += 1;
                    result.push(ch);
                }
                ')' => {
                    paren_depth -= 1;
                    if paren_depth == 0 {
                        in_parens = false;
                    }
                    result.push(ch);
                }
                ',' => {
                    result.push(ch);
                    // Add space after comma only if not in nested parentheses
                    if !in_parens || paren_depth == 1 {
                        // Don't add space - keep compact
                    }
                }
                c if c.is_whitespace() => {
                    // Only preserve whitespace around -> and ~>
                    // Skip other whitespace
                    if result.ends_with("->")
                        || result.ends_with("~>")
                        || result.ends_with("(")
                        || result.ends_with(",")
                    {
                        // Don't add space yet
                    } else if result.ends_with(" ") {
                        // Already have space
                    } else {
                        // Check if next non-whitespace is -> or ~>
                        let remaining = trimmed[result.len()..].trim_start();
                        if remaining.starts_with("->") || remaining.starts_with("~>") {
                            result.push(' ');
                        }
                    }
                }
                _ => {
                    result.push(ch);
                }
            }
        }

        // Clean up: remove spaces before -> and ~>, ensure single space after
        result = result.replace(" ->", "->").replace(" ~>", "~>");
        result = result.replace("->", " -> ").replace("~>", " ~> ");
        // Remove multiple spaces
        while result.contains("  ") {
            result = result.replace("  ", " ");
        }
        result.trim().to_string()
    }

    /// Check if a thunk will be fully applied after adding the given number of arguments
    /// Returns (is_fully_applied, total_args_needed, args_provided_so_far)
    #[allow(dead_code)]
    pub fn check_thunk_completeness(
        &self,
        var_id: u32,
        additional_args: usize,
    ) -> Option<(bool, usize, usize)> {
        let var_kind = self.get_var_kind_from_id(var_id)?;
        if let ValueKind::Thunk(thunk_type_str) = var_kind {
            // Parse the thunk type to see how many args it still needs
            if let Some((param_types, _, _)) = Self::parse_callable_type(&thunk_type_str) {
                // The param_types in a thunk type represent the remaining args needed
                let args_still_needed = param_types.len();
                let args_provided_so_far = 0; // We don't track this in the type string currently
                let total_args_needed = args_still_needed; // For now, assume thunk type shows remaining args
                let will_be_fully_applied = additional_args >= args_still_needed;
                return Some((
                    will_be_fully_applied,
                    total_args_needed,
                    args_provided_so_far,
                ));
            }
        }
        None
    }

    /// Parse a function or thunk type string into its components
    /// Returns (param_types, return_type, is_thunk)
    /// Examples:
    ///   "num -> num" -> (vec!["num"], "num", false)
    ///   "(num, num) -> num" -> (vec!["num", "num"], "num", false)
    ///   "num ~> num" -> (vec!["num"], "num", true)
    fn parse_callable_type(type_str: &str) -> Option<(Vec<String>, String, bool)> {
        let trimmed = type_str.trim();
        let is_thunk = trimmed.contains("~>");
        let arrow = if is_thunk { "~>" } else { "->" };

        if let Some(arrow_pos) = trimmed.find(arrow) {
            let params_str = trimmed[..arrow_pos].trim();
            let return_str = trimmed[arrow_pos + arrow.len()..].trim();

            // Parse parameter types
            let param_types = if params_str.starts_with('(') && params_str.ends_with(')') {
                // Multiple parameters: (type1, type2, ...)
                let inner = &params_str[1..params_str.len() - 1].trim();
                if inner.is_empty() {
                    Vec::new()
                } else {
                    inner.split(',').map(|s| s.trim().to_string()).collect()
                }
            } else {
                // Single parameter: type
                vec![params_str.to_string()]
            };

            Some((param_types, return_str.to_string(), is_thunk))
        } else {
            None
        }
    }

    /// Check if binary operation types are valid
    fn check_binary_op_types(
        &self,
        op: &BinaryOp,
        lhs_type: &ValueKind,
        rhs_type: &ValueKind,
    ) -> Result<(), HirError> {
        match op {
            BinaryOp::Add => {
                let is_valid = matches!(
                    lhs_type,
                    ValueKind::Number | ValueKind::String | ValueKind::Unknown
                ) && matches!(
                    rhs_type,
                    ValueKind::Number | ValueKind::String | ValueKind::Unknown
                );

                if !is_valid {
                    return Err(HirError::BinaryOpTypeError {
                        operator: "+".to_string(),
                        lhs_type: lhs_type.clone(),
                        rhs_type: rhs_type.clone(),
                        expected: "Number or String (supports string + number concatenation)"
                            .to_string(),
                    });
                }
            }
            BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod | BinaryOp::Pow => {
                if !matches!(lhs_type, ValueKind::Number | ValueKind::Unknown)
                    || !matches!(rhs_type, ValueKind::Number | ValueKind::Unknown)
                {
                    let op_str = match op {
                        BinaryOp::Sub => "-",
                        BinaryOp::Mul => "*",
                        BinaryOp::Div => "/",
                        BinaryOp::Mod => "%",
                        BinaryOp::Pow => "^",
                        _ => unreachable!(),
                    };
                    return Err(HirError::BinaryOpTypeError {
                        operator: op_str.to_string(),
                        lhs_type: lhs_type.clone(),
                        rhs_type: rhs_type.clone(),
                        expected: "Number".to_string(),
                    });
                }
            }
            BinaryOp::And | BinaryOp::Or => {
                if !matches!(lhs_type, ValueKind::Boolean | ValueKind::Unknown)
                    || !matches!(rhs_type, ValueKind::Boolean | ValueKind::Unknown)
                {
                    let op_str = match op {
                        BinaryOp::And => "and",
                        BinaryOp::Or => "or",
                        _ => unreachable!(),
                    };
                    return Err(HirError::BinaryOpTypeError {
                        operator: op_str.to_string(),
                        lhs_type: lhs_type.clone(),
                        rhs_type: rhs_type.clone(),
                        expected: "Boolean".to_string(),
                    });
                }
            }
            BinaryOp::Eq
            | BinaryOp::Ne
            | BinaryOp::Gt
            | BinaryOp::Lt
            | BinaryOp::Ge
            | BinaryOp::Le => {
                if lhs_type != rhs_type
                    && !matches!(lhs_type, ValueKind::Unknown)
                    && !matches!(rhs_type, ValueKind::Unknown)
                {
                    let op_str = match op {
                        BinaryOp::Eq => "==",
                        BinaryOp::Ne => "!=",
                        BinaryOp::Gt => ">",
                        BinaryOp::Lt => "<",
                        BinaryOp::Ge => ">=",
                        BinaryOp::Le => "<=",
                        _ => unreachable!(),
                    };
                    return Err(HirError::BinaryOpTypeError {
                        operator: op_str.to_string(),
                        lhs_type: lhs_type.clone(),
                        rhs_type: rhs_type.clone(),
                        expected: format!(
                            "compatible types (got {} and {})",
                            Self::format_value_kind(lhs_type),
                            Self::format_value_kind(rhs_type)
                        ),
                    });
                }
            }
        }
        Ok(())
    }

    /// Check if two callable types (function or thunk) are structurally compatible
    /// This does proper structural comparison instead of string equality
    fn check_callable_type_compatibility(expected: &str, actual: &str) -> bool {
        let expected_parsed = Self::parse_callable_type(expected);
        let actual_parsed = Self::parse_callable_type(actual);

        match (expected_parsed, actual_parsed) {
            (
                Some((exp_params, exp_return, exp_is_thunk)),
                Some((act_params, act_return, act_is_thunk)),
            ) => {
                // Both must be the same kind (function vs thunk)
                if exp_is_thunk != act_is_thunk {
                    return false;
                }

                // Parameter counts must match
                if exp_params.len() != act_params.len() {
                    return false;
                }

                // All parameter types must be compatible
                for (exp_param, act_param) in exp_params.iter().zip(act_params.iter()) {
                    let exp_kind = Self::parse_type_string_static(exp_param);
                    let act_kind = Self::parse_type_string_static(act_param);
                    if !Self::check_type_compatibility(&exp_kind, &act_kind) {
                        return false;
                    }
                }

                // Return types must be compatible
                let exp_ret_kind = Self::parse_type_string_static(&exp_return);
                let act_ret_kind = Self::parse_type_string_static(&act_return);
                Self::check_type_compatibility(&exp_ret_kind, &act_ret_kind)
            }
            _ => false,
        }
    }

    /// Static version of parse_type_string for use in static methods
    fn parse_type_string_static(type_str: &str) -> ValueKind {
        let trimmed = type_str.trim();

        if trimmed.contains("~>") {
            let normalized = Self::normalize_type_string(trimmed);
            return ValueKind::Thunk(normalized);
        }

        if trimmed.contains("->") {
            let normalized = Self::normalize_type_string(trimmed);
            return ValueKind::Function(normalized);
        }

        match trimmed.to_lowercase().as_str() {
            "number" | "num" | "int" | "float" => ValueKind::Number,
            "string" | "str" => ValueKind::String,
            "boolean" | "bool" => ValueKind::Boolean,
            "void" => ValueKind::Void,
            "any" | "" => ValueKind::Unknown,
            _ => ValueKind::Unknown,
        }
    }

    pub fn build(&mut self, program: Program) -> Result<&HirAst, HirError> {
        for block in program.blocks {
            let hir_block = self.process_block(block)?;

            self.ast.blocks.push(hir_block);
        }

        Ok(&self.ast)
    }

    fn intern_constant(&mut self, literal: Literal) -> u32 {
        // Convert literal to constant value
        let value = match &literal {
            Literal::String(s) => ConstantValue::String(s.clone()),
            Literal::Number(n) => ConstantValue::Number(*n),
            Literal::Boolean(n) => ConstantValue::Boolean(*n),
        };

        // Create hashable key for deduplication
        let key = ConstantKey::from_constant_value(&value);

        // Check if constant already exists
        if let Some(&existing_id) = self.constant_map.get(&key) {
            return existing_id;
        }

        // Create new constant
        let id = self.ast.constants.len() as u32;
        let kind = match &value {
            ConstantValue::String(_) => ValueKind::String,
            ConstantValue::Number(_) => ValueKind::Number,
            ConstantValue::Boolean(_) => ValueKind::Boolean,
            ConstantValue::None => ValueKind::Unknown,
        };
        self.ast.constants.push(Constant {
            id,
            name: literal.to_string() + "_literal",
            kind,
            value: value.clone(),
        });

        // Store in deduplication map
        self.constant_map.insert(key, id);

        id
    }

    /// Create a new scope for a loop and return its ID
    fn create_loop_scope(&mut self) -> ScopeId {
        let parent_scope = self.current_scope;
        let loop_scope_id = ScopeId(self.ast.scopes.scopes.len());
        self.ast.scopes.scopes.push(HirBlockContext {
            vars: Vec::new(),
            parent: Some(parent_scope),
        });
        self.current_scope = loop_scope_id;
        loop_scope_id
    }

    /// Restore scope to parent
    fn restore_scope(&mut self, parent_scope: ScopeId) {
        self.current_scope = parent_scope;
    }

    fn process_block(&mut self, block: Block) -> Result<HirBlock, HirError> {
        let parent = self.current_scope;

        let is_top_level = self.current_scope == ScopeId(0);

        let new_scope = if is_top_level {
            self.current_scope // reuse root
        } else {
            ScopeId(self.ast.scopes.scopes.len())
        };

        if !is_top_level {
            self.ast.scopes.scopes.push(HirBlockContext {
                vars: Vec::new(),
                parent: Some(parent),
            });
        }

        self.current_scope = new_scope;

        let mut hir_block = HirBlock {
            scope: new_scope,
            statements: Vec::new(),
        };

        for stmt in block.statements {
            let hir = self.process_statement(stmt)?;

            hir_block.statements.push(hir);
        }

        self.current_scope = parent;
        Ok(hir_block)
    }

    /// Process an assignment expression and validate type compatibility
    fn process_assignment(
        &mut self,
        identifier: String,
        expression: Expression,
        require_exists: bool,
    ) -> Result<(u32, HirExpression), HirError> {
        let expr = self.process_expression(expression)?;
        let actual_kind = self.infer_variable_kind(&expr);

        let slot = if require_exists {
            // For assign operations, variable must already exist
            match self.resolve_var(&identifier) {
                Some(id) => {
                    let expected_kind = self.get_var_kind(id).ok_or_else(|| {
                        HirError::UnknownVariable(format!(
                            "Variable '{}' not found in scope",
                            identifier
                        ))
                    })?;

                    if !Self::check_type_compatibility(&expected_kind, &actual_kind) {
                        return Err(HirError::TypeMismatch {
                            variable: identifier,
                            expected: expected_kind,
                            actual: actual_kind,
                        });
                    }
                    id
                }
                None => {
                    return Err(HirError::UnknownVariable(format!(
                        "Variable '{}' is not declared. Use 'let' to declare a new variable.",
                        identifier
                    )));
                }
            }
        } else {
            // For let, variable must not already exist in the current scope
            if self.var_exists_in_current_scope(&identifier) {
                return Err(HirError::VariableAlreadyDeclared(format!(
                    "Variable '{}' is already declared",
                    identifier
                )));
            }
            self.init_var(&identifier, actual_kind)
        };

        Ok((slot, expr))
    }

    /// Process a let statement with optional type annotation
    fn process_let_statement(
        &mut self,
        identifier: String,
        type_annotation: Option<String>,
        expression: Expression,
    ) -> Result<HirStmt, HirError> {
        let expr = self.process_expression(expression)?;
        let actual_kind = self.infer_variable_kind(&expr);

        // If type annotation is provided, use it and check compatibility
        let expected_kind = if let Some(type_ann) = &type_annotation {
            let parsed_kind = self.parse_type_string(type_ann);
            if !Self::check_type_compatibility(&parsed_kind, &actual_kind) {
                return Err(HirError::TypeMismatch {
                    variable: identifier.clone(),
                    expected: parsed_kind,
                    actual: actual_kind,
                });
            }
            parsed_kind
        } else {
            actual_kind
        };

        // Variable must not already exist in current scope (allows shadowing)
        if self.var_exists_in_current_scope(&identifier) {
            return Err(HirError::VariableAlreadyDeclared(format!(
                "Variable '{}' is already declared",
                identifier
            )));
        }

        let slot = self.init_var(&identifier, expected_kind);
        Ok(HirStmt::Assign { slot, value: expr })
    }

    /// Process an assign statement
    fn process_assign_statement(
        &mut self,
        identifier: String,
        expression: Expression,
    ) -> Result<HirStmt, HirError> {
        let (slot, expr) = self.process_assignment(identifier, expression, true)?;
        Ok(HirStmt::Assign { slot, value: expr })
    }

    /// Process an assign decrement statement
    fn process_assign_decrement_statement(
        &mut self,
        identifier: String,
        expression: Expression,
    ) -> Result<HirStmt, HirError> {
        let (slot, expr) = self.process_assignment(identifier, expression, true)?;
        Ok(HirStmt::AssignDecrement { slot, value: expr })
    }

    /// Process an assign increment statement
    fn process_assign_increment_statement(
        &mut self,
        identifier: String,
        expression: Expression,
    ) -> Result<HirStmt, HirError> {
        let (slot, expr) = self.process_assignment(identifier, expression, true)?;
        Ok(HirStmt::AssignIncrement { slot, value: expr })
    }

    /// Process an if statement
    fn process_if_statement(
        &mut self,
        arms: Vec<(Expression, Block)>,
        else_block: Option<Block>,
    ) -> Result<HirStmt, HirError> {
        let mut hir_arms: Vec<(HirExpression, HirBlock)> = Vec::new();

        for (expression, block) in arms {
            // Process expression in current scope (before processing the block)
            let expr = self.process_expression(expression)?;
            // Process block (creates new scope, processes statements, restores scope)
            let bl = self.process_block(block)?;
            hir_arms.push((expr, bl));
        }

        // For the else_block, we fill an empty block if None.
        let hir_else_block = match else_block {
            Some(block) => Box::new(self.process_block(block)?),
            None => Box::new(HirBlock {
                scope: self.current_scope,
                statements: vec![],
            }),
        };

        Ok(HirStmt::If {
            arms: hir_arms,
            else_block: hir_else_block,
        })
    }

    /// Process a match statement
    fn process_match_statement(
        &mut self,
        expression: Expression,
        cases: Vec<(Option<Expression>, Block)>,
    ) -> Result<HirStmt, HirError> {
        // Process the expression being matched
        let match_expr = self.process_expression(expression)?;

        // Process each case
        let mut hir_cases = Vec::new();
        for (pattern, block) in cases {
            let pattern_expr = if let Some(pat) = pattern {
                Some(self.process_expression(pat)?)
            } else {
                None // Wildcard case
            };
            let hir_block = self.process_block(block)?;
            hir_cases.push((pattern_expr, hir_block));
        }

        Ok(HirStmt::Match {
            expression: match_expr,
            cases: hir_cases,
        })
    }

    /// Process a function declaration statement
    fn process_function_declaration_statement(
        &mut self,
        identifier: String,
        arguments: Vec<Argument>,
        return_type: Option<String>,
        body: Block,
    ) -> Result<HirStmt, HirError> {
        // Parse argument types
        let mut param_types = Vec::new();
        for arg in &arguments {
            let param_kind = self.parse_type_string(&arg.kind);
            param_types.push(param_kind);
        }

        // Parse return type (default to Void if not specified)
        let return_kind = if let Some(return_type_str) = return_type {
            self.parse_type_string(&return_type_str)
        } else {
            ValueKind::Void
        };

        // Create function signature
        let signature = FunctionSignature {
            params: param_types,
            return_type: Box::new(return_kind),
        };

        // Assign a function ID
        let func_id = self.next_function_id;
        self.next_function_id += 1;

        // Save current scope
        let parent_scope = self.current_scope;

        // Create a new scope for the function body
        let func_scope_id = ScopeId(self.ast.scopes.scopes.len());
        self.ast.scopes.scopes.push(HirBlockContext {
            vars: Vec::new(),
            parent: Some(parent_scope),
        });

        // Switch to function scope
        self.current_scope = func_scope_id;

        // Initialize function parameters as variables in the function scope
        let mut param_var_ids = Vec::new();
        for arg in &arguments {
            let param_kind = self.parse_type_string(&arg.kind);
            let var_id = self.init_var(&arg.identifier, param_kind);
            param_var_ids.push(var_id);
        }

        // Create and store the function with a placeholder body first
        // This allows recursive calls within the function body to resolve the function
        let placeholder_def = FunctionDefinition {
            body: HirBlock {
                scope: func_scope_id,
                statements: vec![],
            },
            param_var_ids: param_var_ids.clone(),
            scope_id: func_scope_id,
        };

        let function = Function {
            id: func_id,
            name: identifier.clone(),
            signature,
            definition: placeholder_def,
        };

        self.ast.functions.insert(func_id, function);

        // Process the function body (now that the function is registered, recursive calls will work)
        let func_body = self.process_block(body)?;

        // Update the function with the actual body
        if let Some(func) = self.ast.functions.get_mut(&func_id) {
            func.definition.body = func_body;
        }

        // Restore parent scope
        self.current_scope = parent_scope;

        // Return a no-op statement since the function is now registered
        Ok(HirStmt::Expression(HirExpression::Constant(0))) // Dummy constant, not used
    }

    /// Process a return statement
    fn process_return_statement(
        &mut self,
        expression: Expression,
    ) -> Result<HirStmt, HirError> {
        let expr = self.process_expression(expression)?;
        Ok(HirStmt::Return { value: expr })
    }

    /// Process a while statement
    fn process_while_statement(
        &mut self,
        condition: Expression,
        body: Block,
    ) -> Result<HirStmt, HirError> {
        // Transform while loop into: loop { if !condition { break; } body }
        let parent_scope = self.current_scope;
        let loop_scope_id = self.create_loop_scope();

        // Process condition
        let condition_expr = self.process_expression(condition)?;

        // Create a break statement wrapped in an if
        // if !condition { break; }
        let not_condition = HirExpression::Unary {
            operand: Box::new(condition_expr),
            operator: UnaryOp::Not,
        };
        let break_stmt = HirStmt::Break { value: None };
        let condition_block = HirBlock {
            scope: self.current_scope,
            statements: vec![break_stmt],
        };
        let if_stmt = HirStmt::If {
            arms: vec![(not_condition, condition_block)],
            else_block: Box::new(HirBlock {
                scope: self.current_scope,
                statements: vec![],
            }),
        };

        // Process the loop body
        let hir_body = self.process_block(body)?;

        // Combine the if statement with the body
        let combined_statements = {
            let mut stmts = vec![if_stmt];
            stmts.extend(hir_body.statements);
            stmts
        };
        let combined_body = HirBlock {
            scope: loop_scope_id,
            statements: combined_statements,
        };

        self.restore_scope(parent_scope);

        // Statement loops don't have break_slot
        Ok(HirStmt::Loop {
            init_vars: vec![],
            body: combined_body,
            break_slot: None,
        })
    }

    /// Process a for statement
    fn process_for_statement(
        &mut self,
        var_name: String,
        start: Expression,
        end: Expression,
        body: Block,
    ) -> Result<HirStmt, HirError> {
        // Transform for loop into:
        // loop {
        //   var = start;  // Only on first iteration, handled by init_vars
        //   if var >= end { break; }
        //   body
        //   var = var + 1
        // }
        let parent_scope = self.current_scope;
        let loop_scope_id = self.create_loop_scope();

        // Process start and end expressions
        let start_expr = self.process_expression(start)?;
        let end_expr = self.process_expression(end)?;

        // Initialize the loop variable
        let var_kind = self.infer_variable_kind(&start_expr);
        let var_id = self.init_var(&var_name, var_kind);

        // Create condition check: if var >= end { break; }
        let var_ref = HirExpression::Identifier(var_id);
        let condition = HirExpression::Binary {
            lhs: Box::new(var_ref.clone()),
            rhs: Box::new(end_expr),
            operator: BinaryOp::Ge,
        };
        let break_stmt = HirStmt::Break { value: None };
        let condition_block = HirBlock {
            scope: self.current_scope,
            statements: vec![break_stmt],
        };
        let if_stmt = HirStmt::If {
            arms: vec![(condition, condition_block)],
            else_block: Box::new(HirBlock {
                scope: self.current_scope,
                statements: vec![],
            }),
        };

        // Process the loop body
        let hir_body = self.process_block(body)?;

        // Create increment: var = var + 1
        let one = HirExpression::Number(1.0);
        let increment_expr = HirExpression::Binary {
            lhs: Box::new(var_ref.clone()),
            rhs: Box::new(one),
            operator: BinaryOp::Add,
        };
        let increment_stmt = HirStmt::Assign {
            slot: var_id,
            value: increment_expr,
        };

        // Combine: condition check, body, increment
        let combined_statements = {
            let mut stmts = vec![if_stmt];
            stmts.extend(hir_body.statements);
            stmts.push(increment_stmt);
            stmts
        };
        let combined_body = HirBlock {
            scope: loop_scope_id,
            statements: combined_statements,
        };

        self.restore_scope(parent_scope);

        // Statement loops don't have break_slot
        Ok(HirStmt::Loop {
            init_vars: vec![(var_id, start_expr)],
            body: combined_body,
            break_slot: None,
        })
    }

    /// Process a loop statement
    fn process_loop_statement(
        &mut self,
        init_vars: Vec<(String, Expression)>,
        body: Block,
    ) -> Result<HirStmt, HirError> {
        let parent_scope = self.current_scope;
        let _loop_scope_id = self.create_loop_scope(); // Sets current_scope implicitly

        // Initialize loop variables in the loop scope
        let mut hir_init_vars = Vec::new();
        for (var_name, init_expr) in init_vars {
            let expr = self.process_expression(init_expr)?;
            let actual_kind = self.infer_variable_kind(&expr);
            let var_id = self.init_var(&var_name, actual_kind);
            hir_init_vars.push((var_id, expr));
        }

        // Process the loop body in the loop scope
        let hir_body = self.process_block(body)?;

        self.restore_scope(parent_scope);

        // Statement loops don't have break_slot
        Ok(HirStmt::Loop {
            init_vars: hir_init_vars,
            body: hir_body,
            break_slot: None,
        })
    }

    /// Process a break statement
    fn process_break_statement(
        &mut self,
        expression: Option<Expression>,
    ) -> Result<HirStmt, HirError> {
        let expr = if let Some(expr) = expression {
            Some(self.process_expression(expr)?)
        } else {
            None
        };
        Ok(HirStmt::Break { value: expr })
    }

    /// Process a continue statement
    fn process_continue_statement(&mut self) -> Result<HirStmt, HirError> {
        Ok(HirStmt::Continue)
    }

    /// Process an expression statement
    fn process_expression_statement(
        &mut self,
        expression: Expression,
    ) -> Result<HirStmt, HirError> {
        let expr = self.process_expression(expression)?;
        Ok(HirStmt::Expression(expr))
    }

    fn process_statement(&mut self, statement: Statement) -> Result<HirStmt, HirError> {
        match statement {
            Statement::Mod { identifier: _ } => {
                // Module declarations are handled at the project level, not during semantic analysis
                // They just mark the module name for this file
                Ok(HirStmt::Nop)
            }
            Statement::Let {
                identifier,
                type_annotation,
                expression,
                pub_visibility: _,
            } => self.process_let_statement(identifier, type_annotation, expression),
            Statement::Const {
                identifier,
                expression,
                pub_visibility: _,
            } => {
                // Constants are similar to let statements but are compile-time constants
                // For now, treat them like let statements
                self.process_let_statement(identifier, None, expression)
            }
            Statement::Assign {
                identifier,
                expression,
            } => self.process_assign_statement(identifier, expression),
            Statement::AssignDecrement {
                identifier,
                expression,
            } => self.process_assign_decrement_statement(identifier, expression),
            Statement::AssignIncrement {
                identifier,
                expression,
            } => self.process_assign_increment_statement(identifier, expression),
            Statement::If { arms, else_block } => {
                self.process_if_statement(arms, else_block)
            }
            Statement::Match { expression, cases } => {
                self.process_match_statement(expression, cases)
            }
            Statement::FunctionDeclaration {
                identifier,
                arguments,
                return_type,
                body,
                pub_visibility: _,
            } => self.process_function_declaration_statement(identifier, arguments, return_type, body),
            Statement::Return { expression } => self.process_return_statement(expression),
            Statement::While { condition, body } => {
                self.process_while_statement(condition, body)
            }
            Statement::For {
                var_name,
                start,
                end,
                body,
            } => self.process_for_statement(var_name, start, end, body),
            Statement::Loop { init_vars, body } => {
                self.process_loop_statement(init_vars, body)
            }
            Statement::Break { expression } => self.process_break_statement(expression),
            Statement::Continue => self.process_continue_statement(),
            Statement::Expression(expression) => {
                self.process_expression_statement(expression)
            }
            Statement::Use { path, selector } => {
                // Process imports at compile-time: resolve symbols and add to import table
                let imports = self.resolve_import(&path, &selector)?;
                // Merge imports into the import table (both in HirBuilder and HirAst)
                for (name, func_id) in imports {
                    if self.import_table.contains_key(&name) {
                        return Err(HirError::TypeError(format!("Symbol '{}' already imported", name)));
                    }
                    self.import_table.insert(name.clone(), func_id);
                    // Also store in HirAst for LSP access
                    self.ast.import_table.insert(name, func_id);
                }
                // Use statements don't generate any HIR statements (compile-time only)
                Ok(HirStmt::Nop)
            }
        }
    }

    /// Process arguments list for PostfixInvoke
    fn process_invoke_args(
        &mut self,
        args: Option<Vec<Expression>>,
    ) -> Result<Vec<HirExpression>, HirError> {
        if let Some(arg_list) = args {
            let mut processed_args = Vec::new();
            for arg in arg_list {
                processed_args.push(self.process_expression(arg)?);
            }
            Ok(processed_args)
        } else {
            Ok(Vec::new())
        }
    }

    /// Process PostfixInvoke when lhs is an Identifier
    fn process_identifier_invoke(
        &mut self,
        identifier: String,
        args: Option<Vec<Expression>>,
    ) -> Result<HirExpression, HirError> {
        // Try to resolve as variable first (for thunks/PreparedCall stored in variables)
        if let Some(var_id) = self.resolve_var_aggressive(&identifier) {
            let processed_args = self.process_invoke_args(args)?;
            return Ok(HirExpression::PostfixInvoke {
                operand: Box::new(HirExpression::Identifier(var_id)),
                args: if processed_args.is_empty() {
                    None
                } else {
                    Some(processed_args)
                },
            });
        }

        // Double-check: if it exists as a variable in current scope, prefer that
        if let Some(var_id) = self.resolve_var(&identifier) {
            let processed_args = self.process_invoke_args(args)?;
            return Ok(HirExpression::PostfixInvoke {
                operand: Box::new(HirExpression::Identifier(var_id)),
                args: if processed_args.is_empty() {
                    None
                } else {
                    Some(processed_args)
                },
            });
        }

        // TYPE CHECKING: PostfixInvoke with ! operator MUST be a thunk variable, not a function
        if let Some(_function_id) = self.resolve_function(&identifier) {
            return Err(HirError::TypeError(format!(
                "Cannot use thunk invocation syntax '{}!' on a function. Use '{}' with parentheses to call the function.",
                identifier, identifier
            )));
        }

        Err(HirError::UnknownVariable(format!(
            "{} is not a variable or function",
            identifier
        )))
    }

    /// Process PostfixInvoke when lhs is a FunctionCall
    fn process_function_call_invoke(
        &mut self,
        callee: Box<Expression>,
        fc_args: Vec<Expression>,
        invoke_args: Option<Vec<Expression>>,
    ) -> Result<HirExpression, HirError> {
        let func_expr = self.process_expression(Expression::FunctionCall {
            callee,
            arguments: fc_args,
        })?;

        match func_expr {
            HirExpression::FunctionCall {
                function_id,
                args: existing_args,
                ..
            } => {
                let mut processed_args = existing_args;
                processed_args.extend(self.process_invoke_args(invoke_args)?);
                Ok(HirExpression::FunctionCall {
                    function_id,
                    args: processed_args,
                    invoke: true,
                })
            }
            HirExpression::PostfixInvoke {
                operand,
                args: existing_args,
            } => {
                let mut processed_args = Vec::new();
                if let Some(arg_list) = existing_args {
                    processed_args.extend(arg_list);
                }
                processed_args.extend(self.process_invoke_args(invoke_args)?);
                Ok(HirExpression::PostfixInvoke {
                    operand,
                    args: if processed_args.is_empty() {
                        None
                    } else {
                        Some(processed_args)
                    },
                })
            }
            _ => {
                let processed_args = self.process_invoke_args(invoke_args)?;
                Ok(HirExpression::PostfixInvoke {
                    operand: Box::new(func_expr),
                    args: if processed_args.is_empty() {
                        None
                    } else {
                        Some(processed_args)
                    },
                })
            }
        }
    }

    /// Process PostfixInvoke when lhs is already a PostfixInvoke (nested invocation)
    fn process_nested_postfix_invoke(
        &mut self,
        operand: Box<HirExpression>,
        existing_args: Option<Vec<HirExpression>>,
        invoke_args: Option<Vec<Expression>>,
    ) -> Result<HirExpression, HirError> {
        // If outer Postfix has no extra args and inner already has args, keep as-is
        if invoke_args.is_none() && existing_args.is_some() {
            return Ok(HirExpression::PostfixInvoke {
                operand,
                args: existing_args,
            });
        }

        // Otherwise, combine existing args with new args
        let mut processed_args = Vec::new();
        if let Some(arg_list) = existing_args {
            processed_args.extend(arg_list);
        }
        processed_args.extend(self.process_invoke_args(invoke_args)?);

        Ok(HirExpression::PostfixInvoke {
            operand,
            args: if processed_args.is_empty() {
                None
            } else {
                Some(processed_args)
            },
        })
    }

    /// Check if an expression contains a PostfixInvoke (nested ! operator)
    /// This helps detect confusing patterns like mul2(add10(i)!)!
    #[allow(dead_code)]
    fn expression_contains_postfix_invoke(expr: &Expression) -> bool {
        match expr {
            Expression::Postfix { op, .. } => {
                matches!(op, PostfixOp::Invoke)
            }
            Expression::FunctionCall { arguments, .. } => {
                // Check if any argument contains PostfixInvoke
                arguments
                    .iter()
                    .any(|arg| Self::expression_contains_postfix_invoke(arg))
            }
            Expression::PartialCall { args, .. } => {
                // Check if any argument contains PostfixInvoke
                args.iter().any(|arg| match arg {
                    CallArgument::Expr(expr) => Self::expression_contains_postfix_invoke(expr),
                    CallArgument::Hole => false,
                })
            }
            Expression::Infix { lhs, rhs, .. } => {
                Self::expression_contains_postfix_invoke(lhs)
                    || Self::expression_contains_postfix_invoke(rhs)
            }
            Expression::Prefix { rhs, .. } => Self::expression_contains_postfix_invoke(rhs),
            Expression::Group(inner) => Self::expression_contains_postfix_invoke(inner),
            _ => false,
        }
    }

    /// Process a literal expression
    fn process_literal_expression(
        &mut self,
        lit: Literal,
    ) -> Result<HirExpression, HirError> {
        let cid = self.intern_constant(lit);
        Ok(HirExpression::Constant(cid))
    }

    /// Process a member access expression (e.g., utils.add)
    fn process_member_access_expression(
        &mut self,
        object: Expression,
        member: String,
    ) -> Result<HirExpression, HirError> {
        // The object should be an identifier (module name)
        let module_name = match object {
            Expression::Identifier(name) => name,
            _ => {
                return Err(HirError::TypeError(
                    format!("Member access object must be an identifier, got: {:?}", object)
                ));
            }
        };
        
        // Look up the module
        let module = self.modules.get(&module_name)
            .ok_or_else(|| HirError::TypeError(format!("Module '{}' not found", module_name)))?;
        
        // Look up the member in the module
        if let Some(function_id) = module.functions.get(&member) {
            // It's a function
            Ok(HirExpression::FunctionCall {
                function_id: *function_id,
                args: Vec::new(),
                invoke: false,
            })
        } else if let Some(constant_id) = module.constants.get(&member) {
            // It's a constant (stored as a variable in HIR)
            // Return the variable ID so it can be loaded
            Ok(HirExpression::Identifier(*constant_id))
        } else {
            // TODO: Also check variables
            Err(HirError::TypeError(
                format!("Member '{}' not found in module '{}'", member, module_name)
            ))
        }
    }

    /// Process an identifier expression
    fn process_identifier_expression(
        &mut self,
        identifier: String,
    ) -> Result<HirExpression, HirError> {
        // First check imported symbols (compile-time resolved)
        if let Some(imported_id) = self.import_table.get(&identifier) {
            // Check if this is a variable ID (constant) or function ID
            // Variable IDs are typically < 10000, function IDs for built-ins are >= 10000
            // But we can also check if it exists as a variable in any scope
            if self.ast.scopes.scopes.iter().any(|scope| {
                scope.vars.iter().any(|v| v.id == *imported_id)
            }) {
                // It's a variable (constant) - return as identifier
                Ok(HirExpression::Identifier(*imported_id))
            } else {
                // It's a function - convert to thunk by calling with no args
                Ok(HirExpression::FunctionCall {
                    function_id: *imported_id,
                    args: Vec::new(),
                    invoke: false,
                })
            }
        } else if let Some(slot) = self.resolve_var(&identifier) {
            Ok(HirExpression::Identifier(slot))
        } else if let Some(const_id) = self.resolve_const(&identifier) {
            Ok(HirExpression::Constant(const_id))
        } else if let Some(function_id) = self.resolve_function(&identifier) {
            // Function name used as identifier - convert to thunk by calling with no args
            // This allows functions to be used in compositions like: square <| add10
            Ok(HirExpression::FunctionCall {
                function_id,
                args: Vec::new(),
                invoke: false,
            })
        } else {
            Err(HirError::UnknownVariable(identifier))
        }
    }

    /// Process an infix (binary) expression
    fn process_infix_expression(
        &mut self,
        lhs: Expression,
        op: BinaryOp,
        rhs: Expression,
    ) -> Result<HirExpression, HirError> {
        let lhs_expr = self.process_expression(lhs)?;
        let rhs_expr = self.process_expression(rhs)?;

        // Type check binary operations
        let lhs_type = self.infer_variable_kind(&lhs_expr);
        let rhs_type = self.infer_variable_kind(&rhs_expr);
        self.check_binary_op_types(&op, &lhs_type, &rhs_type)?;

        Ok(HirExpression::Binary {
            lhs: Box::new(lhs_expr),
            rhs: Box::new(rhs_expr),
            operator: op,
        })
    }

    /// Process a prefix (unary) expression
    fn process_prefix_expression(
        &mut self,
        op: UnaryOp,
        rhs: Expression,
    ) -> Result<HirExpression, HirError> {
        let rhs_expr = self.process_expression(rhs)?;

        Ok(HirExpression::Unary {
            operand: Box::new(rhs_expr),
            operator: op,
        })
    }

    /// Process a postfix expression
    fn process_postfix_expression(
        &mut self,
        lhs: Expression,
        op: PostfixOp,
        args: Option<Vec<Expression>>,
    ) -> Result<HirExpression, HirError> {
        match op {
            PostfixOp::Invoke => {
                // Special handling: if lhs is a FunctionCall, process it first
                if let Expression::FunctionCall {
                    callee,
                    arguments: fc_args,
                } = lhs
                {
                    return self.process_function_call_invoke(callee, fc_args, args);
                }

                // If lhs is an Identifier, check if it's a variable first
                if let Expression::Identifier(identifier) = lhs {
                    return self.process_identifier_invoke(identifier, args);
                }

                // For other expressions, process normally
                let lhs_expr = self.process_expression(lhs)?;
                match lhs_expr {
                    HirExpression::FunctionCall {
                        function_id,
                        args: existing_args,
                        ..
                    } => Ok(HirExpression::FunctionCall {
                        function_id,
                        args: existing_args,
                        invoke: true,
                    }),
                    HirExpression::PostfixInvoke {
                        operand,
                        args: existing_args,
                    } => self.process_nested_postfix_invoke(operand, existing_args, args),
                    other => {
                        let processed_args = self.process_invoke_args(args)?;
                        Ok(HirExpression::PostfixInvoke {
                            operand: Box::new(other),
                            args: if processed_args.is_empty() {
                                None
                            } else {
                                Some(processed_args)
                            },
                        })
                    }
                }
            }
        }
    }

    /// Process a function call expression
    fn process_function_call_expression(
        &mut self,
        callee: Expression,
        arguments: Vec<Expression>,
    ) -> Result<HirExpression, HirError> {
        // Special case: if callee is an Identifier, check if it's a function name first
        // before processing it as an expression
        if let Expression::Identifier(ref identifier_name) = callee {
            // Check imported symbols first (compile-time resolved)
            let imported_func_id = self.import_table.get(identifier_name).copied();
            let regular_func_id = self.resolve_function(identifier_name);
            
            if let Some(function_id) = imported_func_id.or(regular_func_id) {
                // It's a function (imported or regular) - create FunctionCall with invoke: false (prepare the call)
                let mut args: Vec<HirExpression> = Vec::new();
                for argument in arguments {
                    args.push(self.process_expression(argument)?);
                }
                return Ok(HirExpression::FunctionCall {
                    function_id,
                    args,
                    invoke: false,
                });
            }
            // Not a function - will be handled as variable below
        }

        // Process the callee expression
        let callee_expr = self.process_expression(callee)?;

        // Process arguments
        let mut processed_args = Vec::new();
        for argument in arguments {
            processed_args.push(self.process_expression(argument)?);
        }

        // Handle different callee types
        match callee_expr {
            // If callee is an identifier (variable/thunk)
            HirExpression::Identifier(var_id) => {
                // This is a variable - allow function call syntax on thunks
                // This is the new semantics: f(x) creates a CallNode, not an invocation
                Ok(HirExpression::PostfixInvoke {
                    operand: Box::new(HirExpression::Identifier(var_id)),
                    args: Some(processed_args),
                })
            }
            // If callee is a ComposeThunk, wrap it in a PostfixInvoke
            HirExpression::ComposeThunk { first, second } => {
                // Create a PostfixInvoke with the ComposeThunk as operand
                // The bytecode emitter will handle calling a ComposeThunk
                Ok(HirExpression::PostfixInvoke {
                    operand: Box::new(HirExpression::ComposeThunk { first, second }),
                    args: Some(processed_args),
                })
            }
            // If callee is a FunctionCall (nested call), combine the args
            HirExpression::FunctionCall {
                function_id,
                args: existing_args,
                invoke: false,
            } => {
                // This is a prepared call being called again - combine the args
                let mut combined_args = existing_args;
                combined_args.extend(processed_args);
                Ok(HirExpression::FunctionCall {
                    function_id,
                    args: combined_args,
                    invoke: false,
                })
            }
            // Other expressions - try to treat as callable
            _ => {
                // For other expressions, create a PostfixInvoke
                // This allows calling any expression that evaluates to a callable
                Ok(HirExpression::PostfixInvoke {
                    operand: Box::new(callee_expr),
                    args: Some(processed_args),
                })
            }
        }
    }

    /// Process a partial call expression
    fn process_partial_call_expression(
        &mut self,
        func: Expression,
        args: Vec<CallArgument>,
    ) -> Result<HirExpression, HirError> {
        // Process the function identifier
        let func_id = if let Expression::Identifier(ref identifier_name) = func {
            // Check if it's a function
            if let Some(function_id) = self.resolve_function(identifier_name) {
                function_id
            } else {
                return Err(HirError::UnknownVariable(identifier_name.clone()));
            }
        } else {
            // For now, only support identifiers as function names in partial calls
            return Err(HirError::TypeError(
                "Partial call function must be an identifier".to_string(),
            ));
        };

        // Process arguments - convert CallArgument to Option<HirExpression>
        let mut bound: Vec<Option<HirExpression>> = Vec::new();
        for arg in args {
            match arg {
                CallArgument::Hole => bound.push(None),
                CallArgument::Expr(expr) => {
                    bound.push(Some(self.process_expression(expr)?));
                }
            }
        }

        Ok(HirExpression::PartialCall { func_id, bound })
    }

    /// Process a group expression (parentheses)
    fn process_group_expression(
        &mut self,
        expr: Expression,
    ) -> Result<HirExpression, HirError> {
        // Group expressions (parentheses) just unwrap and process the inner expression
        self.process_expression(expr)
    }

    /// Process a compose expression
    fn process_compose_expression(
        &mut self,
        lhs: Expression,
        rhs: Expression,
        reverse: bool,
    ) -> Result<HirExpression, HirError> {
        // Process both sides
        let first_expr = self.process_expression(lhs)?;
        let second_expr = self.process_expression(rhs)?;

        // For reverse composition (<|), swap the operands
        // f <| g means f(g(x)), so we want to process g first, then f
        if reverse {
            Ok(HirExpression::ComposeThunk {
                first: Box::new(second_expr),
                second: Box::new(first_expr),
            })
        } else {
            // f |> g means g(f(x)), so process f first, then g
            Ok(HirExpression::ComposeThunk {
                first: Box::new(first_expr),
                second: Box::new(second_expr),
            })
        }
    }

    /// Process a loop expression
    fn process_loop_expression(
        &mut self,
        init_vars: Vec<(String, Expression)>,
        body: Block,
    ) -> Result<HirExpression, HirError> {
        let parent_scope = self.current_scope;
        let _loop_scope_id = self.create_loop_scope(); // Sets current_scope implicitly

        // Initialize loop variables in the loop scope
        let mut hir_init_vars = Vec::new();
        for (var_name, init_expr) in init_vars {
            let expr = self.process_expression(init_expr)?;
            let actual_kind = self.infer_variable_kind(&expr);
            let var_id = self.init_var(&var_name, actual_kind);
            hir_init_vars.push((var_id, expr));
        }

        // Allocate a break_slot for expression-valued loops
        let break_slot = Some(self.next_var_id);
        self.next_var_id += 1;

        // Create a temporary variable for the break slot
        let break_slot_name = format!("__break_slot_{}", break_slot.unwrap());
        self.init_var(&break_slot_name, ValueKind::Unknown);

        // Process the loop body in the loop scope
        let hir_body = self.process_block(body)?;

        self.restore_scope(parent_scope);

        Ok(HirExpression::Loop {
            init_vars: hir_init_vars,
            body: hir_body,
            break_slot,
        })
    }

    fn process_expression(&mut self, expression: Expression) -> Result<HirExpression, HirError> {
        match expression {
            Expression::Literal(lit) => self.process_literal_expression(lit),
            Expression::Identifier(identifier) => {
                self.process_identifier_expression(identifier)
            }
            Expression::MemberAccess { object, member } => {
                self.process_member_access_expression(*object, member)
            }
            Expression::Infix { lhs, op, rhs } => {
                self.process_infix_expression(*lhs, op, *rhs)
            }
            Expression::Prefix { op, rhs } => self.process_prefix_expression(op, *rhs),
            Expression::Postfix { lhs, op, args } => {
                self.process_postfix_expression(*lhs, op, args)
            }
            Expression::FunctionCall { callee, arguments } => {
                self.process_function_call_expression(*callee, arguments)
            }
            Expression::PartialCall { func, args } => {
                self.process_partial_call_expression(*func, args)
            }
            Expression::Group(expr) => self.process_group_expression(*expr),
            Expression::Compose { lhs, rhs, reverse } => {
                self.process_compose_expression(*lhs, *rhs, reverse)
            }
            Expression::Loop { init_vars, body } => {
                self.process_loop_expression(init_vars, body)
            }
        }
    }
}

