use std::collections::HashMap;

use crate::ast::{BinaryOp, Block, Expression, Literal, Program, Statement, UnaryOp, PostfixOp};

#[derive(Debug, Clone)]
pub struct FunctionSignature {
    #[allow(dead_code)]
    pub params: Vec<ValueKind>,
    #[allow(dead_code)]
    pub return_type: Box<ValueKind>,
}

#[derive(Debug, Clone)]
pub enum ValueKind {
    Number, 
    String,
    Boolean, 
    Unknown,
}

#[derive(Debug, Clone)]
pub enum ConstantValue {
    Number(f64),
    String(String),
    Boolean(bool),
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
    pub scope_id: ScopeId,       // The function's scope ID
}

#[derive(Debug, Clone)]
pub struct Function {
    pub id: u32,
    pub name: String,
    pub signature: FunctionSignature,
    pub definition: FunctionDefinition,
}

#[derive(Debug)]
pub struct HirAst {
    // High-level Intermediate Representation
    pub constants: Vec<Constant>,
    pub blocks: Vec<HirBlock>,
    pub scopes: ScopeArena,
    pub functions: std::collections::HashMap<u32, Function>, // Function ID -> Function struct
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
    Return {
        value: HirExpression,
    },
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
}

pub type ScopeId = usize;

#[derive(Debug)]
pub struct ScopeArena {
    scopes: Vec<HirBlockContext>,
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

    fn resolve_var(&self, name: &str) -> Option<u32> {
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

    fn get_var_kind(&self, var_id: u32) -> Option<ValueKind> {
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

    fn check_type_compatibility(expected: &ValueKind, actual: &ValueKind) -> bool {
        // Types must match exactly
        match (expected, actual) {
            (ValueKind::Number, ValueKind::Number) => true,
            (ValueKind::String, ValueKind::String) => true,
            (ValueKind::Boolean, ValueKind::Boolean) => true,
            (ValueKind::Unknown, ValueKind::Unknown) => true,
            // Unknown can be assigned to Unknown, but once a type is known, it must match
            _ => false,
        }
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
            HirExpression::Binary { operator, .. } => {
                match operator {
                    BinaryOp::And | BinaryOp::Or => ValueKind::Boolean,
                    BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Gt | BinaryOp::Lt | BinaryOp::Ge | BinaryOp::Le => ValueKind::Boolean,
                    _ => ValueKind::Number, // Other binary ops produce numbers
                }
            }
            HirExpression::Unary { .. } => ValueKind::Number, // Unary ops typically produce numbers
            HirExpression::FunctionCall { function_id, .. } => {
                // Look up the function's return type from its signature
                if let Some(func) = self.ast.functions.get(function_id) {
                    *func.signature.return_type.clone()
                } else {
                    ValueKind::Unknown
                }
            }
            HirExpression::PostfixInvoke { operand, args: _ } => {
                // Invoking a prepared call - result type is the same as the function's return type
                self.infer_variable_kind(&operand)
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

    fn resolve_function(&self, name: &str) -> Option<u32> {
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

    fn parse_type_string(&self, type_str: &str) -> ValueKind {
        match type_str.to_lowercase().as_str() {
            "number" | "num" | "int" | "float" => ValueKind::Number,
            "string" | "str" => ValueKind::String,
            "boolean" | "bool" => ValueKind::Boolean,
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
            Statement::Let { identifier, expression } => {
                let expr = self.process_expression(expression)?;
                let kind = self.infer_variable_kind(&expr);
                // For let, variable must not already exist
                if self.resolve_var(&identifier).is_some() {
                    return Err(HirError::VariableAlreadyDeclared(format!("Variable '{}' is already declared", identifier)));
                }
                let slot = self.init_var(&identifier, kind);
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
            Statement::FunctionDeclaration { identifier, arguments, body } => {
                // Parse argument types
                let mut param_types = Vec::new();
                for arg in &arguments {
                    let param_kind = self.parse_type_string(&arg.kind);
                    param_types.push(param_kind);
                }

                // Create function signature (return type is Unknown for now since we don't have return type annotations)
                let signature = FunctionSignature {
                    params: param_types,
                    return_type: Box::new(ValueKind::Unknown),
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
            Statement::Expression(expression) => {
                let expr = self.process_expression(expression)?;

                Ok(HirStmt::Expression(expr))
            },
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
                        // Special handling: if lhs is a FunctionCall that can't be resolved as a function,
                        // treat it as a variable invocation (for currying/thunks)
                        if let Expression::FunctionCall { identifier, arguments: fc_args } = *lhs {
                            // Try to resolve as function first
                            if let Some(function_id) = self.resolve_function(&identifier) {
                                // It's a function - process as normal function call with invoke
                                let mut processed_args = Vec::new();
                                for arg in fc_args {
                                    processed_args.push(self.process_expression(arg)?);
                                }
                                return Ok(HirExpression::FunctionCall {
                                    function_id,
                                    args: processed_args,
                                    invoke: true,
                                });
                            }
                            
                            // Not a function - try to resolve as variable (for thunks/PreparedCall)
                            if let Some(var_id) = self.resolve_var(&identifier) {
                                // It's a variable - this should be a PreparedCall at runtime
                                // Process the arguments from the FunctionCall, plus any additional args from Postfix
                                let mut processed_args = Vec::new();
                                for arg in fc_args {
                                    processed_args.push(self.process_expression(arg)?);
                                }
                                if let Some(arg_list) = args {
                                    for arg in arg_list {
                                        processed_args.push(self.process_expression(arg)?);
                                    }
                                }
                                // Create PostfixInvoke with the variable as operand
                                return Ok(HirExpression::PostfixInvoke {
                                    operand: Box::new(HirExpression::Identifier(var_id)),
                                    args: if processed_args.is_empty() { None } else { Some(processed_args) },
                                });
                            }
                            
                            // Neither function nor variable found
                            return Err(HirError::UnknownVariable(format!("{} is not a function or variable", identifier)));
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
                identifier,
                arguments,
            } => {
                // Try to resolve as function first
                if let Some(function_id) = self.resolve_function(&identifier) {
                    let mut args: Vec<HirExpression> = Vec::new();
                    for argument in arguments {
                        let expr = self.process_expression(argument)?;
                        args.push(expr);
                    }
                    // Function call without ! means prepare the call (don't invoke)
                    return Ok(HirExpression::FunctionCall { function_id, args, invoke: false });
                }
                
                // Function not found
                return Err(HirError::UnknownVariable(format!("{} is not a function or not found", identifier)));
            }
            Expression::Group(expr) => {
                // Group expressions (parentheses) just unwrap and process the inner expression
                self.process_expression(*expr)
            }
        }
    }
}
