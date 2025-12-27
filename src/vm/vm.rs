use crate::{bytecode::opcode::OpCode, engine::Engine};

#[derive(Debug, Clone)]
pub enum Value {
    Number(f64),
    String(String),
    Boolean(bool),
    Function(u32), // function ID
    Thunk {
        func_id: u32,
        args: Vec<Value>,
    },
    None,
}

fn pop_n(stack: &mut Vec<Value>, n: usize) -> Vec<Value> {
    let mut args = Vec::with_capacity(n);
    for _ in 0..n {
        args.push(stack.pop().expect("Not enough arguments"));
    }
    args.reverse(); // if order matters
    args
}

struct CallFrame {
    code: Vec<OpCode>,            // Bytecode to execute (either top-level ops or function code)
    ip: usize,                    // Instruction pointer (current position in code)
    locals: Vec<Value>,           // Local variable slots (indexed by var_id)
    scope_id: u32,                // Scope identifier
}

pub struct VM<'a> {
    engine: &'a Engine,
    ops: Vec<OpCode>,
    stack: Vec<Value>,
    call_stack: Vec<CallFrame>,
}

impl<'a> VM<'a> {
    pub fn new(engine: &'a Engine, ops: Vec<OpCode>) -> Self {
        Self {
            engine,
            ops,
            stack: Vec::new(),
            call_stack: Vec::new(),
        }
    }


    pub fn run(&mut self) {
        // Initialize top-level frame
        self.call_stack.push(CallFrame {
            code: self.ops.clone(),
            ip: 0,
            locals: Vec::new(),
            scope_id: 0,
        });

        // Main execution loop - process the current frame
        while !self.call_stack.is_empty() {
            let frame_idx = self.call_stack.len() - 1;
            {
                let frame = &self.call_stack[frame_idx];
                if frame.ip >= frame.code.len() {
                    // Frame finished executing, pop it
                    self.call_stack.pop();
                    continue;
                }
            }

            // Get the opcode we need to execute (clone it to avoid borrow issues)
            let frame = &self.call_stack[frame_idx];
            let opcode = frame.code[frame.ip].clone();
            let mut should_increment = true;
            
            // Execute the opcode
            let mut should_continue_loop = false;
            match &opcode {
                OpCode::LdNum(n) => self.stack.push(Value::Number(*n)),
                OpCode::LdStr(s) => self.stack.push(Value::String(s.clone())),
                OpCode::LdVar(id) => {
                    let idx = *id as usize;
                    let frame = &self.call_stack[frame_idx];
                    let val = if idx < frame.locals.len() {
                        frame.locals[idx].clone()
                    } else {
                        Value::None
                    };
                    self.stack.push(val);
                }
                OpCode::StVar(id) => {
                    let val = self.stack.pop().expect("Stack underflow");
                    let idx = *id as usize;
                    let frame = &mut self.call_stack[frame_idx];
                    // Ensure locals vec is large enough
                    if idx >= frame.locals.len() {
                        frame.locals.resize(idx + 1, Value::None);
                    }
                    frame.locals[idx] = val;
                }
                OpCode::LdConst(id) => {
                    // load constant (data only, no functions)
                    let const_val = self.engine.get_constant(*id);
                    self.stack.push(const_val);
                }
                OpCode::LdFunc(id) => {
                    // load function reference
                    let func_val = self.engine.get_function(*id);
                    self.stack.push(func_val);
                }
                OpCode::Add => self.binary_add(),
                OpCode::Sub => self.binary_sub(),
                OpCode::Mul => self.binary_mul(),
                OpCode::Div => self.binary_div(),
                OpCode::Pow => self.binary_pow(),
                OpCode::Eq => self.comparison_eq(),
                OpCode::Ne => self.comparison_ne(),
                OpCode::Gt => self.comparison_gt(),
                OpCode::Lt => self.comparison_lt(),
                OpCode::Ge => self.comparison_ge(),
                OpCode::Le => self.comparison_le(),
                OpCode::And => self.logical_and(),
                OpCode::Or => self.logical_or(),
                OpCode::Neg => {
                    let v = self.stack.pop().expect("Stack underflow");
                    match v {
                        Value::Number(n) => self.stack.push(Value::Number(-n)),
                        _ => panic!("Negate non-number"),
                    }
                }
                OpCode::Not => {
                    let v = self.stack.pop().expect("Stack underflow");
                    match v {
                        Value::Number(n) => self.stack.push(Value::Number(if n == 0.0 { 1.0 } else { 0.0 })),
                        Value::Boolean(b) => self.stack.push(Value::Boolean(!b)),
                        _ => panic!("Not on non-number/non-boolean"),
                    }
                }
                OpCode::CallStack(n_args) => {
                    self.execute_call_stack(*n_args);
                },
                OpCode::Thunk(n_args) => {
                    self.execute_prepare_call(*n_args);
                },
                OpCode::Invoke => {
                    self.execute_invoke();
                },
                OpCode::Ret => {
                    self.execute_return();
                    // After return, we've popped the frame, restart loop to handle previous frame
                    // Skip IP increment
                    should_continue_loop = true;
                },
                OpCode::JmpIfFalse(offset) => {
                    let v = self.stack.pop().expect("Stack underflow");
                    let is_false = match v {
                        Value::Boolean(b) => !b,
                        Value::Number(n) => n == 0.0,
                        _ => false,
                    };
                    if is_false {
                        self.call_stack[frame_idx].ip = *offset;
                        should_increment = false;
                    }
                }
                OpCode::Jmp(offset) => {
                    self.call_stack[frame_idx].ip = *offset;
                    should_increment = false;
                }
                _ => unimplemented!("{:?}", opcode),
            }
            
            // If Ret was executed, continue loop without incrementing IP
            if should_continue_loop {
                continue;
            }
            
            if should_increment {
                self.call_stack[frame_idx].ip += 1;
            }
        }
    }

