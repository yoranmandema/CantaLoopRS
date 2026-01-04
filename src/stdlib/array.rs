use crate::core::engine::{Arity, StdFunction, StdModule};
use crate::core::hir_lowering::{FunctionSignature, ValueKind};
use crate::core::vm::Value;
use std::sync::Arc;

/// Standard array module.
/// 
/// This is pure declarative metadata describing the array module.
/// It does not mutate the Engine - it's compiler input, not runtime behavior.

// Helper to compute index with negative index support
fn compute_index(index_val: &Value, len: usize) -> usize {
    let idx = index_val
        .as_number()
        .unwrap_or_else(|| {
            panic!("Array index must be a number, got: {:?}", index_val)
        }) as i64;
    
    let len_i64 = len as i64;
    let adjusted = if idx < 0 { len_i64 + idx } else { idx };
    
    if adjusted < 0 || adjusted >= len_i64 {
        panic!(
            "Array index out of bounds: {} (length: {})",
            idx, len
        );
    }
    
    adjusted as usize
}

lazy_static::lazy_static! {
    pub static ref ARRAY_MODULE: StdModule = StdModule {
    name: "array",
    functions: {
        vec![
        StdFunction {
            name: "len",
            signature: FunctionSignature {
                params: vec![ValueKind::Array(Box::new(ValueKind::Any))],
                return_type: Box::new(ValueKind::Number),
            },
            arity: Arity::Fixed(1),
            impl_fn: Arc::new(|args, heap| {
                if let Some(arr) = args[0].as_array(heap) {
                    Value::number(arr.len() as f64)
                } else {
                    panic!("len expects array argument")
                }
            }),
        },
        StdFunction {
            name: "get",
            signature: FunctionSignature {
                params: vec![
                    ValueKind::Array(Box::new(ValueKind::Any)),
                    ValueKind::Number,
                ],
                return_type: Box::new(ValueKind::Any),
            },
            arity: Arity::Fixed(2),
            impl_fn: Arc::new(|args, heap| {
                if let Some(arr) = args[0].as_array(heap) {
                    let index = compute_index(&args[1], arr.len());
                    arr[index]
                } else {
                    panic!("get expects array as first argument")
                }
            }),
        },
        StdFunction {
            name: "set",
            signature: FunctionSignature {
                params: vec![
                    ValueKind::Array(Box::new(ValueKind::Any)),
                    ValueKind::Number,
                    ValueKind::Any,
                ],
                return_type: Box::new(ValueKind::Any),
            },
            arity: Arity::Fixed(3),
            impl_fn: Arc::new(|args, heap| {
                if let Some(arr) = args[0].as_array_mut(heap) {
                    let index = compute_index(&args[1], arr.len());
                    arr[index] = args[2];
                    args[2] // Return the value that was set
                } else {
                    panic!("set expects array as first argument");
                }
            }),
        },
        StdFunction {
            name: "slice",
            signature: FunctionSignature {
                params: vec![
                    ValueKind::Array(Box::new(ValueKind::Any)),
                    ValueKind::Number,
                    ValueKind::Number,
                ],
                return_type: Box::new(ValueKind::Array(Box::new(ValueKind::Any))),
            },
            arity: Arity::Fixed(3),
            impl_fn: Arc::new(|args, heap| {
                if let Some(arr) = args[0].as_array(heap) {
                    let arr_len = arr.len() as i64;
                    
                    // Compute start index
                    let start_num = args[1].as_number().unwrap_or_else(|| {
                        panic!("slice start index must be a number")
                    }) as i64;
                    let start_idx = if start_num < 0 {
                        (arr_len + start_num).max(0) as usize
                    } else {
                        start_num.min(arr_len) as usize
                    };
                    
                    // Compute end index
                    let end_num = args[2].as_number().unwrap_or_else(|| {
                        panic!("slice end index must be a number")
                    }) as i64;
                    let end_idx = if end_num < 0 {
                        (arr_len + end_num).max(0) as usize
                    } else {
                        end_num.min(arr_len) as usize
                    };
                    
                    // Extract slice
                    let result: Vec<Value> = arr[start_idx..end_idx].to_vec();
                    Value::array_with_heap(result, heap)
                } else {
                    panic!("slice expects array as first argument")
                }
            }),
        },
        StdFunction {
            name: "push",
            signature: FunctionSignature {
                params: vec![
                    ValueKind::Array(Box::new(ValueKind::Any)),
                    ValueKind::Any,
                ],
                return_type: Box::new(ValueKind::Array(Box::new(ValueKind::Any))),
            },
            arity: Arity::Fixed(2),
            impl_fn: Arc::new(|args, heap| {
                if let Some(arr) = args[0].as_array_mut(heap) {
                    arr.push(args[1]);
                    args[0] // Return the array (mutated in place)
                } else {
                    panic!("push expects array as first argument");
                }
            }),
        },
        StdFunction {
            name: "concat",
            signature: FunctionSignature {
                params: vec![
                    ValueKind::Array(Box::new(ValueKind::Any)),
                    ValueKind::Array(Box::new(ValueKind::Any)),
                ],
                return_type: Box::new(ValueKind::Array(Box::new(ValueKind::Any))),
            },
            arity: Arity::Fixed(2),
            impl_fn: Arc::new(|args, heap| {
                let arr1 = args[0].as_array(heap).unwrap_or_else(|| {
                    panic!("concat expects array as first argument")
                });
                let arr2 = args[1].as_array(heap).unwrap_or_else(|| {
                    panic!("concat expects array as second argument")
                });
                
                let mut result = Vec::with_capacity(arr1.len() + arr2.len());
                result.extend_from_slice(arr1);
                result.extend_from_slice(arr2);
                Value::array_with_heap(result, heap)
            }),
        },
        StdFunction {
            name: "range",
            signature: FunctionSignature {
                params: vec![
                    ValueKind::Number, ValueKind::Number
                ],
                return_type: Box::new(ValueKind::Array(Box::new(ValueKind::Number))),
            },
            arity: Arity::Fixed(2),
            impl_fn: Arc::new(|args, heap| {
                let min = args[0].as_number().expect("expected number as min boundary");
                let max = args[1].as_number().expect("expected number as max boundary");

                if min > max {
                    panic!("range: min must be <= max (got min={}, max={})", min, max);
                }
                
                let count = (max - min).max(0.0).ceil() as usize;
                let mut result = Vec::with_capacity(count);

                for i in 0..count {
                    result.push(Value::number(min + i as f64));
                }

                Value::array_with_heap(result, heap)
            }),
        },
        ]
    },
    structs: vec![],
    submodules: vec![],
    };
}
