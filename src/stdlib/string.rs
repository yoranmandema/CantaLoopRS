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
            name: "len",
            signature: FunctionSignature {
                params: vec![ValueKind::String],
                return_type: Box::new(ValueKind::Number),
            },
            arity: Arity::Fixed(1),
            impl_fn: Arc::new(|args, heap| {
                if let Some(str) = args[0].as_string(heap) {
                    Value::number(str.len() as f64)
                } else {
                    panic!("len expects string argument")
                }
            }),
        },
        StdFunction {
            name: "join",
            signature: FunctionSignature {
                // join(array, separator)
                params: vec![ ValueKind::Array(Box::new(ValueKind::Any)), ValueKind::String],
                return_type: Box::new(ValueKind::String),
            },
            arity: Arity::Fixed(2),
            impl_fn: Arc::new(|args, heap| {
                let arr = args[0].as_array(heap).expect("join expects array as first argument");
                let sep = args[1].as_string(heap).expect("join expects string as second argument");

                let mut strings: Vec<String> = Vec::with_capacity(arr.len());
                for v in arr {
                    // Try as string, if not, coerce to string representation
                    let s = if let Some(s) = v.as_string(heap) {
                        s
                    } else {
                        v.value_to_string(heap)
                    };
                    strings.push(s);
                }
                Value::string_with_heap(strings.join(&sep), heap)
            }),
        },

        StdFunction {
            name: "concat",
            signature: FunctionSignature {
                // Accepts a variable length argument list, all strings
                params: vec![ValueKind::Array(Box::new(ValueKind::String))],
                return_type: Box::new(ValueKind::String),
            },
            arity: Arity::Fixed(1),
            impl_fn: Arc::new(|args, heap| {
                // concat expects an array of strings as a single argument
                let arr = args[0].as_array(heap).expect("concat expects array of strings");
                let mut result = String::new();
                for v in arr {
                    let s = v.as_string(heap).unwrap_or_else(|| v.value_to_string(heap));
                    result.push_str(&s);
                }
                Value::string_with_heap(result, heap)
            }),
        },
        ]
    },
    structs: vec![],
    submodules: vec![],
    };
}