    fn binary_add(&mut self) {
        let b = self.stack.pop().expect("Stack underflow");
        let a = self.stack.pop().expect("Stack underflow");
        match (a, b) {
            (Value::Number(a_num), Value::Number(b_num)) => {
                self.stack.push(Value::Number(a_num + b_num));
            }
            // If either operand is a string or any other type, convert both to strings and concatenate
            (a, b) => {
                let mut result = Self::value_to_string(a);
                result.push_str(&Self::value_to_string(b));
                self.stack.push(Value::String(result));
            }
        }
    }

    fn binary_sub(&mut self) {
        let b = self.stack.pop().expect("Stack underflow");
        let a = self.stack.pop().expect("Stack underflow");
        match (a, b) {
            (Value::Number(a), Value::Number(b)) => self.stack.push(Value::Number(a - b)),
            _ => panic!("Subtract operation requires both operands to be numbers"),
        }
    }

    fn binary_mul(&mut self) {
        let b = self.stack.pop().expect("Stack underflow");
        let a = self.stack.pop().expect("Stack underflow");
        match (a, b) {
            (Value::Number(a), Value::Number(b)) => self.stack.push(Value::Number(a * b)),
            _ => panic!("Multiply operation requires both operands to be numbers"),
        }
    }

    fn binary_div(&mut self) {
        let b = self.stack.pop().expect("Stack underflow");
        let a = self.stack.pop().expect("Stack underflow");
        match (a, b) {
            (Value::Number(a), Value::Number(b)) => self.stack.push(Value::Number(a / b)),
            _ => panic!("Divide operation requires both operands to be numbers"),
        }
    }

    fn binary_pow(&mut self) {
        let b = self.stack.pop().expect("Stack underflow");
        let a = self.stack.pop().expect("Stack underflow");
        match (a, b) {
            (Value::Number(a), Value::Number(b)) => self.stack.push(Value::Number(a.powf(b))),
            _ => panic!("Power operation requires both operands to be numbers"),
        }
    }

