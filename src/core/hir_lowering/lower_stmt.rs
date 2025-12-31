//! Statement lowering: AST Statement → HIR Statement
//!
//! This module handles lowering AST statements to HIR statements and blocks,
//! including the main HirBuilder implementation.

use std::collections::HashMap;

use crate::core::ast::{
    Argument, BinaryOp, Block, CallArgument, Expression, Literal, PostfixOp, Program, Statement,
    UnaryOp,
};
use serde::Serialize;

use super::{
    scopes::{HirBlockContext, ScopeArena, ScopeId},
    Constant, ConstantValue, Function, FunctionDefinition, FunctionSignature, HirAst, HirError,
    HirExpression, ImportTable, Module, ReducerType, ValueKind, Variable,
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
#[derive(Debug, Clone, Serialize)]
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
#[derive(Debug, Clone, Serialize)]
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
    pub modules: HashMap<String, Module>,
    /// Maps module name to its import table
    module_imports: HashMap<String, ImportTable>,

    /// The current module being processed
    current_module: Option<String>,
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
            ast: HirAst::default(),
            current_scope: root,
            next_var_id: 0,
            next_function_id: 0,
            constant_map: HashMap::new(),
            modules: HashMap::new(),
            module_imports: HashMap::new(),
            current_module: None,
        }
    }

    pub fn reset_hir_only(&mut self) {
        // Reset HIR output
        self.ast = HirAst {
            constants: Vec::new(),
            blocks: Vec::new(),
            scopes: ScopeArena::default(),
            functions: HashMap::new(),
            module_imports: HashMap::new(),
            imported_constant_values: HashMap::new(),
        };

        // Reset lowering state - ensure scopes are properly reset
        // Recreate the root scope
        self.ast.scopes.scopes.clear();
        self.ast.scopes.scopes.push(HirBlockContext {
            vars: Vec::new(),
            parent: None,
        });
        self.current_scope = ScopeId(0);
        self.next_var_id = 0;
        self.next_function_id = 0;
        self.constant_map.clear();
        self.current_module = None;
    }

    /// Check if the HirBuilder has any active scopes beyond the root scope.
    /// Used for debug assertions to catch reuse bugs.
    pub fn has_active_scope(&self) -> bool {
        // If we have more than just the root scope (index 0), we have active scopes
        self.ast.scopes.scopes.len() > 1 || self.current_scope != ScopeId(0)
    }

    pub fn take_ast(&mut self) -> HirAst {
        std::mem::take(&mut self.ast)
    }

    pub fn set_current_module(&mut self, module: Option<String>) {
        self.current_module = module;
    }

    /// Get the import table for the current module
    fn get_current_imports(&self) -> Option<&ImportTable> {
        self.current_module
            .as_ref()
            .and_then(|m| self.module_imports.get(m))
    }

    /// Get a mutable reference to the current module's import table
    fn get_current_imports_mut(&mut self) -> Option<&mut ImportTable> {
        self.current_module
            .as_ref()
            .and_then(|module_name| self.module_imports.get_mut(module_name))
    }

    /// Resolve an imported symbol in the current module
    fn resolve_import_in_current_module(&self, name: &str) -> Option<u32> {
        self.get_current_imports()
            .and_then(|imports| imports.get(name).copied())
    }

    /// Add a symbol to the current module's import table
    fn add_import_to_current_module(&mut self, name: String, id: u32) -> Result<(), HirError> {
        let module_name = self.current_module.clone().ok_or_else(|| {
            HirError::TypeError("Cannot import symbols without a module declaration".to_string())
        })?;

        self.module_imports
            .entry(module_name.clone())
            .or_insert_with(HashMap::new)
            .insert(name.clone(), id);

        // Also update the HirAst for LSP access
        self.ast
            .module_imports
            .entry(module_name)
            .or_insert_with(HashMap::new)
            .insert(name, id);

        Ok(())
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
            ValueKind::Array(inner) => {
                let inner_str = Self::format_value_kind_for_type(inner);
                format!("{}[]", inner_str)
            }
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
            ValueKind::Array(inner) => {
                let inner_str = Self::format_value_kind(inner);
                format!("Array<{}>", inner_str)
            }
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
            (ValueKind::Array(expected_inner), ValueKind::Array(actual_inner)) => {
                // Arrays are compatible if their inner types are compatible
                Self::check_type_compatibility(expected_inner, actual_inner)
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

    fn infer_partial_call_kind(
        &self,
        func_id: &u32,
        bound: &Vec<Option<HirExpression>>,
    ) -> ValueKind {
        if let Some(func) = self.ast.functions.get(func_id) {
            let return_type_str = Self::format_value_kind_for_type(&func.signature.return_type);

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
            HirExpression::Boolean(_) => ValueKind::Boolean,
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
            HirExpression::Array(elements) => {
                // Infer array type from first element (if any)
                if let Some(first) = elements.first() {
                    let inner_type = self.infer_variable_kind(first);
                    ValueKind::Array(Box::new(inner_type))
                } else {
                    // Empty array - use Unknown for inner type
                    ValueKind::Array(Box::new(ValueKind::Unknown))
                }
            }
            HirExpression::ArrayIndex { array, .. } => {
                // Array index returns the element type of the array
                let array_type = self.infer_variable_kind(array);
                match array_type {
                    ValueKind::Array(inner_type) => *inner_type,
                    ValueKind::Unknown => ValueKind::Unknown, // If array type is unknown, index type is unknown
                    _ => ValueKind::Unknown, // Should not happen, but handle gracefully
                }
            }
            HirExpression::ArraySlice { array, .. } => {
                // Array slice returns an array of the same element type
                let array_type = self.infer_variable_kind(array);
                match array_type {
                    ValueKind::Array(inner_type) => ValueKind::Array(inner_type),
                    ValueKind::Unknown => ValueKind::Array(Box::new(ValueKind::Unknown)),
                    _ => ValueKind::Array(Box::new(ValueKind::Unknown)),
                }
            }
            HirExpression::Reducer {
                reducer_type,
                reducer_args,
                ..
            } => {
                // Reducer returns the accumulator type
                match reducer_type {
                    ReducerType::Sum => ValueKind::Number, // sum always returns number
                    ReducerType::Fold => {
                        // fold(init, fn) returns the type of init
                        if let Some(init) = reducer_args.first() {
                            self.infer_variable_kind(init)
                        } else {
                            ValueKind::Unknown
                        }
                    }
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

    pub fn resolve_function(&self, ctx: &crate::core::compileSession::CompileContext, name: &str) -> Option<u32> {
        // 1. Imports first
        if let Some(imported_id) = self.resolve_import_in_current_module(name) {
            if self.ast.functions.contains_key(&imported_id) || (ctx.is_native_function)(imported_id) {
                return Some(imported_id);
            }
        }

        // 2. Find function by name
        let (func_id, _) = self.ast.functions.iter().find(|(_, f)| f.name == name)?;

        // 3. Check module ownership
        if let Some(current_module) = &self.current_module {
            // If function belongs to current module → allowed
            if let Some(module) = self.modules.get(current_module) {
                if module.functions.values().any(|&id| id == *func_id) {
                    return Some(*func_id);
                }
            }
        }

        // 4. Otherwise, function must be imported
        let belongs_to_some_module = self
            .modules
            .values()
            .any(|module| module.functions.values().any(|&id| id == *func_id));

        if belongs_to_some_module {
            return None;
        }

        // 5. Local or built-in function
        Some(*func_id)
    }

    pub fn register_builtin_function(&mut self, name: &str, signature: FunctionSignature, id: u32) {
        // Register a built-in function (from CompileContext) in the HIR function registry
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

    /// Add a symbol to the current module's import table
    /// This replaces the old add_to_import_table method
    pub fn add_to_import_table(&mut self, name: String, id: u32) {
        // This method should now be scoped to the current module
        if let Err(e) = self.add_import_to_current_module(name, id) {
            // Log error or handle appropriately
            eprintln!("Warning: Failed to add import: {:?}", e);
        }
    }

    /// Register a module that can be imported.
    ///
    /// # Arguments
    /// * `path` - Dot-separated module path (e.g., "math.utils")
    /// * `functions` - Map of function names to their function IDs
    /// * `constants` - Map of constant names to their constant IDs
    pub fn register_module(
        &mut self,
        path: &str,
        functions: HashMap<String, u32>,
        constants: HashMap<String, u32>,
    ) {
        self.modules.insert(
            path.to_string(),
            Module {
                functions,
                constants,
            },
        );
    }

    /// Copy all modules from another HirBuilder.
    /// Used when creating a fresh HirBuilder for LSP compilation.
    pub fn copy_modules_from(&mut self, other: &HirBuilder) {
        // Copy module definitions
        for (path, module) in &other.modules {
            self.modules.insert(
                path.clone(),
                Module {
                    functions: module.functions.clone(),
                    constants: module.constants.clone(),
                },
            );
        }

        // NEW: Also copy module imports
        for (module_name, imports) in &other.module_imports {
            self.module_imports
                .insert(module_name.clone(), imports.clone());
        }
    }

    /// Resolve an import path and selector to function IDs or constant IDs.
    /// Returns a map of imported symbol names to IDs.
    /// Functions return function IDs, constants return constant IDs.
    fn resolve_import(
        &self,
        path: &[String],
        selector: &crate::core::ast::ImportSelector,
    ) -> Result<ImportTable, HirError> {
        let module_path = path.join(".");

        let module = self
            .modules
            .get(&module_path)
            .ok_or_else(|| HirError::TypeError(format!("Module '{}' not found", module_path)))?;

        let mut imports = ImportTable::new();

        match selector {
            crate::core::ast::ImportSelector::Single(name) => {
                // Check functions first, then constants
                if let Some(func_id) = module.functions.get(name) {
                    imports.insert(name.clone(), *func_id);
                } else if let Some(const_id) = module.constants.get(name) {
                    // Constants are stored as constant IDs
                    imports.insert(name.clone(), *const_id);
                } else {
                    return Err(HirError::TypeError(format!(
                        "Function or constant '{}' not found in module '{}'",
                        name, module_path
                    )));
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
                        return Err(HirError::TypeError(format!(
                            "Function or constant '{}' not found in module '{}'",
                            name, module_path
                        )));
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
    /// Supports: simple types (num, str, bool), array types ([num], [string]),
    /// function types (num -> num), and thunk types (num ~> num)
    fn parse_type_string(&self, type_str: &str) -> ValueKind {
        let trimmed = type_str.trim();

        // Check for array type (starts with "[")
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            // Extract inner type: [num] -> num
            let inner = &trimmed[1..trimmed.len() - 1].trim();
            let inner_kind = self.parse_type_string(inner);
            return ValueKind::Array(Box::new(inner_kind));
        }

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
                        BinaryOp::And => "&&",
                        BinaryOp::Or => "||",
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

        // Check for array type (starts with "[")
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            // Extract inner type: [num] -> num
            let inner = &trimmed[1..trimmed.len() - 1].trim();
            let inner_kind = Self::parse_type_string_static(inner);
            return ValueKind::Array(Box::new(inner_kind));
        }

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

    pub fn finalize(self) -> HirAst {
        self.ast
    }

    pub fn build_append(&mut self, ctx: &crate::core::compileSession::CompileContext, program: Program) -> Result<(), HirError> {
        debug_assert!(
            self.current_module.is_some(),
            "build_append called without current_module set"
        );

        for block in program.blocks {
            let hir_block = self.process_block(ctx, block)?;
            self.ast.blocks.push(hir_block);
        }
        Ok(())
    }

    // fn intern_constant(&mut self, literal: Literal) -> u32 {
    //     // Convert literal to constant value
    //     let value = match &literal {
    //         Literal::String(s) => ConstantValue::String(s.clone()),
    //         Literal::Number(n) => ConstantValue::Number(*n),
    //         Literal::Boolean(n) => ConstantValue::Boolean(*n),
    //     };

    //     self.intern_constant_value(value)
    // }

    /// Intern a constant value (for constant folding and constant declarations).
    fn intern_constant_value(&mut self, value: ConstantValue) -> u32 {
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
            name: format!("const_{}", id), // Generic name for folded constants
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

    fn process_block(&mut self, ctx: &crate::core::compileSession::CompileContext, block: Block) -> Result<HirBlock, HirError> {
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
            let hir = self.process_statement(ctx, stmt)?;

            hir_block.statements.push(hir);
        }

        self.current_scope = parent;
        Ok(hir_block)
    }

    /// Process an assignment expression and validate type compatibility
    fn process_assignment(
        &mut self,
        ctx: &crate::core::compileSession::CompileContext,
        identifier: String,
        expression: Expression,
        require_exists: bool,
    ) -> Result<(u32, HirExpression), HirError> {
        let expr = self.process_expression(ctx, expression)?;
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

    /// Process a const statement - must be compile-time evaluable
    fn process_const_statement(
        &mut self,
        identifier: String,
        expression: Expression,
        pub_visibility: bool,
    ) -> Result<HirStmt, HirError> {
        // Evaluate the expression at compile time
        let constant_value = self.compile_time_evaluate(&expression)?;

        // Determine the kind from the value
        let kind = match &constant_value {
            ConstantValue::Number(_) => ValueKind::Number,
            ConstantValue::String(_) => ValueKind::String,
            ConstantValue::Boolean(_) => ValueKind::Boolean,
            ConstantValue::None => {
                return Err(HirError::TypeError(
                    "Constant must have a compile-time evaluable value".to_string(),
                ))
            }
        };

        // Variable must not already exist in current scope
        if self.var_exists_in_current_scope(&identifier) {
            return Err(HirError::VariableAlreadyDeclared(format!(
                "Constant '{}' is already declared",
                identifier
            )));
        }

        // Create a constant entry
        let const_id = self.ast.constants.len() as u32;
        let key = ConstantKey::from_constant_value(&constant_value);

        // Check if constant already exists (deduplication)
        let const_id = if let Some(&existing_id) = self.constant_map.get(&key) {
            existing_id
        } else {
            self.ast.constants.push(Constant {
                id: const_id,
                name: identifier.clone(),
                kind: kind.clone(),
                value: constant_value,
            });
            self.constant_map.insert(key, const_id);
            const_id
        };

        // Create a variable for the constant (so it can be referenced)
        let slot = self.init_var(&identifier, kind);

        // Register pub constants in the module registry
        // Store constant ID (not variable slot ID) - imports need the constant, not the storage
        if pub_visibility {
            if let Some(module_name) = &self.current_module {
                self.modules
                    .entry(module_name.clone())
                    .or_insert_with(|| Module {
                        functions: HashMap::new(),
                        constants: HashMap::new(),
                    })
                    .constants
                    .insert(identifier.clone(), const_id);
            }
        }

        // Store the constant ID in the variable name for lookup
        // Actually, we need to track which variables are constants
        // For now, we'll use the constant directly in the HIR expression
        Ok(HirStmt::Assign {
            slot,
            value: HirExpression::Constant(const_id),
        })
    }

    /// Compile-time evaluate an expression.
    /// Returns the constant value if the expression can be evaluated at compile time.
    /// Returns an error if the expression cannot be evaluated at compile time.
    fn compile_time_evaluate(&self, expr: &Expression) -> Result<ConstantValue, HirError> {
        match expr {
            Expression::Literal(lit) => {
                Ok(match lit {
                    Literal::Number(n) => ConstantValue::Number(*n),
                    Literal::String(s) => ConstantValue::String(s.clone()),
                    Literal::Boolean(b) => ConstantValue::Boolean(*b),
                })
            }
            Expression::Identifier(name) => {
                // Look up constant by name
                if let Some(const_id) = self.resolve_const(name) {
                    // Find the constant value
                    if let Some(constant) = self.ast.constants.iter().find(|c| c.id == const_id) {
                        Ok(constant.value.clone())
                    } else {
                        Err(HirError::TypeError(format!(
                            "Constant '{}' not found",
                            name
                        )))
                    }
                } else {
                    Err(HirError::TypeError(format!(
                        "Constant expression cannot reference variable '{}'. Only constants can be referenced in constant expressions.",
                        name
                    )))
                }
            }
            Expression::Infix { lhs, op, rhs } => {
                let lhs_val = self.compile_time_evaluate(lhs)?;
                let rhs_val = self.compile_time_evaluate(rhs)?;
                self.evaluate_binary_op(op, &lhs_val, &rhs_val)
            }
            Expression::Prefix { op, rhs } => {
                let rhs_val = self.compile_time_evaluate(rhs)?;
                self.evaluate_unary_op(op, &rhs_val)
            }
            Expression::Group(inner) => {
                self.compile_time_evaluate(inner)
            }
            _ => {
                Err(HirError::TypeError(format!(
                    "Constant expression must be compile-time evaluable. Expressions like function calls, loops, and member access are not allowed in constant declarations."
                )))
            }
        }
    }

    /// Evaluate a binary operation on constant values.
    fn evaluate_binary_op(
        &self,
        op: &BinaryOp,
        lhs: &ConstantValue,
        rhs: &ConstantValue,
    ) -> Result<ConstantValue, HirError> {
        match op {
            BinaryOp::Add => match (lhs, rhs) {
                (ConstantValue::Number(a), ConstantValue::Number(b)) => {
                    Ok(ConstantValue::Number(a + b))
                }
                (ConstantValue::String(a), ConstantValue::String(b)) => {
                    Ok(ConstantValue::String(format!("{}{}", a, b)))
                }
                (ConstantValue::String(a), ConstantValue::Number(b)) => {
                    Ok(ConstantValue::String(format!("{}{}", a, b)))
                }
                (ConstantValue::Number(a), ConstantValue::String(b)) => {
                    Ok(ConstantValue::String(format!("{}{}", a, b)))
                }
                _ => Err(HirError::TypeError(format!(
                    "Invalid operands for addition: {:?} and {:?}",
                    lhs, rhs
                ))),
            },
            BinaryOp::Sub => match (lhs, rhs) {
                (ConstantValue::Number(a), ConstantValue::Number(b)) => {
                    Ok(ConstantValue::Number(a - b))
                }
                _ => Err(HirError::TypeError(format!(
                    "Subtraction requires number operands, got {:?} and {:?}",
                    lhs, rhs
                ))),
            },
            BinaryOp::Mul => match (lhs, rhs) {
                (ConstantValue::Number(a), ConstantValue::Number(b)) => {
                    Ok(ConstantValue::Number(a * b))
                }
                _ => Err(HirError::TypeError(format!(
                    "Multiplication requires number operands, got {:?} and {:?}",
                    lhs, rhs
                ))),
            },
            BinaryOp::Div => match (lhs, rhs) {
                (ConstantValue::Number(a), ConstantValue::Number(b)) => {
                    if *b == 0.0 {
                        return Err(HirError::TypeError(
                            "Division by zero in constant expression".to_string(),
                        ));
                    }
                    Ok(ConstantValue::Number(a / b))
                }
                _ => Err(HirError::TypeError(format!(
                    "Division requires number operands, got {:?} and {:?}",
                    lhs, rhs
                ))),
            },
            BinaryOp::Mod => match (lhs, rhs) {
                (ConstantValue::Number(a), ConstantValue::Number(b)) => {
                    if *b == 0.0 {
                        return Err(HirError::TypeError(
                            "Modulo by zero in constant expression".to_string(),
                        ));
                    }
                    Ok(ConstantValue::Number(a % b))
                }
                _ => Err(HirError::TypeError(format!(
                    "Modulo requires number operands, got {:?} and {:?}",
                    lhs, rhs
                ))),
            },
            BinaryOp::Pow => match (lhs, rhs) {
                (ConstantValue::Number(a), ConstantValue::Number(b)) => {
                    Ok(ConstantValue::Number(a.powf(*b)))
                }
                _ => Err(HirError::TypeError(format!(
                    "Power requires number operands, got {:?} and {:?}",
                    lhs, rhs
                ))),
            },
            BinaryOp::Eq => Ok(ConstantValue::Boolean(lhs == rhs)),
            BinaryOp::Ne => Ok(ConstantValue::Boolean(lhs != rhs)),
            BinaryOp::Gt => match (lhs, rhs) {
                (ConstantValue::Number(a), ConstantValue::Number(b)) => {
                    Ok(ConstantValue::Boolean(a > b))
                }
                _ => Err(HirError::TypeError(format!(
                    "Comparison requires number operands, got {:?} and {:?}",
                    lhs, rhs
                ))),
            },
            BinaryOp::Lt => match (lhs, rhs) {
                (ConstantValue::Number(a), ConstantValue::Number(b)) => {
                    Ok(ConstantValue::Boolean(a < b))
                }
                _ => Err(HirError::TypeError(format!(
                    "Comparison requires number operands, got {:?} and {:?}",
                    lhs, rhs
                ))),
            },
            BinaryOp::Ge => match (lhs, rhs) {
                (ConstantValue::Number(a), ConstantValue::Number(b)) => {
                    Ok(ConstantValue::Boolean(a >= b))
                }
                _ => Err(HirError::TypeError(format!(
                    "Comparison requires number operands, got {:?} and {:?}",
                    lhs, rhs
                ))),
            },
            BinaryOp::Le => match (lhs, rhs) {
                (ConstantValue::Number(a), ConstantValue::Number(b)) => {
                    Ok(ConstantValue::Boolean(a <= b))
                }
                _ => Err(HirError::TypeError(format!(
                    "Comparison requires number operands, got {:?} and {:?}",
                    lhs, rhs
                ))),
            },
            BinaryOp::And => match (lhs, rhs) {
                (ConstantValue::Boolean(a), ConstantValue::Boolean(b)) => {
                    Ok(ConstantValue::Boolean(*a && *b))
                }
                _ => Err(HirError::TypeError(format!(
                    "Logical AND requires boolean operands, got {:?} and {:?}",
                    lhs, rhs
                ))),
            },
            BinaryOp::Or => match (lhs, rhs) {
                (ConstantValue::Boolean(a), ConstantValue::Boolean(b)) => {
                    Ok(ConstantValue::Boolean(*a || *b))
                }
                _ => Err(HirError::TypeError(format!(
                    "Logical OR requires boolean operands, got {:?} and {:?}",
                    lhs, rhs
                ))),
            },
        }
    }

    /// Evaluate a unary operation on a constant value.
    fn evaluate_unary_op(
        &self,
        op: &UnaryOp,
        rhs: &ConstantValue,
    ) -> Result<ConstantValue, HirError> {
        match op {
            UnaryOp::Neg => match rhs {
                ConstantValue::Number(n) => Ok(ConstantValue::Number(-n)),
                _ => Err(HirError::TypeError(format!(
                    "Negation requires number operand, got {:?}",
                    rhs
                ))),
            },
            UnaryOp::Not => match rhs {
                ConstantValue::Boolean(b) => Ok(ConstantValue::Boolean(!b)),
                _ => Err(HirError::TypeError(format!(
                    "Logical NOT requires boolean operand, got {:?}",
                    rhs
                ))),
            },
            UnaryOp::Increment | UnaryOp::Decrement => Err(HirError::TypeError(format!(
                "Increment/decrement operations are not allowed in constant expressions"
            ))),
        }
    }

    /// Check if an expression is a simple literal (possibly wrapped in groups)
    fn is_simple_literal(expr: &Expression) -> Option<&Literal> {
        match expr {
            Expression::Literal(lit) => Some(lit),
            Expression::Group(inner) => Self::is_simple_literal(inner),
            _ => None,
        }
    }

    /// Process a let statement with optional type annotation
    /// Also performs constant folding for compile-time evaluable expressions
    fn process_let_statement(
        &mut self,
        ctx: &crate::core::compileSession::CompileContext,
        identifier: String,
        type_annotation: Option<String>,
        expression: Expression,
    ) -> Result<HirStmt, HirError> {
        // Try to evaluate the expression at compile time (constant folding)
        let expr = if let Ok(constant_value) = self.compile_time_evaluate(&expression) {
            // Expression can be evaluated at compile time
            // For simple literals, keep them as direct HIR expressions (Number/String/Boolean)
            // Only use Constant for complex expressions (like 2+3 or const references)
            if let Some(literal) = Self::is_simple_literal(&expression) {
                // Simple literal - convert directly to HIR expression
                match literal {
                    Literal::Number(n) => HirExpression::Number(*n),
                    Literal::String(s) => HirExpression::String(s.clone()),
                    Literal::Boolean(b) => HirExpression::Boolean(*b),
                }
            } else {
                // Complex constant expression - intern it
                let const_id = self.intern_constant_value(constant_value);
                HirExpression::Constant(const_id)
            }
        } else {
            // Expression cannot be evaluated at compile time - process normally
            self.process_expression(ctx, expression)?
        };

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
        ctx: &crate::core::compileSession::CompileContext,
        identifier: String,
        expression: Expression,
    ) -> Result<HirStmt, HirError> {
        let (slot, expr) = self.process_assignment(ctx, identifier, expression, true)?;
        Ok(HirStmt::Assign { slot, value: expr })
    }

    /// Process an assign decrement statement
    fn process_assign_decrement_statement(
        &mut self,
        ctx: &crate::core::compileSession::CompileContext,
        identifier: String,
        expression: Expression,
    ) -> Result<HirStmt, HirError> {
        let (slot, expr) = self.process_assignment(ctx, identifier, expression, true)?;
        Ok(HirStmt::AssignDecrement { slot, value: expr })
    }

    /// Process an assign increment statement
    fn process_assign_increment_statement(
        &mut self,
        ctx: &crate::core::compileSession::CompileContext,
        identifier: String,
        expression: Expression,
    ) -> Result<HirStmt, HirError> {
        let (slot, expr) = self.process_assignment(ctx, identifier, expression, true)?;
        Ok(HirStmt::AssignIncrement { slot, value: expr })
    }

    /// Process an if statement
    fn process_if_statement(
        &mut self,
        ctx: &crate::core::compileSession::CompileContext,
        arms: Vec<(Expression, Block)>,
        else_block: Option<Block>,
    ) -> Result<HirStmt, HirError> {
        let mut hir_arms: Vec<(HirExpression, HirBlock)> = Vec::new();

        for (expression, block) in arms {
            // Process expression in current scope (before processing the block)
            let expr = self.process_expression(ctx, expression)?;
            // Process block (creates new scope, processes statements, restores scope)
            let bl = self.process_block(ctx, block)?;
            hir_arms.push((expr, bl));
        }

        // For the else_block, we fill an empty block if None.
        let hir_else_block = match else_block {
            Some(block) => Box::new(self.process_block(ctx, block)?),
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
        ctx: &crate::core::compileSession::CompileContext,
        expression: Expression,
        cases: Vec<(Option<Expression>, Block)>,
    ) -> Result<HirStmt, HirError> {
        // Process the expression being matched
        let match_expr = self.process_expression(ctx, expression)?;

        // Process each case
        let mut hir_cases = Vec::new();
        for (pattern, block) in cases {
            let pattern_expr = if let Some(pat) = pattern {
                Some(self.process_expression(ctx, pat)?)
            } else {
                None // Wildcard case
            };
            let hir_block = self.process_block(ctx, block)?;
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
        ctx: &crate::core::compileSession::CompileContext,
        identifier: String,
        arguments: Vec<Argument>,
        return_type: Option<String>,
        body: Block,
        pub_visibility: bool,
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

        // Register pub functions in the module registry
        if pub_visibility {
            if let Some(module_name) = &self.current_module {
                self.modules
                    .entry(module_name.clone())
                    .or_insert_with(|| Module {
                        functions: HashMap::new(),
                        constants: HashMap::new(),
                    })
                    .functions
                    .insert(identifier.clone(), func_id);
            }
        }

        // Process the function body (now that the function is registered, recursive calls will work)
        let func_body = self.process_block(ctx, body)?;

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
    fn process_return_statement(&mut self, ctx: &crate::core::compileSession::CompileContext, expression: Expression) -> Result<HirStmt, HirError> {
        let expr = self.process_expression(ctx, expression)?;
        Ok(HirStmt::Return { value: expr })
    }

    /// Process a while statement
    fn process_while_statement(
        &mut self,
        ctx: &crate::core::compileSession::CompileContext,
        condition: Expression,
        body: Block,
    ) -> Result<HirStmt, HirError> {
        // Transform while loop into: loop { if !condition { break; } body }
        let parent_scope = self.current_scope;
        let loop_scope_id = self.create_loop_scope();

        // Process condition
        let condition_expr = self.process_expression(ctx, condition)?;

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
        let hir_body = self.process_block(ctx, body)?;

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
        ctx: &crate::core::compileSession::CompileContext,
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
        let start_expr = self.process_expression(ctx, start)?;
        let end_expr = self.process_expression(ctx, end)?;

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
        let hir_body = self.process_block(ctx, body)?;

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
        ctx: &crate::core::compileSession::CompileContext,
        init_vars: Vec<(String, Expression)>,
        body: Block,
    ) -> Result<HirStmt, HirError> {
        let parent_scope = self.current_scope;
        let _loop_scope_id = self.create_loop_scope(); // Sets current_scope implicitly

        // Initialize loop variables in the loop scope
        let mut hir_init_vars = Vec::new();
        for (var_name, init_expr) in init_vars {
            let expr = self.process_expression(ctx, init_expr)?;
            let actual_kind = self.infer_variable_kind(&expr);
            let var_id = self.init_var(&var_name, actual_kind);
            hir_init_vars.push((var_id, expr));
        }

        // Process the loop body in the loop scope
        let hir_body = self.process_block(ctx, body)?;

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
        ctx: &crate::core::compileSession::CompileContext,
        expression: Option<Expression>,
    ) -> Result<HirStmt, HirError> {
        let expr = if let Some(expr) = expression {
            Some(self.process_expression(ctx, expr)?)
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
        ctx: &crate::core::compileSession::CompileContext,
        expression: Expression,
    ) -> Result<HirStmt, HirError> {
        let expr = self.process_expression(ctx, expression)?;
        Ok(HirStmt::Expression(expr))
    }

    fn process_statement(&mut self, ctx: &crate::core::compileSession::CompileContext, statement: Statement) -> Result<HirStmt, HirError> {
        match statement {
            Statement::Mod { identifier: _ } => {
                // Module context is already set by the engine before build_append()
                // mod declarations do not participate in HIR lowering
                Ok(HirStmt::Nop)
            }
            Statement::Let {
                identifier,
                type_annotation,
                expression,
                pub_visibility: _,
            } => self.process_let_statement(ctx, identifier, type_annotation, expression),
            Statement::Const {
                identifier,
                expression,
                pub_visibility,
            } => {
                // Constants must be compile-time evaluable
                self.process_const_statement(identifier, expression, pub_visibility)
            }
            Statement::Assign {
                identifier,
                expression,
            } => self.process_assign_statement(ctx, identifier, expression),
            Statement::AssignDecrement {
                identifier,
                expression,
            } => self.process_assign_decrement_statement(ctx, identifier, expression),
            Statement::AssignIncrement {
                identifier,
                expression,
            } => self.process_assign_increment_statement(ctx, identifier, expression),
            Statement::If { arms, else_block } => self.process_if_statement(ctx, arms, else_block),
            Statement::Match { expression, cases } => {
                self.process_match_statement(ctx, expression, cases)
            }
            Statement::FunctionDeclaration {
                identifier,
                arguments,
                return_type,
                body,
                pub_visibility,
            } => self.process_function_declaration_statement(
                ctx,
                identifier,
                arguments,
                return_type,
                body,
                pub_visibility,
            ),
            Statement::Return { expression } => self.process_return_statement(ctx, expression),
            Statement::While { condition, body } => self.process_while_statement(ctx, condition, body),
            Statement::For {
                var_name,
                start,
                end,
                body,
            } => self.process_for_statement(ctx, var_name, start, end, body),
            Statement::Loop { init_vars, body } => self.process_loop_statement(ctx, init_vars, body),
            Statement::Break { expression } => self.process_break_statement(ctx, expression),
            Statement::Continue => self.process_continue_statement(),
            Statement::Expression(expression) => self.process_expression_statement(ctx, expression),
            Statement::Use { path, selector } => {
                // Ensure we're inside a module
                if self.current_module.is_none() {
                    // If no module declared yet, treat as an implicit default module
                    // This allows standalone files to work
                    self.current_module = Some("__main__".to_string());
                    self.module_imports
                        .insert("__main__".to_string(), HashMap::new());
                    self.ast
                        .module_imports
                        .insert("__main__".to_string(), HashMap::new());
                }

                let imports = self.resolve_import(&path, &selector)?;

                for (name, id) in imports {
                    // Check for duplicate imports within THIS module only
                    if let Some(existing_imports) = self.get_current_imports() {
                        if existing_imports.contains_key(&name) {
                            return Err(HirError::TypeError(format!(
                                "Symbol '{}' already imported in module '{}'",
                                name,
                                self.current_module.as_ref().unwrap()
                            )));
                        }
                    }

                    // No constant copying, no searching, no reconstruction needed
                    // The ID from resolve_import is already a constant ID (for constants)
                    // or function ID (for functions) - both are valid import IDs
                    self.add_import_to_current_module(name, id)?;
                }

                Ok(HirStmt::Nop)
            }
        }
    }

    /// Process arguments list for PostfixInvoke
    fn process_invoke_args(
        &mut self,
        ctx: &crate::core::compileSession::CompileContext,
        args: Option<Vec<Expression>>,
    ) -> Result<Vec<HirExpression>, HirError> {
        if let Some(arg_list) = args {
            let mut processed_args = Vec::new();
            for arg in arg_list {
                processed_args.push(self.process_expression(ctx, arg)?);
            }
            Ok(processed_args)
        } else {
            Ok(Vec::new())
        }
    }

    /// Process PostfixInvoke when lhs is an Identifier
    fn process_identifier_invoke(
        &mut self,
        ctx: &crate::core::compileSession::CompileContext,
        identifier: String,
        args: Option<Vec<Expression>>,
    ) -> Result<HirExpression, HirError> {
        // Try to resolve as imported symbol first (module-scoped)
        if let Some(imported_id) = self.resolve_import_in_current_module(&identifier) {
            // Check if it's a variable (thunk)
            if self
                .ast
                .scopes
                .scopes
                .iter()
                .any(|scope| scope.vars.iter().any(|v| v.id == imported_id))
            {
                let processed_args = self.process_invoke_args(ctx, args)?;
                return Ok(HirExpression::PostfixInvoke {
                    operand: Box::new(HirExpression::Identifier(imported_id)),
                    args: if processed_args.is_empty() {
                        None
                    } else {
                        Some(processed_args)
                    },
                });
            }
        }

        // Try to resolve as variable (for thunks stored in variables)
        if let Some(var_id) = self.resolve_var_aggressive(&identifier) {
            let processed_args = self.process_invoke_args(ctx, args)?;
            return Ok(HirExpression::PostfixInvoke {
                operand: Box::new(HirExpression::Identifier(var_id)),
                args: if processed_args.is_empty() {
                    None
                } else {
                    Some(processed_args)
                },
            });
        }

        // Check if it's a function (should error - can't use ! on functions)
        if self.resolve_function(ctx, &identifier).is_some() {
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
        ctx: &crate::core::compileSession::CompileContext,
        callee: Box<Expression>,
        fc_args: Vec<Expression>,
        invoke_args: Option<Vec<Expression>>,
    ) -> Result<HirExpression, HirError> {
        let func_expr = self.process_expression(ctx, Expression::FunctionCall {
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
                processed_args.extend(self.process_invoke_args(ctx, invoke_args)?);
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
                processed_args.extend(self.process_invoke_args(ctx, invoke_args)?);
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
                let processed_args = self.process_invoke_args(ctx, invoke_args)?;
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
        ctx: &crate::core::compileSession::CompileContext,
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
        processed_args.extend(self.process_invoke_args(ctx, invoke_args)?);

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
    fn process_literal_expression(&mut self, lit: Literal) -> Result<HirExpression, HirError> {
        return match lit {
            Literal::String(s) => Ok(HirExpression::String(s.clone())),
            Literal::Number(n) => Ok(HirExpression::Number(n)),
            Literal::Boolean(b) => Ok(HirExpression::Boolean(b)),
        };

        // let cid = self.intern_constant(lit);
        // Ok(HirExpression::Constant(cid))
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
                return Err(HirError::TypeError(format!(
                    "Member access object must be an identifier, got: {:?}",
                    object
                )));
            }
        };

        // Look up the module
        let module = self
            .modules
            .get(&module_name)
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
            // It's a constant - return the constant ID directly
            // The constant ID is stored in modules.constants (not variable slot ID)
            Ok(HirExpression::Constant(*constant_id))
        } else {
            // TODO: Also check variables
            Err(HirError::TypeError(format!(
                "Member '{}' not found in module '{}'",
                member, module_name
            )))
        }
    }

    /// Process an identifier expression
    fn process_identifier_expression(
        &mut self,
        ctx: &crate::core::compileSession::CompileContext,
        identifier: String,
    ) -> Result<HirExpression, HirError> {
        // First check imported symbols (compile-time resolved)
        if let Some(imported_id) = self.resolve_import_in_current_module(&identifier) {
            // Check if this is a constant ID or function ID
            // Constants are stored in hir_ast.constants, functions are in hir_ast.functions
            if self.ast.constants.iter().any(|c| c.id == imported_id) {
                // It's a constant - return as constant expression
                return Ok(HirExpression::Constant(imported_id));
            } else {
                // It's a function - convert to thunk by calling with no args
                return Ok(HirExpression::FunctionCall {
                    function_id: imported_id,
                    args: Vec::new(),
                    invoke: false,
                });
            }
        }

        if let Some(slot) = self.resolve_var(&identifier) {
            return Ok(HirExpression::Identifier(slot));
        }
        if let Some(const_id) = self.resolve_const(&identifier) {
            return Ok(HirExpression::Constant(const_id));
        }

        if let Some(function_id) = self.resolve_function(ctx, &identifier) {
            // Function name used as identifier - convert to thunk by calling with no args
            // This allows functions to be used in compositions like: square <| add10
            return Ok(HirExpression::FunctionCall {
                function_id,
                args: Vec::new(),
                invoke: false,
            });
        } else {
            Err(HirError::UnknownVariable(identifier))
        }
    }

    /// Process an infix (binary) expression
    fn process_infix_expression(
        &mut self,
        ctx: &crate::core::compileSession::CompileContext,
        lhs: Expression,
        op: BinaryOp,
        rhs: Expression,
    ) -> Result<HirExpression, HirError> {
        let lhs_expr = self.process_expression(ctx, lhs)?;
        let rhs_expr = self.process_expression(ctx, rhs)?;

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
        ctx: &crate::core::compileSession::CompileContext,
        op: UnaryOp,
        rhs: Expression,
    ) -> Result<HirExpression, HirError> {
        let rhs_expr = self.process_expression(ctx, rhs)?;

        Ok(HirExpression::Unary {
            operand: Box::new(rhs_expr),
            operator: op,
        })
    }

    /// Process a postfix expression
    fn process_postfix_expression(
        &mut self,
        ctx: &crate::core::compileSession::CompileContext,
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
                    return self.process_function_call_invoke(ctx, callee, fc_args, args);
                }

                // If lhs is an Identifier, check if it's a variable first
                if let Expression::Identifier(identifier) = lhs {
                    return self.process_identifier_invoke(ctx, identifier, args);
                }

                // For other expressions, process normally
                let lhs_expr = self.process_expression(ctx, lhs)?;
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
                    } => self.process_nested_postfix_invoke(ctx, operand, existing_args, args),
                    other => {
                        let processed_args = self.process_invoke_args(ctx, args)?;
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
        ctx: &crate::core::compileSession::CompileContext,
        callee: Expression,
        arguments: Vec<Expression>,
    ) -> Result<HirExpression, HirError> {
        // Identifier callee: MUST be a function
        if let Expression::Identifier(identifier_name) = callee {
            let function_id = self.resolve_function(ctx, &identifier_name).ok_or_else(|| {
                HirError::TypeError(format!("'{}' is not a callable function", identifier_name))
            })?;

            let args = arguments
                .into_iter()
                .map(|arg| self.process_expression(ctx, arg))
                .collect::<Result<Vec<_>, _>>()?;

            return Ok(HirExpression::FunctionCall {
                function_id,
                args,
                invoke: false,
            });
        }

        // Non-identifier callees (thunks, compositions, etc.)
        let callee_expr = self.process_expression(ctx, callee)?;

        let processed_args = arguments
            .into_iter()
            .map(|arg| self.process_expression(ctx, arg))
            .collect::<Result<Vec<_>, _>>()?;

        match callee_expr {
            HirExpression::ComposeThunk { first, second } => Ok(HirExpression::PostfixInvoke {
                operand: Box::new(HirExpression::ComposeThunk { first, second }),
                args: Some(processed_args),
            }),
            HirExpression::FunctionCall {
                function_id,
                args: existing_args,
                invoke: false,
            } => {
                let mut combined = existing_args;
                combined.extend(processed_args);
                Ok(HirExpression::FunctionCall {
                    function_id,
                    args: combined,
                    invoke: false,
                })
            }
            _ => Ok(HirExpression::PostfixInvoke {
                operand: Box::new(callee_expr),
                args: Some(processed_args),
            }),
        }
    }

    /// Process a partial call expression
    fn process_partial_call_expression(
        &mut self,
        ctx: &crate::core::compileSession::CompileContext,
        func: Expression,
        args: Vec<CallArgument>,
    ) -> Result<HirExpression, HirError> {
        // Process the function identifier
        let func_id = if let Expression::Identifier(ref identifier_name) = func {
            // Check if it's a function
            if let Some(function_id) = self.resolve_function(ctx, identifier_name) {
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
                    bound.push(Some(self.process_expression(ctx, expr)?));
                }
            }
        }

        Ok(HirExpression::PartialCall { func_id, bound })
    }

    /// Process a group expression (parentheses)
    fn process_group_expression(&mut self, ctx: &crate::core::compileSession::CompileContext, expr: Expression) -> Result<HirExpression, HirError> {
        // Group expressions (parentheses) just unwrap and process the inner expression
        self.process_expression(ctx, expr)
    }

    /// Process an array expression
    fn process_array_expression(
        &mut self,
        ctx: &crate::core::compileSession::CompileContext,
        elements: Vec<Expression>,
    ) -> Result<HirExpression, HirError> {
        let mut hir_elements = Vec::new();
        for elem in elements {
            hir_elements.push(self.process_expression(ctx, elem)?);
        }
        Ok(HirExpression::Array(hir_elements))
    }

    /// Process an array index expression
    /// Supports single index (arr[3]) and ranges/slices (arr[1..5], arr[1..=5], etc.)
    /// Multi-dimensional indexing not yet supported
    fn process_array_index_expression(
        &mut self,
        ctx: &crate::core::compileSession::CompileContext,
        array: Expression,
        indices: Vec<crate::core::ast::IndexSpec>,
    ) -> Result<HirExpression, HirError> {
        // For now, only support single dimension (one IndexSpec)
        if indices.len() != 1 {
            return Err(HirError::TypeError(format!(
                "Multi-dimensional indexing not yet supported (got {} indices)",
                indices.len()
            )));
        }

        let index_spec = &indices[0];
        let array_expr = self.process_expression(ctx, array)?;

        match index_spec {
            crate::core::ast::IndexSpec::Single(index_expr) => {
                // Single index: arr[3]
                let index_hir_expr = self.process_expression(ctx, index_expr.clone())?;
                Ok(HirExpression::ArrayIndex {
                    array: Box::new(array_expr),
                    index: Box::new(index_hir_expr),
                })
            }
            crate::core::ast::IndexSpec::Range { start, end, step } => {
                // Range: arr[1..5] or arr[1..5..2] or arr[..5] or arr[5..]
                let start_hir = if let Some(start_expr) = start {
                    Some(Box::new(self.process_expression(ctx, start_expr.clone())?))
                } else {
                    None
                };
                let end_hir = if let Some(end_expr) = end {
                    Some(Box::new(self.process_expression(ctx, end_expr.clone())?))
                } else {
                    None
                };
                let step_hir = if let Some(step_expr) = step {
                    Some(Box::new(self.process_expression(ctx, step_expr.clone())?))
                } else {
                    None
                };
                Ok(HirExpression::ArraySlice {
                    array: Box::new(array_expr),
                    start: start_hir,
                    end: end_hir,
                    step: step_hir,
                    inclusive_end: false, // Range is exclusive end
                })
            }
            crate::core::ast::IndexSpec::InclusiveRange { start, end } => {
                // Inclusive range: arr[1..=5]
                let start_hir = if let Some(start_expr) = start {
                    Some(Box::new(self.process_expression(ctx, start_expr.clone())?))
                } else {
                    None
                };
                let end_hir = if let Some(end_expr) = end {
                    Some(Box::new(self.process_expression(ctx, end_expr.clone())?))
                } else {
                    None
                };
                Ok(HirExpression::ArraySlice {
                    array: Box::new(array_expr),
                    start: start_hir,
                    end: end_hir,
                    step: None,          // Inclusive range doesn't support step
                    inclusive_end: true, // Inclusive range has inclusive end
                })
            }
        }
    }

    /// Process a compose expression
    /// Detects reducer patterns: array |> reducer (e.g., xs |> sum, xs |> fold(...))
    fn process_compose_expression(
        &mut self,
        ctx: &crate::core::compileSession::CompileContext,
        lhs: Expression,
        rhs: Expression,
        reverse: bool,
    ) -> Result<HirExpression, HirError> {
        // Check if this is a reducer pattern BEFORE processing expressions
        // This allows us to detect array literals directly from the AST
        // Only for forward pipe (|>), not reverse (<|)
        if !reverse {
            // Check if LHS is an array literal in the AST
            let is_array_literal = matches!(lhs, Expression::Array(_));

            // Process both sides
            let first_expr = self.process_expression(ctx, lhs)?;
            let second_expr = self.process_expression(ctx, rhs)?;

            // Check if LHS is an array (either array literal or array variable)
            let is_array = is_array_literal
                || match &first_expr {
                    HirExpression::Array(_) => true,
                    HirExpression::Identifier(var_id) => {
                        // Check if variable is of array type
                        if let Some(var_kind) = self.get_var_kind_from_id(*var_id) {
                            matches!(var_kind, ValueKind::Array(_))
                        } else {
                            // If we can't get the type, infer it from the expression
                            // This handles cases where the variable type hasn't been set yet
                            let inferred = self.infer_variable_kind(&first_expr);
                            matches!(inferred, ValueKind::Array(_))
                        }
                    }
                    _ => {
                        // For other expressions, infer the type
                        let inferred = self.infer_variable_kind(&first_expr);
                        matches!(inferred, ValueKind::Array(_))
                    }
                };

            if is_array {
                // Check if RHS is a reducer function (sum or fold)
                let reducer_info = self.detect_reducer(&second_expr)?;

                if let Some((reducer_type, reducer_args)) = reducer_info {
                    // This is a reducer pattern - return a special reducer expression
                    // We'll lower this to bytecode that emits an internal loop
                    return Ok(HirExpression::Reducer {
                        array: Box::new(first_expr),
                        reducer_type,
                        reducer_args,
                    });
                }
            }

            // If not a reducer, treat as normal composition
            // f |> g means g(f(x)), so process f first, then g
            Ok(HirExpression::ComposeThunk {
                first: Box::new(first_expr),
                second: Box::new(second_expr),
            })
        } else {
            // For reverse composition, process both sides normally
            let first_expr = self.process_expression(ctx, lhs)?;
            let second_expr = self.process_expression(ctx, rhs)?;

            // For reverse composition (<|), swap the operands
            // f <| g means f(g(x)), so we want to process g first, then f
            Ok(HirExpression::ComposeThunk {
                first: Box::new(second_expr),
                second: Box::new(first_expr),
            })
        }
    }

    /// Detect if an expression is a reducer function (sum or fold)
    /// Returns (reducer_type, args) if it's a reducer, None otherwise
    fn detect_reducer(
        &self,
        expr: &HirExpression,
    ) -> Result<Option<(ReducerType, Vec<HirExpression>)>, HirError> {
        match expr {
            HirExpression::FunctionCall {
                function_id, args, ..
            } => {
                // Check if this is a reducer function by name
                // First try to get the function by ID
                let func_name = if let Some(func) = self.ast.functions.get(function_id) {
                    Some(func.name.as_str())
                } else {
                    // Fallback 1: search all functions to find one with this ID
                    // This handles cases where the function might be registered differently
                    if let Some(name) = self
                        .ast
                        .functions
                        .iter()
                        .find(|(id, _)| *id == function_id)
                        .map(|(_, func)| func.name.as_str())
                    {
                        Some(name)
                    } else {
                        // Search current module's imports
                        self.get_current_imports().and_then(|imports| {
                            imports
                                .iter()
                                .find(|(_, &imported_id)| imported_id == *function_id)
                                .map(|(name, _)| name.as_str())
                        })
                    }
                };

                if let Some(name) = func_name {
                    // Check if the name is a reducer (strip module prefix if present)
                    let base_name = name.split('.').last().unwrap_or(name);
                    match base_name {
                        "sum" => {
                            // sum is a reducer with no args (or any number of args when used as identifier)
                            Ok(Some((ReducerType::Sum, Vec::new())))
                        }
                        "fold" => {
                            // fold(init, fn) is a reducer with 2 args
                            if args.len() == 2 {
                                Ok(Some((ReducerType::Fold, args.clone())))
                            } else {
                                Ok(None)
                            }
                        }
                        _ => Ok(None),
                    }
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
    }

    /// Process a loop expression
    fn process_loop_expression(
        &mut self,
        ctx: &crate::core::compileSession::CompileContext,
        init_vars: Vec<(String, Expression)>,
        body: Block,
    ) -> Result<HirExpression, HirError> {
        let parent_scope = self.current_scope;
        let _loop_scope_id = self.create_loop_scope(); // Sets current_scope implicitly

        // Initialize loop variables in the loop scope
        let mut hir_init_vars = Vec::new();
        for (var_name, init_expr) in init_vars {
            let expr = self.process_expression(ctx, init_expr)?;
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
        let hir_body = self.process_block(ctx, body)?;

        self.restore_scope(parent_scope);

        Ok(HirExpression::Loop {
            init_vars: hir_init_vars,
            body: hir_body,
            break_slot,
        })
    }

    fn process_expression(&mut self, ctx: &crate::core::compileSession::CompileContext, expression: Expression) -> Result<HirExpression, HirError> {
        match expression {
            Expression::Literal(lit) => self.process_literal_expression(lit),
            Expression::Array(elements) => self.process_array_expression(ctx, elements),
            Expression::ArrayIndex { array, indices } => {
                self.process_array_index_expression(ctx, *array, indices)
            }
            Expression::Identifier(identifier) => self.process_identifier_expression(ctx, identifier),
            Expression::MemberAccess { object, member } => {
                self.process_member_access_expression(*object, member)
            }
            Expression::Infix { lhs, op, rhs } => self.process_infix_expression(ctx, *lhs, op, *rhs),
            Expression::Prefix { op, rhs } => self.process_prefix_expression(ctx, op, *rhs),
            Expression::Postfix { lhs, op, args } => {
                self.process_postfix_expression(ctx, *lhs, op, args)
            }
            Expression::FunctionCall { callee, arguments } => {
                self.process_function_call_expression(ctx, *callee, arguments)
            }
            Expression::PartialCall { func, args } => {
                self.process_partial_call_expression(ctx, *func, args)
            }
            Expression::Group(expr) => self.process_group_expression(ctx, *expr),
            Expression::Compose { lhs, rhs, reverse } => {
                self.process_compose_expression(ctx, *lhs, *rhs, reverse)
            }
            Expression::Loop { init_vars, body } => self.process_loop_expression(ctx, init_vars, body),
        }
    }
}
