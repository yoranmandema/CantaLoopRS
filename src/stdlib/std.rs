use crate::core::engine::{Arity, StdFunction, StdModule};
use crate::core::hir_lowering::{FunctionSignature, ValueKind};
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
        },
        StdFunction {
            name: "format_number",
            signature: FunctionSignature {
                params: vec![ValueKind::Number, ValueKind::Number],
                return_type: Box::new(ValueKind::String),
            },
            arity: Arity::Fixed(2),
            impl_fn: Arc::new(|args, heap| {
                let n = args[0].as_number().expect("expected number");
                let decimals = args[1].as_number().expect("expected number") as i32;
                let formatted = format!("{:.1$}", n, decimals as usize);
                Value::string_with_heap(formatted, heap)
            }),
        },
        StdFunction {
            name: "array_length",
            signature: FunctionSignature {
                params: vec![ValueKind::Unknown],
                return_type: Box::new(ValueKind::Number),
            },
            arity: Arity::Fixed(1),
            impl_fn: Arc::new(|args, heap| {
                if let Some(arr) = args[0].as_array(heap) {
                    Value::number(arr.len() as f64)
                } else {
                    panic!("array_length expects array argument")
                }
            }),
        }]
    },
    submodules: vec![],
    };
}
