use crate::core::engine::{Arity, StdFunction, StdModule};
use crate::core::hir_lowering::{FunctionSignature, ValueKind};
use crate::core::vm::Value;
use std::sync::Arc;

/// Standard number module.
///
/// This is pure declarative metadata describing the string module.
/// It does not mutate the Engine - it's compiler input, not runtime behavior.
lazy_static::lazy_static! {
    pub static ref NUMBER_MODULE: StdModule = StdModule {
    name: "number",
    functions: {
        vec![
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
                        panic!("clamp: min must be <= max (got min={}, max={})", min, max);
                    }

                    let clamped = val.clamp(min, max);

                    Value::number(clamped)
                }),
            }
        ]
    },
    structs: vec![],
    submodules: vec![],
    };
}
