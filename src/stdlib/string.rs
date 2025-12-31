use crate::core::engine::{Arity, StdFunction, StdModule};
use crate::core::hir_lowering::{FunctionSignature, ValueKind};
use crate::core::vm::Value;
use std::sync::Arc;

/// Standard string module.
/// 
/// This is pure declarative metadata describing the string module.
/// It does not mutate the Engine - it's compiler input, not runtime behavior.
lazy_static::lazy_static! {
    pub static ref STRING_MODULE: StdModule = StdModule {
    name: "string",
    functions: {
        vec![
        StdFunction {
            name: "str_len",
            signature: FunctionSignature {
                params: vec![ValueKind::String],
                return_type: Box::new(ValueKind::Number),
            },
            arity: Arity::Fixed(1),
            impl_fn: Arc::new(|args, heap| {
                if let Some(str) = args[0].as_string(heap) {
                    Value::number(str.len() as f64)
                } else {
                    panic!("str_len expects string argument")
                }
            }),
        }]
    },
    submodules: vec![],
    };
}
