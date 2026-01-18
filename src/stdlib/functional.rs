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
                // Generic reducer:
                // map: (T -> U) -> U[]
                // In pipelines, `xs` is supplied by `|>` and `T` is inferred from the LHS array element type.
                params: vec![ValueKind::ThunkSig {
                    params: vec![ValueKind::TypeVar(0)],
                    return_type: Box::new(ValueKind::TypeVar(1)),
                    is_effectful: false,
                }],
                return_type: Box::new(ValueKind::Array(Box::new(ValueKind::TypeVar(1)))),
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
                // Generic reducer:
                // filter: (T -> bool) -> T[]
                // In pipelines, `T` is inferred from the LHS array element type.
                params: vec![ValueKind::ThunkSig {
                    params: vec![ValueKind::TypeVar(0)],
                    return_type: Box::new(ValueKind::Boolean),
                    is_effectful: false,
                }],
                return_type: Box::new(ValueKind::Array(Box::new(ValueKind::TypeVar(0)))),
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
                // Generic reducer:
                // fold: (init: T, f: (T -> T)) -> T
                // In pipelines, the array is supplied by `|>`; `T` is inferred from `init`.
                params: vec![
                    ValueKind::TypeVar(0),
                    ValueKind::FnSig {
                        params: vec![ValueKind::TypeVar(0)],
                        return_type: Box::new(ValueKind::TypeVar(0)),
                        is_effectful: false,
                    },
                ],
                return_type: Box::new(ValueKind::TypeVar(0)),
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
                // Generic reducer:
                // reduce: (f: ((T, T) -> T)) -> T
                // In pipelines, `T` is inferred from the LHS array element type.
                params: vec![ValueKind::FnSig {
                    params: vec![ValueKind::TypeVar(0), ValueKind::TypeVar(0)],
                    return_type: Box::new(ValueKind::TypeVar(0)),
                    is_effectful: false,
                }],
                return_type: Box::new(ValueKind::TypeVar(0)),
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
