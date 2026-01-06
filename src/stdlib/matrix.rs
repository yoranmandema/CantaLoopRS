use crate::core::engine::{Arity, StdFunction, StdModule, StdStruct};
use crate::core::hir_lowering::{FunctionSignature, ValueKind};
use crate::core::vm::Value;
use std::sync::Arc;

/// Standard library matrix module.
///
/// This is pure declarative metadata describing the math module.
/// It does not mutate the Engine - it's compiler input, not runtime behavior.

// Type ID for Matrix4 struct (computed using stable hash function)
// This matches the type_id computation used in bytecode emitter and VM
fn matrix4_type_id() -> u32 {
    crate::core::vm::compute_struct_type_id("Matrix4")
}

lazy_static::lazy_static! {
    pub static ref MATRIX_MODULE: StdModule = StdModule {
    name: "matrix",
    functions: {
        vec![
            StdFunction {
                name: "matrix4",
                signature: FunctionSignature {
                    params: vec![
                        ValueKind::Number, ValueKind::Number,
                        ValueKind::Number, ValueKind::Number,
                    ],
                return_type: Box::new(ValueKind::Struct("Matrix4".into())),
                is_effectful: false,
            },
                arity: Arity::Fixed(4),
                impl_fn: Arc::new(|args, heap| {
                    Value::struct_with_heap(
                        matrix4_type_id(),
                        vec![
                            args[0].clone(),
                            args[1].clone(),
                            args[2].clone(),
                            args[3].clone(),
                        ],
                        heap,
                    )
                }),
            },
            StdFunction {
                name: "identity",
                signature: FunctionSignature {
                    params: vec![],
                return_type: Box::new(ValueKind::Struct("Matrix4".into())),
                is_effectful: false,
            },
                arity: Arity::Fixed(0),
                impl_fn: Arc::new(|_, heap| {
                    Value::struct_with_heap(
                        matrix4_type_id(),
                        vec![
                            Value::number(1.0),
                            Value::number(0.0),
                            Value::number(0.0),
                            Value::number(1.0),
                        ],
                        heap,
                    )
                }),
            },
            StdFunction {
                name: "add",
                signature: FunctionSignature {
                    params: vec![
                        ValueKind::Struct("Matrix4".into()),
                        ValueKind::Struct("Matrix4".into()),
                    ],
                return_type: Box::new(ValueKind::Struct("Matrix4".into())),
                is_effectful: false,
            },
                arity: Arity::Fixed(2),
                impl_fn: Arc::new(|args, heap| {
                    let a = args[0].as_struct(heap).unwrap();
                    let b = args[1].as_struct(heap).unwrap();
            
                    let values = a.fields.iter()
                        .zip(b.fields.iter())
                        .map(|(x, y)| Value::number(
                            x.as_number().unwrap() + y.as_number().unwrap()
                        ))
                        .collect();
            
                    Value::struct_with_heap(matrix4_type_id(), values, heap)
                }),
            },

            StdFunction {
                name: "scale",
                signature: FunctionSignature {
                    params: vec![
                        ValueKind::Struct("Matrix4".into()),
                        ValueKind::Number,
                    ],
                return_type: Box::new(ValueKind::Struct("Matrix4".into())),
                is_effectful: false,
            },
                arity: Arity::Fixed(2),
                impl_fn: Arc::new(|args, heap| {
                    let m = args[0].as_struct(heap).unwrap();
                    let s = args[1].as_number().unwrap();
            
                    let values = m.fields.iter()
                        .map(|v| Value::number(v.as_number().unwrap() * s))
                        .collect();
            
                    Value::struct_with_heap(matrix4_type_id(), values, heap)
                }),
            },
            StdFunction {
                name: "get",
                signature: FunctionSignature {
                    params: vec![
                        ValueKind::Struct("Matrix4".into()),
                        ValueKind::String,
                    ],
                return_type: Box::new(ValueKind::Number),
                is_effectful: false,
            },
                arity: Arity::Fixed(2),
                impl_fn: Arc::new(|args, heap| {
                    let m = args[0].as_struct(heap).unwrap();
                    let field = args[1].as_string(heap).unwrap();
            
                    let idx = match field.as_str() {
                        "x" => 0,
                        "y" => 1,
                        "z" => 2,
                        "w" => 3,
                        _ => panic!("unknown Matrix4 field"),
                    };
            
                    m.fields[idx].clone()
                }),
            }
        ]
    },
    structs: vec![
        StdStruct {
            name: "Matrix4",
            fields: vec![
                ("x", ValueKind::Number),
                ("y", ValueKind::Number),
                ("z", ValueKind::Number),
                ("w", ValueKind::Number),
            ],
            methods: vec![],
        }
    ],
    submodules: vec![],
    };
}