    fn comparison_eq(&mut self) {
        let b = self.stack.pop().expect("Stack underflow");
        let a = self.stack.pop().expect("Stack underflow");
        let result = match (a, b) {
            (Value::Number(a), Value::Number(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Boolean(a), Value::Boolean(b)) => a == b,
            _ => panic!("Comparison == on incompatible types"),
        };
        self.stack.push(Value::Boolean(result));
    }

    fn comparison_ne(&mut self) {
        let b = self.stack.pop().expect("Stack underflow");
        let a = self.stack.pop().expect("Stack underflow");
        let result = match (a, b) {
            (Value::Number(a), Value::Number(b)) => a != b,
            (Value::String(a), Value::String(b)) => a != b,
            (Value::Boolean(a), Value::Boolean(b)) => a != b,
            _ => panic!("Comparison != on incompatible types"),
        };
        self.stack.push(Value::Boolean(result));
    }

    fn comparison_gt(&mut self) {
        let b = self.stack.pop().expect("Stack underflow");
        let a = self.stack.pop().expect("Stack underflow");
        let result = match (a, b) {
            (Value::Number(a), Value::Number(b)) => a > b,
            (Value::String(a), Value::String(b)) => a > b,
            (Value::Boolean(a), Value::Boolean(b)) => {
                let a_num = if a { 1.0 } else { 0.0 };
                let b_num = if b { 1.0 } else { 0.0 };
                a_num > b_num
            }
            _ => panic!("Comparison > on incompatible types"),
        };
        self.stack.push(Value::Boolean(result));
    }

    fn comparison_lt(&mut self) {
        let b = self.stack.pop().expect("Stack underflow");
        let a = self.stack.pop().expect("Stack underflow");
        let result = match (a, b) {
            (Value::Number(a), Value::Number(b)) => a < b,
            (Value::String(a), Value::String(b)) => a < b,
            (Value::Boolean(a), Value::Boolean(b)) => {
                let a_num = if a { 1.0 } else { 0.0 };
                let b_num = if b { 1.0 } else { 0.0 };
                a_num < b_num
            }
            _ => panic!("Comparison < on incompatible types"),
        };
        self.stack.push(Value::Boolean(result));
    }

    fn comparison_ge(&mut self) {
        let b = self.stack.pop().expect("Stack underflow");
        let a = self.stack.pop().expect("Stack underflow");
        let result = match (a, b) {
            (Value::Number(a), Value::Number(b)) => a >= b,
            (Value::String(a), Value::String(b)) => a >= b,
            (Value::Boolean(a), Value::Boolean(b)) => {
                let a_num = if a { 1.0 } else { 0.0 };
                let b_num = if b { 1.0 } else { 0.0 };
                a_num >= b_num
            }
            _ => panic!("Comparison >= on incompatible types"),
        };
        self.stack.push(Value::Boolean(result));
    }

    fn comparison_le(&mut self) {
        let b = self.stack.pop().expect("Stack underflow");
        let a = self.stack.pop().expect("Stack underflow");
        let result = match (a, b) {
            (Value::Number(a), Value::Number(b)) => a <= b,
            (Value::String(a), Value::String(b)) => a <= b,
            (Value::Boolean(a), Value::Boolean(b)) => {
                let a_num = if a { 1.0 } else { 0.0 };
                let b_num = if b { 1.0 } else { 0.0 };
                a_num <= b_num
            }
            _ => panic!("Comparison <= on incompatible types"),
        };
        self.stack.push(Value::Boolean(result));
    }

    fn to_bool(value: &Value) -> bool {
        match value {
            Value::Boolean(b) => *b,
            Value::Number(n) => *n != 0.0,
            _ => panic!("Cannot convert to boolean"),
        }
    }

    fn value_to_string(value: Value) -> String {
        match value {
            Value::String(s) => s,
            Value::Number(n) => n.to_string(),
            Value::Boolean(b) => b.to_string(),
            Value::Function(id) => format!("<function:{}>", id),
            Value::Thunk { .. } => "<prepared_call>".to_string(),
            Value::None => "None".to_string(),
        }
    }

    fn logical_and(&mut self) {
        let b = self.stack.pop().expect("Stack underflow");
        let a = self.stack.pop().expect("Stack underflow");
        let result = Self::to_bool(&a) && Self::to_bool(&b);
        self.stack.push(Value::Boolean(result));
    }

    fn logical_or(&mut self) {
        let b = self.stack.pop().expect("Stack underflow");
        let a = self.stack.pop().expect("Stack underflow");
        let result = Self::to_bool(&a) || Self::to_bool(&b);
        self.stack.push(Value::Boolean(result));
    }

    fn execute_call_stack(&mut self, n_args: u32) {
        let n_args = n_args as usize;
        // Pop function reference
        let func_val = self.stack.pop().expect("Stack underflow");
        let func_id = match func_val {
            Value::Function(id) => id,
            _ => panic!("Expected function on stack"),
        };

        // Pop arguments
        let args: Vec<Value> = pop_n(&mut self.stack, n_args);

        // Check if it's a native function or bytecode function
        if let Some(native_func) = self.engine.functions.get(&func_id) {
            // Native function: convert args to strings and call
            let args_str: Vec<String> = args
                .into_iter()
                .map(|v| match v {
                    Value::String(s) => s,
                    Value::Number(n) => n.to_string(),
                    Value::Function(id) => format!("<function:{}>", id),
                    Value::Boolean(v) => v.to_string(),
                    Value::Thunk { .. } => "<prepared_call>".to_string(),
                    Value::None => "None".to_string(),
                })
                .collect();
            let result = native_func(&args_str);
            self.stack.push(Value::String(result));
            // Native functions don't need frame management - they return immediately
        } else if let Some(bytecode_func) = self.engine.bytecode_functions.get(&func_id) {
            // Bytecode function: push new call frame
            // Current frame's ip is already incremented, so it will continue after this call
            
            // Determine the maximum var_id we'll need for locals
            let max_var_id = bytecode_func.param_var_ids.iter()
                .max()
                .copied()
                .unwrap_or(0);
            
            // Initialize locals vector with arguments bound to parameter slots
            let mut locals = vec![Value::None; (max_var_id + 1) as usize];
            for (i, param_var_id) in bytecode_func.param_var_ids.iter().enumerate() {
                if i < args.len() {
                    locals[*param_var_id as usize] = args[i].clone();
                }
            }

            // Push new frame with function bytecode
            self.call_stack.push(CallFrame {
                code: bytecode_func.code.clone(),
                ip: 0,  // Start at beginning of function
                locals,
                scope_id: func_id, // Use func_id as scope_id for now
            });
            // Execution will continue in the main loop with the new frame
        } else {
            panic!("Function {} not found (neither native nor bytecode)", func_id);
        }
    }

    fn execute_return(&mut self) {
        // Pop return value (or use None if stack is empty)
        let return_value = self.stack.pop().unwrap_or(Value::None);

        // Pop the current frame (this removes it from call_stack)
        self.call_stack.pop();

        // Push return value back on stack
        self.stack.push(return_value);
        // Execution will continue in the main loop with the previous frame
    }

    fn execute_prepare_call(&mut self, n_args: u32) {
        let n_args = n_args as usize;
        
        // Pop function reference
        let func_val = self.stack.pop().expect("Stack underflow");
        let func_id = match func_val {
            Value::Function(id) => id,
            _ => panic!("Expected function on stack for Thunk"),
        };

        // Pop arguments (they're on the stack in order, so we need to reverse them)
        let args: Vec<Value> = pop_n(&mut self.stack, n_args);

        // Create a Thunk value
        let prepared_call = Value::Thunk {
            func_id,
            args,
        };

        // Push the prepared call onto the stack
        self.stack.push(prepared_call);
    }

    fn execute_invoke(&mut self) {
        // Pop the prepared call from the stack
        // Note: For currying (e.g., add5!(10)), extra arguments may be on the stack
        // before the Thunk. We need to check if we need more args and pop them.
        let call = self.stack.pop().expect("Expected prepared call on stack");

        let (func_id, mut args) = match call {
            Value::Thunk { func_id, args } => (func_id, args),
            _ => panic!("Invoke expects Thunk value, got {:?}", call),
        };

        // Get the required number of parameters for this function
        let required_params = if self.engine.functions.contains_key(&func_id) {
            // For native functions, we don't know the param count statically
            // But we should use the number of args already in the Thunk
            // Native functions should not consume extra stack values - they get exactly what's in the Thunk
            args.len()
        } else if let Some(bytecode_func) = self.engine.bytecode_functions.get(&func_id) {
            bytecode_func.param_var_ids.len()
        } else {
            panic!("Function {} not found (neither native nor bytecode)", func_id);
        };

        // Check if we need more arguments and pop them from the stack
        // This handles currying: add5!(10) where add5=Thunk{func_id, args=[5]}
        // The bytecode for add5!(10) should push 10, then the Thunk, then Invoke
        // So when we get here, 10 should be on the stack (below the Thunk we just popped)
        // Note: Arguments on stack are in reverse order (last pushed is first popped)
        // So we need to collect them and then reverse to get correct order
        // IMPORTANT: For native functions, we should NOT pop extra arguments from the stack
        // because they don't support currying - they get exactly what's in the Thunk
        let mut extra_args = Vec::new();
        let is_native = self.engine.functions.contains_key(&func_id);
        if !is_native {
            // Only bytecode functions support currying (extra arguments)
            while args.len() + extra_args.len() < required_params {
                if self.stack.is_empty() {
                    // Not enough arguments available, create a new Thunk (still partial)
                    // Combine existing args with any extra args we collected
                    args.extend(extra_args);
                    self.stack.push(Value::Thunk {
                        func_id,
                        args,
                    });
                    return;
                }
                // Pop an additional argument from the stack
                extra_args.push(self.stack.pop().unwrap());
            }
        }
        // Reverse extra_args to get correct order (stack is LIFO)
        extra_args.reverse();
        // Append extra args to existing args
        args.extend(extra_args);

        // Ensure we have enough arguments before invoking
        if args.len() < required_params {
            // Still not enough args, create a new Thunk (shouldn't happen here, but be safe)
            self.stack.push(Value::Thunk {
                func_id,
                args,
            });
            return;
        }

        // Now invoke the function with the complete set of arguments
        self.invoke_function(func_id, args);
    }

    fn invoke_function(&mut self, func_id: u32, args: Vec<Value>) {
        // Safety check: ensure we have enough arguments for bytecode functions
        if let Some(bytecode_func) = self.engine.bytecode_functions.get(&func_id) {
            let required_params = bytecode_func.param_var_ids.len();
            if args.len() < required_params {
                panic!("Attempted to invoke function {} with {} args but it requires {}", 
                    func_id, args.len(), required_params);
            }
        }
        
        // Check if it's a native function or bytecode function
        if let Some(native_func) = self.engine.functions.get(&func_id) {
            // Native function: convert args to strings and call
            let args_str: Vec<String> = args
                .into_iter()
                .map(|v| match v {
                    Value::String(s) => s,
                    Value::Number(n) => n.to_string(),
                    Value::Function(id) => format!("<function:{}>", id),
                    Value::Boolean(v) => v.to_string(),
                    Value::Thunk { .. } => "<prepared_call>".to_string(),
                    Value::None => "None".to_string(),
                })
                .collect();
            let result = native_func(&args_str);
            self.stack.push(Value::String(result));
        } else if let Some(bytecode_func) = self.engine.bytecode_functions.get(&func_id) {
            // Bytecode function: push new call frame
            
            // Determine the maximum var_id we'll need for locals
            let max_var_id = bytecode_func.param_var_ids.iter()
                .max()
                .copied()
                .unwrap_or(0);
            
            // Initialize locals vector with arguments bound to parameter slots
            let mut locals = vec![Value::None; (max_var_id + 1) as usize];
            for (i, param_var_id) in bytecode_func.param_var_ids.iter().enumerate() {
                if i < args.len() {
                    locals[*param_var_id as usize] = args[i].clone();
                }
            }

            // Push new frame with function bytecode
            self.call_stack.push(CallFrame {
                code: bytecode_func.code.clone(),
                ip: 0,  // Start at beginning of function
                locals,
                scope_id: func_id, // Use func_id as scope_id for now
            });
            // Execution will continue in the main loop with the new frame
        } else {
            panic!("Function {} not found (neither native nor bytecode)", func_id);
        }
    }
}
