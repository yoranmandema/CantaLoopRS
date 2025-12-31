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
        },
        StdFunction {
            name: "fold",
            signature: FunctionSignature {
                params: vec![ValueKind::Unknown, ValueKind::Function("(num, num) -> num".to_string())],
                return_type: Box::new(ValueKind::Unknown),
            },
            arity: Arity::Fixed(2),
            impl_fn: Arc::new(|_args, _heap| {
                // fold is only used as a reducer in pipelines, not as a direct function call
                // This implementation should never be called directly
                panic!("fold should only be used as a reducer in pipelines (e.g., xs |> fold(init, fn))");
            }),
        },
        StdFunction {
            name: "add",
            signature: FunctionSignature {
                params: vec![ValueKind::Number, ValueKind::Number],
                return_type: Box::new(ValueKind::Number),
            },
            arity: Arity::Fixed(2),
            impl_fn: Arc::new(|args, _heap| {
                let a = args[0].as_number().expect("expected number");
                let b = args[1].as_number().expect("expected number");
                Value::number(a + b)
            }),
        },
        StdFunction {
            name: "mul",
            signature: FunctionSignature {
                params: vec![ValueKind::Number, ValueKind::Number],
                return_type: Box::new(ValueKind::Number),
            },
            arity: Arity::Fixed(2),
            impl_fn: Arc::new(|args, _heap| {
                let a = args[0].as_number().expect("expected number");
                let b = args[1].as_number().expect("expected number");
                Value::number(a * b)
            }),
        },
        StdFunction {
            name: "min",
            signature: FunctionSignature {
                params: vec![ValueKind::Number, ValueKind::Number],
                return_type: Box::new(ValueKind::Number),
            },
            arity: Arity::Variadic { min: 2 },
            impl_fn: Arc::new(|args, _heap| {
                let min_val = args
                    .iter()
                    .map(|v| v.as_number().expect("expected number"))
                    .fold(f64::INFINITY, |acc, x| if x < acc { x } else { acc });
                Value::number(min_val)
            }),
        },
        StdFunction {
            name: "max",
            signature: FunctionSignature {
                params: vec![ValueKind::Number, ValueKind::Number],
                return_type: Box::new(ValueKind::Number),
            },
            arity: Arity::Variadic { min: 2 },
            impl_fn: Arc::new(|args, _heap| {
                let max_val = args
                    .iter()
                    .map(|v| v.as_number().expect("expected number"))
                    .fold(f64::NEG_INFINITY, |acc, x| if x > acc { x } else { acc });
                Value::number(max_val)
            })
        },
        StdFunction {
            name: "clamp",
            signature: FunctionSignature {
                params: vec![ValueKind::Number, ValueKind::Number, ValueKind::Number],
                return_type: Box::new(ValueKind::Number),
            },
            arity: Arity::Fixed(3),
            impl_fn: Arc::new(|args, _heap| {
                let val = args[0].as_number().expect("expected number as input");
                let min = args[1].as_number().expect("expected number as min boundary");
                let max = args[2].as_number().expect("expected number as max boundary");

                if min > max {
                    panic!("clamp: min must be <= max");
                }

                let clamped = val.clamp(min, max);

                Value::number(clamped)
            }),     
        }]
    },
    submodules: vec![],
    };
}
