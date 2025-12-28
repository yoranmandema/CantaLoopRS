use crate::core::engine::{Arity, StdFunction, StdModule};
use crate::core::semantic_analyser::{FunctionSignature, ValueKind};
use crate::core::vm::Value;
use std::sync::Arc;

/// Standard library I/O module.
/// 
/// This is pure declarative metadata describing the std module.
/// It does not mutate the Engine - it's compiler input, not runtime behavior.
lazy_static::lazy_static! {
    pub static ref STD_MODULE: StdModule = StdModule {
    name: "std",
    functions: {
        vec![
        StdFunction {
            name: "print",
            signature: FunctionSignature {
                params: vec![ValueKind::String],
                return_type: Box::new(ValueKind::String),
            },
            arity: Arity::Fixed(1),
            impl_fn: Arc::new(|args, heap| {
                let s = args[0].value_to_string(heap);
                println!("{}", s);
                Value::string_with_heap(String::new(), heap)
            }),
        }]
    },
    submodules: vec![],
    };
}
