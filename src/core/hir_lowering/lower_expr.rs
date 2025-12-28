//! Expression lowering: AST Expression → HIR Expression
//! 
//! This module handles lowering AST expressions to HIR expressions, including
//! type inference and expression transformation.

use crate::core::ast::{BinaryOp, UnaryOp};

use super::HirBlock;

/// High-level Intermediate Representation of an expression.
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
    PartialCall {
        func_id: u32,                      // Function ID from functions registry
        bound: Vec<Option<HirExpression>>, // None = hole, Some(expr) = bound argument
    },
}

// Type inference and expression processing will be moved here from the main file
// For now, this is a placeholder that exports the HirExpression type

