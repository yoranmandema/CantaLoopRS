use crate::core::engine::{Arity, StdFunction, StdModule};
use crate::core::hir_lowering::{FunctionSignature, ValueKind};
use crate::core::vm::Value;
use std::sync::Arc;

/// Standard string module.
///
/// This is pure declarative metadata describing the string module.
/// It does not mutate the Engine - it's compiler input, not runtime behavior.
lazy_static::lazy_static! {
    pub static ref LOGIC_MODULE: StdModule = StdModule {
    name: "logic",
    functions: {
        vec![
        StdFunction {
            name: "and",
            signature: FunctionSignature {
                params: vec![ValueKind::Boolean, ValueKind::Boolean],
                return_type: Box::new(ValueKind::Boolean),
            },
            arity: Arity::Fixed(2),
            impl_fn: Arc::new(|args, heap| {
                let a = args[0].as_boolean().expect("and expects boolean arguments");
                let b = args[1].as_boolean().expect("and expects boolean arguments");
                Value::boolean(a && b)
            }),
        },

        StdFunction {
            name: "or",
            signature: FunctionSignature {
                params: vec![ValueKind::Boolean, ValueKind::Boolean],
                return_type: Box::new(ValueKind::Boolean),
            },
            arity: Arity::Fixed(2),
            impl_fn: Arc::new(|args, heap| {
                let a = args[0].as_boolean().expect("or expects boolean arguments");
                let b = args[1].as_boolean().expect("or expects boolean arguments");
                Value::boolean(a || b)
            }),
        },

        StdFunction {
            name: "not",
            signature: FunctionSignature {
                params: vec![ValueKind::Boolean],
                return_type: Box::new(ValueKind::Boolean),
            },
            arity: Arity::Fixed(1),
            impl_fn: Arc::new(|args, heap| {
                let a = args[0].as_boolean().expect("not expects boolean argument");
                Value::boolean(!a)
            }),
        },

        StdFunction {
            name: "xor",
            signature: FunctionSignature {
                params: vec![ValueKind::Boolean, ValueKind::Boolean],
                return_type: Box::new(ValueKind::Boolean),
            },
            arity: Arity::Fixed(2),
            impl_fn: Arc::new(|args, heap| {
                let a = args[0].as_boolean().expect("xor expects boolean arguments");
                let b = args[1].as_boolean().expect("xor expects boolean arguments");
                Value::boolean(a ^ b)
            }),
        }
        ]
    },
    structs: vec![],
    submodules: vec![],
    };
}
