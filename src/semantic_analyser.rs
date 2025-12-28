use std::collections::HashMap;

use crate::ast::{BinaryOp, Block, Expression, Literal, Program, Statement, UnaryOp, PostfixOp};

/// Function signature describing parameter types and return type.
/// 
/// Used for type checking function calls and declarations.
#[derive(Debug, Clone)]
pub struct FunctionSignature {
    #[allow(dead_code)]
    pub params: Vec<ValueKind>,
    #[allow(dead_code)]
    pub return_type: Box<ValueKind>,
}

/// Represents the type of a value in the CantaLoop type system.
/// 
/// Includes primitive types (Number, String, Boolean) and
/// function/thunk types with their signatures.
#[derive(Debug, Clone, PartialEq)]
pub enum ValueKind {
    Number, 
    String,
    Boolean, 
    Unknown,
    Void,
    // Function type: stores the full type string like "num -> num" or "(num, num) -> num"
    Function(String),
    // Thunk type: stores the full type string like "num ~> num" or "(num, num) ~> num"
    Thunk(String),
}

#[derive(Debug, Clone)]
pub enum ConstantValue {
    Number(f64),
    String(String),
    Boolean(bool),
    #[allow(dead_code)]
    None,
}

#[derive(Debug)]
pub struct Constant {
    pub id: u32,
    pub name: String,
    pub value: ConstantValue,
    pub kind: ValueKind,
}

#[derive(Debug, Clone)]
pub struct Variable {
    pub id: u32,
    pub name: String,
    pub kind: ValueKind,
}

#[derive(Debug, Clone)]
pub struct FunctionDefinition {
    pub body: HirBlock,
    pub param_var_ids: Vec<u32>, // Variable IDs for parameters, in order
    #[allow(dead_code)]
    pub scope_id: ScopeId,       // The function's scope ID
}

#[derive(Debug, Clone)]
pub struct Function {
    #[allow(dead_code)]
    pub id: u32,
    pub name: String,
    pub signature: FunctionSignature,
    pub definition: FunctionDefinition,
}

/// High-level Intermediate Representation (HIR) of a CantaLoop program.
/// 
/// HIR is the typed representation after semantic analysis, containing:
/// - Typed expressions and statements
/// - Variable slot assignments
/// - Function definitions with resolved types
/// - Constant table
/// 
/// This is the input to the bytecode compiler.
#[derive(Debug)]
pub struct HirAst {
    pub constants: Vec<Constant>,
    pub blocks: Vec<HirBlock>,
    pub scopes: ScopeArena,
    pub functions: std::collections::HashMap<u32, Function>, // Function ID -> Function struct
}

impl HirAst {
    /// Get the function signature for a thunk variable to determine total args needed
    /// This helps determine if a thunk will be fully applied
    pub fn get_thunk_function_info(&self, var_id: u32) -> Option<(u32, usize)> {
        // Try to find the original function by looking at the variable's assigned expression
        if let Some(expr) = self.get_var_assigned_expression(var_id) {
            if let HirExpression::FunctionCall { function_id, args: _, .. } = expr {
                // This is a function call that created the thunk
                if let Some(func) = self.functions.get(function_id) {
                    let total_params = func.signature.params.len();
                    return Some((*function_id, total_params));
                }
            }
        }
        None
    }
    
    /// Find the expression assigned to a variable (for LSP queries to detect thunks)
    pub fn get_var_assigned_expression(&self, var_id: u32) -> Option<&HirExpression> {
        // Search through all blocks and statements to find the assignment
        for block in &self.blocks {
            for stmt in &block.statements {
                if let HirStmt::Assign { slot, value } = stmt {
                    if *slot == var_id {
                        return Some(value);
                    }
                }
            }
        }
        None
    }
}

#[derive(Debug, Clone)]
pub struct HirBlock {
    #[allow(dead_code)]
    pub scope: ScopeId,
    pub statements: Vec<HirStmt>,
}

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
        else_block: Box<HirBlock>
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
    Expression(HirExpression)
}

#[derive(Debug, Clone)]
pub enum HirExpression {
    #[allow(dead_code)]
    Number(f64),
    #[allow(dead_code)]
    String(String),
    Identifier(u32),
    Constant(u32),
    Binary {
        lhs: Box<HirExpression>,
        rhs: Box<HirExpression>,
        operator: BinaryOp,
    },
    Unary {
        operand: Box<HirExpression>,
        operator: UnaryOp,
    },
    PostfixInvoke {
        operand: Box<HirExpression>, // Expression that should be invoked (typically a PreparedCall)
        args: Option<Vec<HirExpression>>, // Optional additional arguments for currying: add5!(10)
    },
    FunctionCall {
        function_id: u32, // Function ID from functions registry
        args: Vec<HirExpression>,
        invoke: bool, // true if should invoke immediately (!), false if just prepare
    },
    Loop {
        init_vars: Vec<(u32, HirExpression)>, // (variable_id, initial_value) for loop initialization variables
        body: HirBlock,
        break_slot: Option<u32>, // Variable slot for break value (None for statement loops, Some(slot) for expression loops)
    },
    ComposeThunk {
        first: Box<HirExpression>,
        second: Box<HirExpression>,
    },
}

