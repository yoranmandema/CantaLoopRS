use crate::core::engine::{Arity, StdFunction, StdModule};
use crate::core::hir_lowering::{FunctionSignature, ValueKind};
use crate::core::vm::Value;
use std::sync::Arc;

/// Standard library math module.
///
/// This is pure declarative metadata describing the math module.
/// It does not mutate the Engine - it's compiler input, not runtime behavior.
lazy_static::lazy_static! {
    pub static ref MATH_MODULE: StdModule = {
        // Use macro for simple fixed-arity functions
        let mut module = crate::melon_module! {
            module math {
                fn round(a: num, b: num) -> num {
                    |args, _heap| {
                        let a = args[0].as_number().expect("expected number");
                        let b = args[1].as_number().expect("expected number");
                        Value::number(a.round() + b.round())
                    }
                }
                fn floor(x: num) -> num {
                    |args, _heap| {
                        let x = args[0].as_number().expect("expected number");
                        Value::number(x.floor())
                    }
                }
                fn ceil(x: num) -> num {
                    |args, _heap| {
                        let x = args[0].as_number().expect("expected number");
                        Value::number(x.ceil())
                    }
                }
                fn abs(x: num) -> num {
                    |args, _heap| {
                        let x = args[0].as_number().expect("expected number");
                        Value::number(x.abs())
                    }
                }
                fn pow(base: num, exp: num) -> num {
                    |args, _heap| {
                        let base = args[0].as_number().expect("expected number");
                        let exp = args[1].as_number().expect("expected number");
                        Value::number(base.powf(exp))
                    }
                }
                fn sqrt(x: num) -> num {
                    |args, _heap| {
                        let x = args[0].as_number().expect("expected number");
                        Value::number(x.sqrt())
                    }
                }
            }
        };
        
        // Manually add variadic functions
        module.functions.extend(vec![
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
                }),
            },
        ]);
        
        module
    };
}
