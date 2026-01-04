use crate::core::engine::{Arity, StdFunction, StdModule};
use crate::core::hir_lowering::{FunctionSignature, ValueKind};
use crate::core::vm::Value;
use std::sync::Arc;

/// Standard comparison module.
/// 
/// This is pure declarative metadata describing the comparison module.
/// It does not mutate the Engine - it's compiler input, not runtime behavior.
lazy_static::lazy_static! {
    pub static ref COMPARISON_MODULE: StdModule = StdModule {
    name: "comparison",
    functions: {
        vec![
        StdFunction {
            name: "eq",
            signature: FunctionSignature {
                params: vec![ValueKind::Any, ValueKind::Any],
                return_type: Box::new(ValueKind::Boolean),
            },
            arity: Arity::Fixed(2),
            impl_fn: Arc::new(|args, heap| {
                let a = &args[0];
                let b = &args[1];
                let result = if let (Some(a_num), Some(b_num)) = (a.as_number(), b.as_number()) {
                    a_num == b_num
                } else if let (Some(a_str), Some(b_str)) = (a.as_string(heap), b.as_string(heap)) {
                    a_str == b_str
                } else if let (Some(a_bool), Some(b_bool)) = (a.as_boolean(), b.as_boolean()) {
                    a_bool == b_bool
                } else {
                    panic!("Comparison eq on incompatible types")
                };
                Value::boolean(result)
            }),
        },
        StdFunction {
            name: "neq",
            signature: FunctionSignature {
                params: vec![ValueKind::Any, ValueKind::Any],
                return_type: Box::new(ValueKind::Boolean),
            },
            arity: Arity::Fixed(2),
            impl_fn: Arc::new(|args, heap| {
                let a = &args[0];
                let b = &args[1];
                let result = if let (Some(a_num), Some(b_num)) = (a.as_number(), b.as_number()) {
                    a_num != b_num
                } else if let (Some(a_str), Some(b_str)) = (a.as_string(heap), b.as_string(heap)) {
                    a_str != b_str
                } else if let (Some(a_bool), Some(b_bool)) = (a.as_boolean(), b.as_boolean()) {
                    a_bool != b_bool
                } else {
                    panic!("Comparison neq on incompatible types")
                };
                Value::boolean(result)
            }),
        },
        StdFunction {
            name: "lt",
            signature: FunctionSignature {
                params: vec![ValueKind::Number, ValueKind::Number],
                return_type: Box::new(ValueKind::Boolean),
            },
            arity: Arity::Fixed(2),
            impl_fn: Arc::new(|args, _heap| {
                let a = args[0].as_number().expect("lt expects number arguments");
                let b = args[1].as_number().expect("lt expects number arguments");
                Value::boolean(a < b)
            }),
        },
        StdFunction {
            name: "lte",
            signature: FunctionSignature {
                params: vec![ValueKind::Number, ValueKind::Number],
                return_type: Box::new(ValueKind::Boolean),
            },
            arity: Arity::Fixed(2),
            impl_fn: Arc::new(|args, _heap| {
                let a = args[0].as_number().expect("lte expects number arguments");
                let b = args[1].as_number().expect("lte expects number arguments");
                Value::boolean(a <= b)
            }),
        },
        StdFunction {
            name: "gt",
            signature: FunctionSignature {
                params: vec![ValueKind::Number, ValueKind::Number],
                return_type: Box::new(ValueKind::Boolean),
            },
            arity: Arity::Fixed(2),
            impl_fn: Arc::new(|args, _heap| {
                let a = args[0].as_number().expect("gt expects number arguments");
                let b = args[1].as_number().expect("gt expects number arguments");
                Value::boolean(a > b)
            }),
        },
        StdFunction {
            name: "gte",
            signature: FunctionSignature {
                params: vec![ValueKind::Number, ValueKind::Number],
                return_type: Box::new(ValueKind::Boolean),
            },
            arity: Arity::Fixed(2),
            impl_fn: Arc::new(|args, _heap| {
                let a = args[0].as_number().expect("gte expects number arguments");
                let b = args[1].as_number().expect("gte expects number arguments");
                Value::boolean(a >= b)
            }),
        }]
    },
    structs: vec![],
    submodules: vec![],
    };
}