pub type ScopeId = usize;

#[derive(Debug)]
pub struct ScopeArena {
    pub scopes: Vec<HirBlockContext>,
}

#[derive(Debug)]
pub struct HirBlockContext {
    pub vars: Vec<Variable>,
    pub parent: Option<ScopeId>,
}

#[derive(Debug)]
pub enum HirError {
    #[allow(dead_code)]
    NotImplemented,
    UnknownVariable(String),
    VariableAlreadyDeclared(String),
    TypeMismatch { variable: String, expected: ValueKind, actual: ValueKind },
    TypeError(String),
    BinaryOpTypeError { 
        operator: String, 
        lhs_type: ValueKind, 
        rhs_type: ValueKind,
        expected: String,
    },
    // You can add more specific error variants as needed
}

// Hashable key for constant deduplication
#[derive(Hash, PartialEq, Eq, Clone)]
enum ConstantKey {
    Number(u64),  // f64 bit representation
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

pub struct HirBuilder {
    pub ast: HirAst,
    current_scope: ScopeId,
    next_var_id: u32,
    next_function_id: u32,
    constant_map: HashMap<ConstantKey, u32>, // Maps constant value to constant ID
}

impl HirBuilder {
    pub fn new() -> Self {
        let mut scopes = ScopeArena { scopes: Vec::new() };

        let root = scopes.scopes.len();
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
            },
            current_scope: root,
            next_var_id: 0,
            next_function_id: 0,
            constant_map: HashMap::new(),
        }
    }

    pub fn resolve_var(&self, name: &str) -> Option<u32> {
        let mut scope = Some(self.current_scope);

        while let Some(id) = scope {
            let ctx = &self.ast.scopes.scopes[id];
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
        for scope_id in 0..self.ast.scopes.scopes.len() {
            let ctx = &self.ast.scopes.scopes[scope_id];
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
            let ctx = &self.ast.scopes.scopes[scope_id];
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
        for scope_id in 0..self.ast.scopes.scopes.len() {
            let ctx = &self.ast.scopes.scopes[scope_id];
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
        let ctx = &self.ast.scopes.scopes[self.current_scope];
        ctx.vars.iter().any(|v| v.name == name)
    }

    pub fn init_var(&mut self, name: &str, kind: ValueKind) -> u32 {
        let id = self.next_var_id;
        self.next_var_id += 1;

        let ctx = &mut self.ast.scopes.scopes[self.current_scope];
        ctx.vars.push(Variable {
            id,
            name: name.to_string(),
            kind,
        });

        id
    }

    fn infer_variable_kind(&self, expr: &HirExpression) -> ValueKind {
        match expr {
            HirExpression::Number(_) => ValueKind::Number,
            HirExpression::String(_) => ValueKind::String,
            HirExpression::Constant(id) => {
                // Look up constant's kind
                if let Some(c) = self.ast.constants.iter().find(|c| c.id == *id) {
                    c.kind.clone()
                } else {
                    ValueKind::Unknown
                }
            }
            HirExpression::Identifier(id) => {
                // Look up the variable's kind
                for scope in &self.ast.scopes.scopes {
                    if let Some(v) = scope.vars.iter().find(|v| v.id == *id) {
                        return v.kind.clone();
                    }
                }
                ValueKind::Unknown
            }
            HirExpression::Binary { operator, lhs, rhs } => {
                match operator {
                    BinaryOp::And | BinaryOp::Or => ValueKind::Boolean,
                    BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Gt | BinaryOp::Lt | BinaryOp::Ge | BinaryOp::Le => ValueKind::Boolean,
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
            HirExpression::Unary { .. } => ValueKind::Number, // Unary ops typically produce numbers
            HirExpression::FunctionCall { function_id, invoke, args: _, .. } => {
                if let Some(func) = self.ast.functions.get(function_id) {
                    if !invoke {
                        // This is a thunk (prepared call) - return a Thunk type
                        // For fully applied thunks, we use "return_type ~> return_type" format
                        // This follows the grammar: atom_type ~> atom_type
                        let return_type_str = Self::format_value_kind_for_type(&func.signature.return_type);
                        // Create thunk type: "return_type ~> return_type"
                        // This represents a thunk that when invoked returns return_type
                        // If return type is Unknown, use "unknown ~> unknown"
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
            HirExpression::PostfixInvoke { operand, args: _ } => {
                // Invoking a prepared call - result type is the same as the function's return type
                // The operand should be a thunk (FunctionCall with invoke=false) or a variable containing a thunk
                // For variables, we look up their type which should be the function's return type
                match operand.as_ref() {
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
                            ValueKind::Thunk(type_str) => {
                                // Extract the return type from the thunk type string
                                // Try parsing first
                                if let Some((_, return_type, _)) = Self::parse_callable_type(type_str) {
                                    let trimmed_return = return_type.trim();
                                    Self::parse_type_string_static(trimmed_return)
                                } else if let Some(pos) = type_str.find("~>") {
                                    // Fallback: extract type directly from "X ~> Y" format
                                    let after_arrow = type_str[pos + 2..].trim();
                                    Self::parse_type_string_static(after_arrow)
                                } else {
                                    ValueKind::Unknown
                                }
                            }
                            ValueKind::Function(type_str) => {
                                // Extract the return type from the function type string
                                if let Some((_, return_type, _)) = Self::parse_callable_type(type_str) {
                                    let trimmed_return = return_type.trim();
                                    Self::parse_type_string_static(trimmed_return)
                                } else {
                                    ValueKind::Unknown
                                }
                            }
                            _ => {
                                // If it's not a thunk/function type, assume it's already the result type
                                var_kind
                            }
                        }
                    }
                    HirExpression::PostfixInvoke { .. } => {
                        // Nested invocation: first infer the inner invocation's type
                        // The inner invocation should return a thunk, which we then invoke
                        let inner_type = self.infer_variable_kind(&operand);
                        match &inner_type {
                            ValueKind::Thunk(type_str) => {
                                // Extract the return type from the thunk type string
                                // For fully applied thunks, format is "return_type ~> return_type"
                                if let Some((_, return_type, _)) = Self::parse_callable_type(type_str) {
                                    let trimmed_return = return_type.trim();
                                    let parsed = Self::parse_type_string_static(trimmed_return);
                                    if matches!(parsed, ValueKind::Unknown) {
                                        // Fallback: if parsing failed, try to extract the type directly
                                        // For "X ~> X" format, we can extract X directly
                                        if let Some(pos) = type_str.find("~>") {
                                            let before = type_str[..pos].trim();
                                            let after = type_str[pos + 2..].trim();
                                            if before == after {
                                                // It's "X ~> X" format, use X
                                                Self::parse_type_string_static(before)
                                            } else {
                                                parsed
                                            }
                                        } else {
                                            parsed
                                        }
                                    } else {
                                        parsed
                                    }
                                } else {
                                    ValueKind::Unknown
                                }
                            }
                            ValueKind::Function(type_str) => {
                                // Extract the return type from the function type string
                                if let Some((_, return_type, _)) = Self::parse_callable_type(type_str) {
                                    let trimmed_return = return_type.trim();
                                    Self::parse_type_string_static(trimmed_return)
                                } else {
                                    ValueKind::Unknown
                                }
                            }
                            _ => {
                                // If it's not a thunk/function type, assume it's already the result type
                                inner_type
                            }
                        }
                    }
                    _ => {
                        // Fallback: try to infer from operand
                        let operand_type = self.infer_variable_kind(&operand);
                        match &operand_type {
                            ValueKind::Thunk(type_str) => {
                                // Extract the return type from the thunk type string
                                // For fully applied thunks, format is "return_type ~> return_type"
                                if let Some((_, return_type, _)) = Self::parse_callable_type(type_str) {
                                    let trimmed_return = return_type.trim();
                                    let parsed = Self::parse_type_string_static(trimmed_return);
                                    if matches!(parsed, ValueKind::Unknown) {
                                        // Fallback: if parsing failed, try to extract the type directly
                                        // For "X ~> X" format, we can extract X directly
                                        if let Some(pos) = type_str.find("~>") {
                                            let before = type_str[..pos].trim();
                                            let after = type_str[pos + 2..].trim();
                                            if before == after {
                                                // It's "X ~> X" format, use X
                                                Self::parse_type_string_static(before)
                                            } else {
                                                parsed
                                            }
                                        } else {
                                            parsed
                                        }
                                    } else {
                                        parsed
                                    }
                                } else {
                                    ValueKind::Unknown
                                }
                            }
                            ValueKind::Function(type_str) => {
                                // Extract the return type from the function type string
                                if let Some((_, return_type, _)) = Self::parse_callable_type(type_str) {
                                    let trimmed_return = return_type.trim();
                                    Self::parse_type_string_static(trimmed_return)
                                } else {
                                    ValueKind::Unknown
                                }
                            }
                            _ => operand_type
                        }
                    }
                }
            }
            HirExpression::ComposeThunk { first: _, second: _ } => {
                // Composition produces a thunk type
                // For now, return Unknown - could be enhanced to infer types
                // The result type would be the return type of second, taking the input type of first
                ValueKind::Unknown
            }
            HirExpression::Loop { break_slot, .. } => {
                // Loop expression returns the type of the break value
                // For now, we can't infer it statically, so return Unknown
                // TODO: Could infer from break statements in the loop body
                if break_slot.is_some() {
                    // Expression-valued loop - type depends on break value
                    ValueKind::Unknown
                } else {
                    // Statement loop (shouldn't happen in expression context)
                    ValueKind::Unknown
                }
            }
        }
    }

    #[allow(dead_code)]
    pub fn init_const(&mut self, name: &str, value: ConstantValue) -> u32 {
        let id = self.next_var_id;
        self.next_var_id += 1;
        
        let kind = match &value {
            ConstantValue::Number(_) => ValueKind::Number,
            ConstantValue::String(_) => ValueKind::String,
            ConstantValue::Boolean(_) => ValueKind::Boolean,
            ConstantValue::None => ValueKind::Unknown,
        };

        self.ast.constants.push(Constant {
            id,
            name: name.to_string(),
            kind,
            value
        });

        id
    }

    #[allow(dead_code)]
    pub fn init_const_by_kind(&mut self, name: &str, kind: ValueKind) -> u32 {
        let id = self.ast.constants.len() as u32;

        self.ast.constants.push(Constant {
            id,
            name: name.to_string(),
            kind,
            value: ConstantValue::None,
        });

        id
    }

    fn resolve_const(&self, name: &str) -> Option<u32> {
        // Only resolve data constants (not functions)
        self.ast.constants.iter()
            .rev()
            .find(|c| c.name == name)
            .map(|c| c.id)
    }

    pub fn resolve_function(&self, name: &str) -> Option<u32> {
        // Look up function by name in the functions registry
        self.ast.functions.iter()
            .find(|(_, func)| func.name == name)
            .map(|(&id, _)| id)
    }
    
    pub fn register_builtin_function(&mut self, name: &str, signature: FunctionSignature, id: u32) {
        // Register a built-in function (from Engine) in the HIR function registry
        // Create a dummy function definition since built-ins are handled in the VM
        let dummy_def = FunctionDefinition {
            body: HirBlock {
                scope: 0,
                statements: vec![],
            },
            param_var_ids: vec![],
            scope_id: 0,
        };
        
        let function = Function {
            id,
            name: name.to_string(),
            signature,
            definition: dummy_def,
        };
        
        self.ast.functions.insert(id, function);
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
                    if result.ends_with("->") || result.ends_with("~>") || 
                       result.ends_with("(") || result.ends_with(",") {
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
    pub fn check_thunk_completeness(&self, var_id: u32, additional_args: usize) -> Option<(bool, usize, usize)> {
        let var_kind = self.get_var_kind_from_id(var_id)?;
        if let ValueKind::Thunk(thunk_type_str) = var_kind {
            // Parse the thunk type to see how many args it still needs
            if let Some((param_types, _, _)) = Self::parse_callable_type(&thunk_type_str) {
                // The param_types in a thunk type represent the remaining args needed
                let args_still_needed = param_types.len();
                let args_provided_so_far = 0; // We don't track this in the type string currently
                let total_args_needed = args_still_needed; // For now, assume thunk type shows remaining args
                let will_be_fully_applied = additional_args >= args_still_needed;
                return Some((will_be_fully_applied, total_args_needed, args_provided_so_far));
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
                    inner.split(',')
                        .map(|s| s.trim().to_string())
                        .collect()
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

    /// Check if two callable types (function or thunk) are structurally compatible
    /// This does proper structural comparison instead of string equality
    fn check_callable_type_compatibility(expected: &str, actual: &str) -> bool {
        let expected_parsed = Self::parse_callable_type(expected);
        let actual_parsed = Self::parse_callable_type(actual);
        
        match (expected_parsed, actual_parsed) {
            (Some((exp_params, exp_return, exp_is_thunk)), Some((act_params, act_return, act_is_thunk))) => {
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

    fn process_block(&mut self, block: Block) -> Result<HirBlock, HirError> {
        let parent = self.current_scope;

        let is_top_level = self.current_scope == 0;

        let new_scope = if is_top_level {
            self.current_scope // reuse root
        } else {
            self.ast.scopes.scopes.len()
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

    fn process_statement(&mut self, statement: Statement) -> Result<HirStmt, HirError> {
        match statement {
            Statement::Let { identifier, type_annotation, expression } => {
                let expr = self.process_expression(expression)?;
                let actual_kind = self.infer_variable_kind(&expr);
                
                // If type annotation is provided, use it and check compatibility
                // Otherwise, infer the type from the expression
                let expected_kind = if let Some(type_ann) = &type_annotation {
                    let parsed_kind = self.parse_type_string(type_ann);
                    // Check type compatibility
                    if !Self::check_type_compatibility(&parsed_kind, &actual_kind) {
                        return Err(HirError::TypeMismatch {
                            variable: identifier.clone(),
                            expected: parsed_kind,
                            actual: actual_kind,
                        });
                    }
                    parsed_kind
                } else {
                    // Infer type from expression
                    actual_kind
                };
                
                // For let, variable must not already exist in the current scope only
                // This allows shadowing: variables can be redeclared in nested scopes
                if self.var_exists_in_current_scope(&identifier) {
                    return Err(HirError::VariableAlreadyDeclared(format!("Variable '{}' is already declared", identifier)));
                }
                let slot = self.init_var(&identifier, expected_kind);
                Ok(HirStmt::Assign { slot, value: expr })
            }
            Statement::Assign { identifier, expression } => {
                let expr = self.process_expression(expression)?;
                let actual_kind = self.infer_variable_kind(&expr);
                // For assign, variable must already exist
                let slot = match self.resolve_var(&identifier) {
                    Some(id) => {
                        // Check type compatibility
                        let expected_kind = self.get_var_kind(id)
                            .ok_or_else(|| HirError::UnknownVariable(format!("Variable '{}' not found in scope", identifier)))?;
                        
                        if !Self::check_type_compatibility(&expected_kind, &actual_kind) {
                            return Err(HirError::TypeMismatch {
                                variable: identifier,
                                expected: expected_kind,
                                actual: actual_kind,
                            });
                        }
                        id
                    }
                    None => return Err(HirError::UnknownVariable(format!("Variable '{}' is not declared. Use 'let' to declare a new variable.", identifier))),
                };
                Ok(HirStmt::Assign { slot, value: expr })
            }
            Statement::AssignDecrement { identifier, expression } => {
                let expr = self.process_expression(expression)?;
                let actual_kind = self.infer_variable_kind(&expr);
                // For assign operations, variable must already exist
                let slot = match self.resolve_var(&identifier) {
                    Some(id) => {
                        // Check type compatibility
                        let expected_kind = self.get_var_kind(id)
                            .ok_or_else(|| HirError::UnknownVariable(format!("Variable '{}' not found in scope", identifier)))?;
                        
                        if !Self::check_type_compatibility(&expected_kind, &actual_kind) {
                            return Err(HirError::TypeMismatch {
                                variable: identifier,
                                expected: expected_kind,
                                actual: actual_kind,
                            });
                        }
                        id
                    }
                    None => return Err(HirError::UnknownVariable(format!("Variable '{}' is not declared. Use 'let' to declare a new variable.", identifier))),
                };
                Ok(HirStmt::AssignDecrement { slot, value: expr })
            }
            Statement::AssignIncrement { identifier, expression } => {
                let expr = self.process_expression(expression)?;
                let actual_kind = self.infer_variable_kind(&expr);
                // For assign operations, variable must already exist
                let slot = match self.resolve_var(&identifier) {
                    Some(id) => {
                        // Check type compatibility
                        let expected_kind = self.get_var_kind(id)
                            .ok_or_else(|| HirError::UnknownVariable(format!("Variable '{}' not found in scope", identifier)))?;
                        
                        if !Self::check_type_compatibility(&expected_kind, &actual_kind) {
                            return Err(HirError::TypeMismatch {
                                variable: identifier,
                                expected: expected_kind,
                                actual: actual_kind,
                            });
                        }
                        id
                    }
                    None => return Err(HirError::UnknownVariable(format!("Variable '{}' is not declared. Use 'let' to declare a new variable.", identifier))),
                };
                Ok(HirStmt::AssignIncrement { slot, value: expr })
            }
            Statement::If { arms, else_block } => {
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
                    None => Box::new(HirBlock { scope: self.current_scope, statements: vec![] }),
                };

                Ok(HirStmt::If {
                    arms: hir_arms,
                    else_block: hir_else_block,
                })
            }
            Statement::Match { expression, cases } => {
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
            Statement::FunctionDeclaration { identifier, arguments, return_type, body } => {
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
                let func_scope_id = self.ast.scopes.scopes.len();
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
                    body: HirBlock { scope: func_scope_id, statements: vec![] },
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
            Statement::Return { expression } => {
                let expr = self.process_expression(expression)?;
                Ok(HirStmt::Return { value: expr })
            }
            Statement::While { condition, body } => {
                // Transform while loop into: loop { if !condition { break; } body }
                // Save current scope
                let parent_scope = self.current_scope;
                
                // Create a new scope for the loop
                let loop_scope_id = self.ast.scopes.scopes.len();
                self.ast.scopes.scopes.push(HirBlockContext {
                    vars: Vec::new(),
                    parent: Some(parent_scope),
                });
                
                // Switch to loop scope
                self.current_scope = loop_scope_id;
                
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
                
                // Restore parent scope
                self.current_scope = parent_scope;
                
                // Statement loops don't have break_slot
                Ok(HirStmt::Loop { 
                    init_vars: vec![],
                    body: combined_body, 
                    break_slot: None 
                })
            }
            Statement::For { var_name, start, end, body } => {
                // Transform for loop into:
                // loop {
                //   var = start;  // Only on first iteration, handled by init_vars
                //   if var >= end { break; }
                //   body
                //   var = var + 1
                // }
                
                // Save current scope
                let parent_scope = self.current_scope;
                
                // Create a new scope for the loop
                let loop_scope_id = self.ast.scopes.scopes.len();
                self.ast.scopes.scopes.push(HirBlockContext {
                    vars: Vec::new(),
                    parent: Some(parent_scope),
                });
                
                // Switch to loop scope
                self.current_scope = loop_scope_id;
                
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
                
                // Restore parent scope
                self.current_scope = parent_scope;
                
                // Statement loops don't have break_slot
                Ok(HirStmt::Loop { 
                    init_vars: vec![(var_id, start_expr)],
                    body: combined_body, 
                    break_slot: None 
                })
            }
            Statement::Loop { init_vars, body } => {
                // Save current scope
                let parent_scope = self.current_scope;
                
                // Create a new scope for the loop
                let loop_scope_id = self.ast.scopes.scopes.len();
                self.ast.scopes.scopes.push(HirBlockContext {
                    vars: Vec::new(),
                    parent: Some(parent_scope),
                });
                
                // Switch to loop scope
                self.current_scope = loop_scope_id;
                
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
                
                // Restore parent scope
                self.current_scope = parent_scope;
                
                // Statement loops don't have break_slot
                Ok(HirStmt::Loop { 
                    init_vars: hir_init_vars,
                    body: hir_body, 
                    break_slot: None 
                })
            }
            Statement::Break { expression } => {
                let expr = if let Some(expr) = expression {
                    Some(self.process_expression(expr)?)
                } else {
                    None
                };
                Ok(HirStmt::Break { value: expr })
            }
            Statement::Continue => {
                Ok(HirStmt::Continue)
            }
            Statement::Expression(expression) => {
                let expr = self.process_expression(expression)?;

                Ok(HirStmt::Expression(expr))
            },
        }
    }

    /// Check if an expression contains a PostfixInvoke (nested ! operator)
    /// This helps detect confusing patterns like mul2(add10(i)!)!
    fn expression_contains_postfix_invoke(expr: &Expression) -> bool {
        match expr {
            Expression::Postfix { op, .. } => {
                matches!(op, PostfixOp::Invoke)
            }
            Expression::FunctionCall { arguments, .. } => {
                // Check if any argument contains PostfixInvoke
                arguments.iter().any(|arg| Self::expression_contains_postfix_invoke(arg))
            }
            Expression::Infix { lhs, rhs, .. } => {
                Self::expression_contains_postfix_invoke(lhs) || Self::expression_contains_postfix_invoke(rhs)
            }
            Expression::Prefix { rhs, .. } => {
                Self::expression_contains_postfix_invoke(rhs)
            }
            Expression::Group(inner) => {
                Self::expression_contains_postfix_invoke(inner)
            }
            _ => false,
        }
    }

    fn process_expression(&mut self, expression: Expression) -> Result<HirExpression, HirError> {
        match expression {
            Expression::Literal(lit) => {
                let cid = self.intern_constant(lit);
                Ok(HirExpression::Constant(cid))
            }           
            Expression::Identifier(identifier) => {
                if let Some(slot) = self.resolve_var(&identifier) {
                    Ok(HirExpression::Identifier(slot))
                } else if let Some(const_id) = self.resolve_const(&identifier) {
                    Ok(HirExpression::Constant(const_id))
                } else {
                    Err(HirError::UnknownVariable(identifier))
                }
            }
            Expression::Infix { lhs, op, rhs } => {
                let lhs_expr = self.process_expression(*lhs)?;
                let rhs_expr = self.process_expression(*rhs)?;
                
                // Type check binary operations
                let lhs_type = self.infer_variable_kind(&lhs_expr);
                let rhs_type = self.infer_variable_kind(&rhs_expr);
                
                match op {
                    BinaryOp::Add => {
                        // Addition supports both numbers and strings (concatenation)
                        // Allows Number + Number, String + String, String + Number, Number + String
                        // The VM will convert numbers to strings when concatenating with strings
                        // Allow Unknown types (will be resolved at runtime)
                        let is_valid = matches!(lhs_type, ValueKind::Number | ValueKind::String | ValueKind::Unknown) && 
                                       matches!(rhs_type, ValueKind::Number | ValueKind::String | ValueKind::Unknown);
                        
                        if !is_valid {
                            return Err(HirError::BinaryOpTypeError {
                                operator: "+".to_string(),
                                lhs_type: lhs_type.clone(),
                                rhs_type: rhs_type.clone(),
                                expected: "Number or String (supports string + number concatenation)".to_string(),
                            });
                        }
                    }
                    BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod | BinaryOp::Pow => {
                        // Other arithmetic operations require both operands to be numbers
                        // Allow Unknown types (will be resolved at runtime)
                        if !matches!(lhs_type, ValueKind::Number | ValueKind::Unknown) || 
                           !matches!(rhs_type, ValueKind::Number | ValueKind::Unknown) {
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
                        // Boolean operations require both operands to be booleans
                        // Allow Unknown types (will be resolved at runtime)
                        if !matches!(lhs_type, ValueKind::Boolean | ValueKind::Unknown) || 
                           !matches!(rhs_type, ValueKind::Boolean | ValueKind::Unknown) {
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
                    BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Gt | BinaryOp::Lt | BinaryOp::Ge | BinaryOp::Le => {
                        // Comparison operations require compatible types
                        if lhs_type != rhs_type && !matches!(lhs_type, ValueKind::Unknown) && !matches!(rhs_type, ValueKind::Unknown) {
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
                                expected: format!("compatible types (got {} and {})", Self::format_value_kind(&lhs_type), Self::format_value_kind(&rhs_type)),
                            });
                        }
                    }
                }

                Ok(HirExpression::Binary {
                    lhs: Box::new(lhs_expr),
                    rhs: Box::new(rhs_expr),
                    operator: op,
                })
            }
            Expression::Prefix { op, rhs } => {
                let rhs_expr = self.process_expression(*rhs)?;

                Ok(HirExpression::Unary {
                    operand: Box::new(rhs_expr),
                    operator: op,
                })
            }
            Expression::Postfix { lhs, op, args } => {
                match op {
                    PostfixOp::Invoke => {
                        // Special handling: if lhs is a FunctionCall, process it first, then apply the ! operator
                        // This handles cases like func(...)! which is valid (function call followed by force evaluation)
                        if let Expression::FunctionCall { callee, arguments: fc_args } = *lhs {
                            // Process the function call first
                            let func_expr = self.process_expression(Expression::FunctionCall {
                                callee,
                                arguments: fc_args,
                            })?;
                            
                            // Then apply the ! operator to force evaluation
                            // If there are additional args from Postfix, combine them
                            match func_expr {
                                HirExpression::FunctionCall { function_id, args: existing_args, invoke: _ } => {
                                    // Function call - apply ! to force evaluation
                                    let mut processed_args = existing_args;
                                    if let Some(arg_list) = args {
                                        for arg in arg_list {
                                            processed_args.push(self.process_expression(arg)?);
                                        }
                                    }
                                    return Ok(HirExpression::FunctionCall {
                                        function_id,
                                        args: processed_args,
                                        invoke: true,
                                    });
                                }
                                HirExpression::PostfixInvoke { operand, args: existing_args } => {
                                    // Already a PostfixInvoke - combine args and force evaluation
                                    let mut processed_args = Vec::new();
                                    if let Some(arg_list) = existing_args {
                                        processed_args.extend(arg_list);
                                    }
                                    if let Some(arg_list) = args {
                                        for arg in arg_list {
                                            processed_args.push(self.process_expression(arg)?);
                                        }
                                    }
                                    return Ok(HirExpression::PostfixInvoke {
                                        operand,
                                        args: if processed_args.is_empty() { None } else { Some(processed_args) },
                                    });
                                }
                                _ => {
                                    // Other expressions - wrap in PostfixInvoke
                                    let mut processed_args = Vec::new();
                                    if let Some(arg_list) = args {
                                        for arg in arg_list {
                                            processed_args.push(self.process_expression(arg)?);
                                        }
                                    }
                                    return Ok(HirExpression::PostfixInvoke {
                                        operand: Box::new(func_expr),
                                        args: if processed_args.is_empty() { None } else { Some(processed_args) },
                                    });
                                }
                            }
                        }
                        
                        // CRITICAL FIX: If lhs is an Identifier, check if it's a variable first
                        // Variables can contain thunks, and we want to invoke those thunks, not treat them as functions
                        // Only if it's not a variable should we check if it's a function
                        if let Expression::Identifier(identifier) = *lhs {
                            // Try to resolve as variable first (for thunks/PreparedCall stored in variables)
                            // Use aggressive resolution to ensure we find variables in all scopes (including function parameters)
                            // This is critical because variables can contain thunks that need to be invoked
                            if let Some(var_id) = self.resolve_var_aggressive(&identifier) {
                                // Check if the variable is a thunk or function type - if so, it should be invoked as a variable
                                let var_kind = self.get_var_kind_from_id(var_id);
                                let _is_callable = var_kind.as_ref()
                                    .map(|k| matches!(k, ValueKind::Thunk(_) | ValueKind::Function(_)))
                                    .unwrap_or(false);
                                
                                // If it's a callable type (thunk or function), treat it as a variable invocation
                                // Otherwise, also treat it as a variable (could be a thunk at runtime)
                                // This handles cases like add10!(i) where add10 is a variable containing a thunk
                                let mut processed_args = Vec::new();
                                if let Some(arg_list) = args {
                                    for arg in arg_list {
                                        processed_args.push(self.process_expression(arg)?);
                                    }
                                }
                                return Ok(HirExpression::PostfixInvoke {
                                    operand: Box::new(HirExpression::Identifier(var_id)),
                                    args: if processed_args.is_empty() { None } else { Some(processed_args) },
                                });
                            }
                            
                            // Not a variable - try to resolve as function
                            // But first double-check: if it exists as a variable in current scope, prefer that
                            // This handles cases where resolve_var_from_root might have missed it
                            if let Some(var_id) = self.resolve_var(&identifier) {
                                // Found as variable in current scope - use it
                                let mut processed_args = Vec::new();
                                if let Some(arg_list) = args {
                                    for arg in arg_list {
                                        processed_args.push(self.process_expression(arg)?);
                                    }
                                }
                                return Ok(HirExpression::PostfixInvoke {
                                    operand: Box::new(HirExpression::Identifier(var_id)),
                                    args: if processed_args.is_empty() { None } else { Some(processed_args) },
                                });
                            }
                            
                            // TYPE CHECKING: PostfixInvoke with ! operator (mul2!(...)) MUST be a thunk variable, not a function
                            // If it's a function, it's an error - you must use parentheses syntax to call functions
                            if let Some(_function_id) = self.resolve_function(&identifier) {
                                // It's a function - using ! operator is incorrect
                                // User should use mul2(...) instead of mul2!(...)
                                return Err(HirError::TypeError(format!(
                                    "Cannot use thunk invocation syntax '{}!' on a function. Use '{}' with parentheses to call the function.",
                                    identifier, identifier
                                )));
                            }
                            
                            // Neither variable nor function found
                            return Err(HirError::UnknownVariable(format!("{} is not a variable or function", identifier)));
                        }
                        
                        // For other expressions, process normally
                        let lhs_expr = self.process_expression(*lhs)?;
                        match lhs_expr {
                            HirExpression::FunctionCall { function_id, args: existing_args, .. } => {
                                Ok(HirExpression::FunctionCall {
                                    function_id,
                                    args: existing_args,
                                    invoke: true,
                                })
                            }
                            // If lhs is a PostfixInvoke, handle properly:
                            // If the outer Postfix has no extra args and the inner PostfixInvoke already has args,
                            // return the inner PostfixInvoke as-is.
                            // Otherwise, combine any inner and outer args appropriately.
                            HirExpression::PostfixInvoke { operand, args: existing_args } => {
                                // If there are no additional args from the outer Postfix, and the inner PostfixInvoke already has args,
                                // then the outer ! is just forcing the inner PostfixInvoke, so we keep it as-is.
                                // This handles mul2!(add10!(i))! correctly.
                                if args.is_none() && existing_args.is_some() {
                                    // The inner PostfixInvoke already has args and the outer ! has no additional args.
                                    // The outer ! is just forcing the inner PostfixInvoke, so we keep it as-is.
                                    return Ok(HirExpression::PostfixInvoke {
                                        operand,
                                        args: existing_args,
                                    });
                                }
                                
                                // Otherwise, combine existing args with new args
                                let mut processed_args = Vec::new();
                                // Process any existing args from the inner PostfixInvoke
                                if let Some(arg_list) = existing_args {
                                    for arg in arg_list {
                                        processed_args.push(arg);
                                    }
                                }
                                // Process any additional args from the outer Postfix
                                if let Some(arg_list) = args {
                                    for arg in arg_list {
                                        processed_args.push(self.process_expression(arg)?);
                                    }
                                }
                                // The outer ! means we want to force this, so we keep it as PostfixInvoke
                                // which will emit Invoke opcode. But we need to make sure the operand is evaluated.
                                Ok(HirExpression::PostfixInvoke {
                                    operand,
                                    args: if processed_args.is_empty() { None } else { Some(processed_args) },
                                })
                            }
                            // For non-function-call expressions, we assume it's a PreparedCall value
                            // that will be invoked at runtime. Process any additional arguments.
                            other => {
                                let mut processed_args = Vec::new();
                                if let Some(arg_list) = args {
                                    for arg in arg_list {
                                        processed_args.push(self.process_expression(arg)?);
                                    }
                                }
                                Ok(HirExpression::PostfixInvoke {
                                    operand: Box::new(other),
                                    args: if processed_args.is_empty() { None } else { Some(processed_args) },
                                })
                            },
                        }
                    }
                }
            }
            Expression::FunctionCall {
                callee,
                arguments,
            } => {
                // Special case: if callee is an Identifier, check if it's a function name first
                // before processing it as an expression
                if let Expression::Identifier(ref identifier_name) = *callee {
                    // Check if it's a function first
                    if let Some(function_id) = self.resolve_function(identifier_name) {
                        // It's a function - create FunctionCall with invoke: false (prepare the call)
                        let mut args: Vec<HirExpression> = Vec::new();
                        for argument in arguments {
                            args.push(self.process_expression(argument)?);
                        }
                        return Ok(HirExpression::FunctionCall { function_id, args, invoke: false });
                    }
                    // Not a function - will be handled as variable below
                }
                
                // Process the callee expression
                let callee_expr = self.process_expression(*callee)?;
                
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
                        return Ok(HirExpression::PostfixInvoke {
                            operand: Box::new(HirExpression::Identifier(var_id)),
                            args: Some(processed_args),
                        });
                    }
                    // If callee is a ComposeThunk, wrap it in a PostfixInvoke
                    HirExpression::ComposeThunk { first, second } => {
                        // Create a PostfixInvoke with the ComposeThunk as operand
                        // The bytecode emitter will handle calling a ComposeThunk
                        return Ok(HirExpression::PostfixInvoke {
                            operand: Box::new(HirExpression::ComposeThunk { first, second }),
                            args: Some(processed_args),
                        });
                    }
                    // If callee is a FunctionCall (nested call), combine the args
                    HirExpression::FunctionCall { function_id, args: existing_args, invoke: false } => {
                        // This is a prepared call being called again - combine the args
                        let mut combined_args = existing_args;
                        combined_args.extend(processed_args);
                        return Ok(HirExpression::FunctionCall {
                            function_id,
                            args: combined_args,
                            invoke: false,
                        });
                    }
                    // Other expressions - try to treat as callable
                    _ => {
                        // For other expressions, create a PostfixInvoke
                        // This allows calling any expression that evaluates to a callable
                        return Ok(HirExpression::PostfixInvoke {
                            operand: Box::new(callee_expr),
                            args: Some(processed_args),
                        });
                    }
                }
            }
            Expression::Group(expr) => {
                // Group expressions (parentheses) just unwrap and process the inner expression
                self.process_expression(*expr)
            }
            Expression::Compose { lhs, rhs, reverse } => {
                // Process both sides
                let first_expr = self.process_expression(*lhs)?;
                let second_expr = self.process_expression(*rhs)?;
                
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
            Expression::Loop { init_vars, body } => {
                // Save current scope
                let parent_scope = self.current_scope;
                
                // Create a new scope for the loop
                let loop_scope_id = self.ast.scopes.scopes.len();
                self.ast.scopes.scopes.push(HirBlockContext {
                    vars: Vec::new(),
                    parent: Some(parent_scope),
                });
                
                // Switch to loop scope
                self.current_scope = loop_scope_id;
                
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
                
                // Restore parent scope
                self.current_scope = parent_scope;
                
                Ok(HirExpression::Loop {
                    init_vars: hir_init_vars,
                    body: hir_body,
                    break_slot,
                })
            }
        }
    }
}
