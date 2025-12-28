use crate::core::engine::{Arity, StdFunction, StdModule};
use crate::core::hir_lowering::{FunctionSignature, ValueKind};
use crate::core::vm::Value;
use std::sync::Arc;

/// Standard library math module.
/// 
/// This is pure declarative metadata describing the math module.
/// It does not mutate the Engine - it's compiler input, not runtime behavior.
lazy_static::lazy_static! {
    pub static ref MATH_MODULE: StdModule = StdModule {
    name: "math",
    functions: {
        vec![
        StdFunction {
            name: "round",
            signature: FunctionSignature {
                params: vec![ValueKind::Number, ValueKind::Number],
                return_type: Box::new(ValueKind::Number),
            },
            arity: Arity::Fixed(2),
            impl_fn: Arc::new(|args, _heap| {
                let a = args[0].as_number().expect("expected number");
                let b = args[1].as_number().expect("expected number");
                Value::number(a.round() + b.round())
            }),
        },
        StdFunction {
            name: "floor",
            signature: FunctionSignature {
                params: vec![ValueKind::Number],
                return_type: Box::new(ValueKind::Number),
            },
            arity: Arity::Fixed(1),
            impl_fn: Arc::new(|args, _heap| {
                let x = args[0].as_number().expect("expected number");
                Value::number(x.floor())
            }),
        },
        StdFunction {
            name: "ceil",
            signature: FunctionSignature {
                params: vec![ValueKind::Number],
                return_type: Box::new(ValueKind::Number),
            },
            arity: Arity::Fixed(1),
            impl_fn: Arc::new(|args, _heap| {
                let x = args[0].as_number().expect("expected number");
                Value::number(x.ceil())
            }),
        },
        StdFunction {
            name: "abs",
            signature: FunctionSignature {
                params: vec![ValueKind::Number],
                return_type: Box::new(ValueKind::Number),
            },
            arity: Arity::Fixed(1),
            impl_fn: Arc::new(|args, _heap| {
                let x = args[0].as_number().expect("expected number");
                Value::number(x.abs())
            }),
        },
        StdFunction {
            name: "pow",
            signature: FunctionSignature {
                params: vec![ValueKind::Number, ValueKind::Number],
                return_type: Box::new(ValueKind::Number),
            },
            arity: Arity::Fixed(2),
            impl_fn: Arc::new(|args, _heap| {
                let base = args[0].as_number().expect("expected number");
                let exp = args[1].as_number().expect("expected number");
                Value::number(base.powf(exp))
            }),
        },
        StdFunction {
            name: "sqrt",
            signature: FunctionSignature {
                params: vec![ValueKind::Number],
                return_type: Box::new(ValueKind::Number),
            },
            arity: Arity::Fixed(1),
            impl_fn: Arc::new(|args, _heap| {
                let x = args[0].as_number().expect("expected number");
                Value::number(x.sqrt())
            }),
        },
        StdFunction {
            name: "sum",
            signature: FunctionSignature {
                params: vec![ValueKind::Number, ValueKind::Number],
                return_type: Box::new(ValueKind::Number),
            },
            arity: Arity::Variadic { min: 2 },
            impl_fn: Arc::new(|args, _heap| {
                let sum: f64 = args
                    .iter()
                    .map(|v| v.as_number().expect("expected number"))
                    .sum();
                Value::number(sum)
            }),
        }]
    },
    submodules: vec![],
    };
}
