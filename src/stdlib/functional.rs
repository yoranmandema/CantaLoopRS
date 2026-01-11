use crate::core::engine::{Arity, StdFunction, StdModule};
use crate::core::hir_lowering::{FunctionSignature, ValueKind};
use std::sync::Arc;

lazy_static::lazy_static! {
    pub static ref FUNCTIONAL_MODULE: StdModule = StdModule {
    name: "functional",
    functions: {
        vec![
        // map, filter, and fold are now VM bytecode operations (reducers)
        // They are kept here for registration/import purposes, but should never be called directly
        // Reducers are detected and compiled to Map/Filter/Fold opcodes
        StdFunction {
            name: "map",
            signature: FunctionSignature {
                params: vec![ValueKind::Thunk("Any ~> Any".to_string()), ValueKind::Array(Box::new(ValueKind::Any))],
                return_type: Box::new(ValueKind::Array(Box::new(ValueKind::Any))),
                is_effectful: false,
            },
            // Support partial application: map(fn) creates a thunk, map(fn, xs) applies immediately
            // When used in pipelines (xs |> map(fn)), only the function is provided
            arity: Arity::Variadic { min: 1 },
            impl_fn: Arc::new(|_args, _heap| {
                panic!("map should only be used as a reducer in pipelines (e.g., xs |> map(fn)) or with partial application");
            }),
            docs: None,
        },
        StdFunction {
            name: "filter",
            signature: FunctionSignature {
                params: vec![ValueKind::Thunk("Any ~> Boolean".to_string()), ValueKind::Array(Box::new(ValueKind::Any))],
                return_type: Box::new(ValueKind::Array(Box::new(ValueKind::Any))),
                is_effectful: false,
            },
            // Support partial application: filter(pred) creates a thunk, filter(pred, xs) applies immediately
            // When used in pipelines (xs |> filter(pred)), only the predicate is provided
            arity: Arity::Variadic { min: 1 },
            impl_fn: Arc::new(|_args, _heap| {
                panic!("filter should only be used as a reducer in pipelines (e.g., xs |> filter(pred)) or with partial application");
            }),
            docs: None,
        },
        StdFunction {
            name: "fold",
            signature: FunctionSignature {
                params: vec![ValueKind::Unknown, ValueKind::Function("(num, num) -> num".to_string())],
                return_type: Box::new(ValueKind::Unknown),
                is_effectful: false,
            },
            arity: Arity::Fixed(2),
            impl_fn: Arc::new(|_args, _heap| {
                panic!("fold should only be used as a reducer in pipelines (e.g., xs |> fold(init, fn))");
            }),
            docs: None,
        },
        StdFunction {
            name: "reduce",
            signature: FunctionSignature {
                params: vec![ValueKind::Function("(num, num) -> num".to_string())],
                return_type: Box::new(ValueKind::Unknown),
                is_effectful: false,
            },
            arity: Arity::Fixed(1),
            impl_fn: Arc::new(|_args, _heap| {
                // reduce is only used as a reducer in pipelines, not as a direct function call
                // This implementation should never be called directly
                panic!("reduce should only be used as a reducer in pipelines (e.g., xs |> reduce(fn))");
            }),
            docs: None,
        },
        ]
    },
    structs: vec![],
    submodules: vec![],
    docs: None,
    };
}
