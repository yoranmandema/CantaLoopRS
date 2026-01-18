//! Expression lowering: AST Expression → HIR Expression
//! 
//! This module handles lowering AST expressions to HIR expressions, including
//! type inference and expression transformation.

use serde::Serialize;

use crate::core::ast::{BinaryOp, UnaryOp};
use crate::core::entity_id::EntityId;

use super::HirBlock;

/// High-level Intermediate Representation of an expression.
#[derive(Debug, Clone, Serialize)]
pub enum HirExpression {
    Number(f64),
    String(String),
    Boolean(bool),
    Identifier(EntityId),
    Constant(EntityId),
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
        function_id: EntityId, // Function ID from functions registry
        args: Vec<HirExpression>,
        invoke: bool, // true if should invoke immediately (!), false if just prepare
    },
    Loop {
        init_vars: Vec<(EntityId, HirExpression)>, // (variable_id, initial_value) for loop initialization variables
        body: HirBlock,
        break_slot: Option<EntityId>, // Variable slot for break value (None for statement loops, Some(slot) for expression loops)
    },
    ComposeThunk {
        first: Box<HirExpression>,
        second: Box<HirExpression>,
    },
    PartialCall {
        func_id: EntityId,                 // Function ID from functions registry
        bound: Vec<Option<HirExpression>>, // None = hole, Some(expr) = bound argument
    },
    Array(Vec<HirExpression>), // Array literal: [expr, expr, ...]
    ArrayIndex {
        array: Box<HirExpression>, // The array being indexed
        index: Box<HirExpression>, // Single index expression
    },
    ArraySlice {
        array: Box<HirExpression>, // The array being sliced
        start: Option<Box<HirExpression>>, // None means from start
        end: Option<Box<HirExpression>>,   // None means to end
        step: Option<Box<HirExpression>>,  // None means step of 1
        inclusive_end: bool, // true for ..=, false for ..
    },
    Reducer {
        array: Box<HirExpression>, // Array to reduce
        reducer_type: ReducerType,
        reducer_args: Vec<HirExpression>, // Arguments for reducer (empty for sum, [init, fn] for fold)
    },
    StructInit {
        struct_name: String, // Struct type name
        fields: Vec<(String, HirExpression)>, // (field_name, value)
    },
    FieldAccess {
        base: Box<HirExpression>, // The struct instance
        field_name: String, // Field name to access
    },
    Closure {
        function_id: EntityId, // Function ID for the anonymous function
    },
}

/// Type of reducer function
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ReducerType {
    Sum,    // sum: equivalent to fold(0, add)
    Fold,   // fold(init, fn): custom reducer
    Map,    // map(fn): transform each element
    Filter, // filter(predicate): keep elements where predicate returns true
    Reduce, // reduce(fn): fold without initial value (uses first element)
}

// Type inference and expression processing will be moved here from the main file
// For now, this is a placeholder that exports the HirExpression type

