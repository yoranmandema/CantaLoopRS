use std::collections::HashMap;

use crate::{
    bytecode::{emitter::ByteCodeEmitter, opcode::OpCode}, parser::parse_program, semantic_analyser::{FunctionSignature, HirBuilder, ValueKind},
    vm::{VM, Value},
};

pub struct BytecodeFunction {
    pub code: Vec<OpCode>,
    pub param_var_ids: Vec<u32>,
}

pub struct Engine {
    emitter: ByteCodeEmitter,
    hir_builder: HirBuilder,
    pub functions: HashMap<u32, Box<dyn Fn(&[String]) -> String + Send + Sync>>,
    pub bytecode_functions: HashMap<u32, BytecodeFunction>, // Function constant ID -> bytecode
}

impl Engine {
    pub fn new() -> Self {
        let emitter = ByteCodeEmitter::new();
        let hir_builder = HirBuilder::new();

        Self {
            hir_builder,
            emitter,
            functions: HashMap::new(),
            bytecode_functions: HashMap::new(),
        }
    }

    pub fn add_function<F>(&mut self, name: &str, signature: FunctionSignature, func: F)
    where
        F: Fn(&[String]) -> String + 'static + Send + Sync,
    {
        // Create a function ID - note: this should match the function registry
        // For built-in functions, we'll use a special ID range (e.g., starting from 10000)
        let id = 10000 + self.functions.len() as u32;

        self.functions.insert(id, Box::new(func));
        
        // Register the built-in function in the HIR builder's function registry
        self.hir_builder.register_builtin_function(name, signature, id);
    }

    pub fn get_constant(&self, id: u32) -> Value {
        let c = self
            .hir_builder
            .ast
            .constants
            .iter()
            .find(|c| c.id == id)
            .expect("Constant not found");

        // Constants no longer contain functions - only data
        match &c.kind {
            ValueKind::Number => {
                match &c.value {
                    crate::semantic_analyser::ConstantValue::Number(n) => Value::Number(*n),
                    _ => panic!("Constant number should have a value"),
                }
            },
            ValueKind::String => {
                match &c.value {
                    crate::semantic_analyser::ConstantValue::String(s) => Value::String(s.clone()),
                    _ => panic!("Constant string should have a value"),
                }
            },
            ValueKind::Boolean => {
                match &c.value {
                    crate::semantic_analyser::ConstantValue::Boolean(s) => Value::Boolean(*s),
                    _ => panic!("Constant boolean should have a value"),
                }
            },
            ValueKind::Unknown => panic!("Constant should not have Unknown kind"),
        }
    }
    
    pub fn get_function(&self, id: u32) -> Value {
        // Functions are now separate from constants
        Value::Function(id)
    }

    pub fn run(&mut self, input: &str) {
        let res = parse_program(&input).expect("Failed to parse program");

        println!("AST: {:#?}", res);


        let hir_ast = match self.hir_builder.build(res) {
            Ok(ast) => ast,
            Err(e) => {
                match e {
                    crate::semantic_analyser::HirError::TypeMismatch { variable, expected, actual } => {
                        let expected_str = match expected {
                            ValueKind::Number => "Number",
                            ValueKind::String => "String",
                            ValueKind::Boolean => "Boolean",
                            ValueKind::Unknown => "Unknown",
                        };
                        let actual_str = match actual {
                            ValueKind::Number => "Number",
                            ValueKind::String => "String",
                            ValueKind::Boolean => "Boolean",
                            ValueKind::Unknown => "Unknown",
                        };
                        panic!("Type mismatch error: Cannot assign {} to variable '{}' which is of type {}", actual_str, variable, expected_str);
                    }
                    crate::semantic_analyser::HirError::UnknownVariable(msg) => {
                        panic!("Semantic error: {}", msg);
                    }
                    crate::semantic_analyser::HirError::VariableAlreadyDeclared(msg) => {
                        panic!("Semantic error: {}", msg);
                    }
                    crate::semantic_analyser::HirError::NotImplemented => {
                        panic!("Semantic error: Feature not implemented");
                    }
                }
            }
        };

        println!("HIR AST: {:#?}", hir_ast);

        // Emit function bodies first
        // We need to temporarily borrow emitter separately to avoid multiple mutable borrows
        let function_ids: Vec<u32> = hir_ast.functions.keys().cloned().collect();
        for func_id in function_ids {
            let func = hir_ast.functions.get(&func_id).unwrap();
            let mut func_code = Vec::new();
            self.emitter.emit_block(&mut func_code, &func.definition.body, hir_ast);
            
            self.bytecode_functions.insert(
                func_id,
                BytecodeFunction {
                    code: func_code,
                    param_var_ids: func.definition.param_var_ids.clone(),
                },
            );
        }

        let emitted = &self.emitter.emit_program(hir_ast);

        for op in emitted {
            println!("{:?}", op);
        }

        println!("\nOutput:\n");

        let mut vm = VM::new(self, emitted.to_vec());

        vm.run();

        println!("\nProgram ran successfully!");
    }
}
