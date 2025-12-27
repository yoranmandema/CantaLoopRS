use crate::{
    ast::{BinaryOp, UnaryOp},
    bytecode::opcode::OpCode,
    semantic_analyser::{HirAst, HirBlock, HirExpression, HirStmt},
};

pub struct ByteCodeEmitter {}

impl ByteCodeEmitter {
    pub fn new() -> Self {
        Self {}
    }

    /*
        Technically 'programs' could have dependencies, multiple files etc.

        For now we assume a program only has one scope
    */
    pub fn emit_program(&self, program: &HirAst) -> Vec<OpCode> {
        let mut ops: Vec<OpCode> = Vec::new();
        for block in &program.blocks {
            self.emit_block(&mut ops, block, program);
        }
        ops
    }

    // Emits bytecode for a block
    pub(crate) fn emit_block(&self, ops: &mut Vec<OpCode>, block: &HirBlock, program: &HirAst) {
        for statement in &block.statements {
            self.emit_statement(ops, statement, program);
        }
    }

    pub fn emit_statement(&self, ops: &mut Vec<OpCode>, statement: &HirStmt, program: &HirAst) {
        match statement {
            HirStmt::Assign { slot, value } => {
                self.emit_expression(ops, value, program);
                ops.push(OpCode::StVar(*slot));
            }
            HirStmt::AssignIncrement { slot, value } => {
                // Load current value of variable
                ops.push(OpCode::LdVar(*slot));
                // Evaluate the increment value expression
                self.emit_expression(ops, value, program);
                // Add them
                ops.push(OpCode::Add);
                // Store result back
                ops.push(OpCode::StVar(*slot));
            }
            HirStmt::AssignDecrement { slot, value } => {
                // Load current value of variable
                ops.push(OpCode::LdVar(*slot));
                // Evaluate the decrement value expression
                self.emit_expression(ops, value, program);
                // Subtract the value
                ops.push(OpCode::Sub);
                // Store result back
                ops.push(OpCode::StVar(*slot));
            }
            HirStmt::If { arms, else_block } => {
                let mut jump_to_end_positions = Vec::new();
                let mut jmp_if_false_positions = Vec::new();
                let mut condition_start_positions = Vec::new();
                
                // Emit each arm
                for (condition, block) in arms {
                    // Record where this condition starts
                    condition_start_positions.push(ops.len());
                    
                    // Emit condition expression
                    self.emit_expression(ops, condition, program);
                    
                    // Emit conditional jump to skip this block if false
                    // We'll patch this jump target after we know where the next arm/else starts
                    jmp_if_false_positions.push(ops.len());
                    ops.push(OpCode::JmpIfFalse(0)); // Placeholder
                    
                    // Emit the block
                    self.emit_block(ops, block, program);
                    
                    // Emit unconditional jump to end (skip remaining arms and else)
                    // We'll patch this after we know where the end is
                    jump_to_end_positions.push(ops.len());
                    ops.push(OpCode::Jmp(0)); // Placeholder
                }
                
                // Patch JmpIfFalse instructions: each should jump to the next condition or else block
                let else_block_start = ops.len();
                for (i, &jmp_pos) in jmp_if_false_positions.iter().enumerate() {
                    let target = if i + 1 < condition_start_positions.len() {
                        // Jump to next arm's condition
                        condition_start_positions[i + 1]
                    } else {
                        // Jump to else block (or end if no else)
                        else_block_start
                    };
                    ops[jmp_pos] = OpCode::JmpIfFalse(target);
                }
                
                // Emit else block if it has statements
                if !else_block.statements.is_empty() {
                    self.emit_block(ops, else_block, program);
                }
                let end_pos = ops.len();
                
                // Patch all jumps to end
                for &jmp_pos in &jump_to_end_positions {
                    ops[jmp_pos] = OpCode::Jmp(end_pos);
                }
            }
            HirStmt::Return { value } => {
                // Emit the return value expression
                self.emit_expression(ops, value, program);
                // Emit return opcode
                ops.push(OpCode::Ret);
            }
            HirStmt::Expression(expr) => {
                self.emit_expression(ops, expr, program);
                // Optionally pop the value if not used
            }
        }        
    }

    fn emit_expression(&self, ops: &mut Vec<OpCode>, expr: &HirExpression, program: &HirAst) {
        match expr {
            HirExpression::Number(n) => ops.push(OpCode::LdNum(*n)),
            HirExpression::String(s) => ops.push(OpCode::LdStr(s.clone())),
            HirExpression::Identifier(slot) => ops.push(OpCode::LdVar(*slot)),
            HirExpression::Constant(id) => ops.push(OpCode::LdConst(*id)),
            HirExpression::Binary { lhs, rhs, operator } => {
                self.emit_expression(ops, lhs, program);
                self.emit_expression(ops, rhs, program);
                match operator {
                    BinaryOp::Add => ops.push(OpCode::Add),
                    BinaryOp::Sub => ops.push(OpCode::Sub),
                    BinaryOp::Mul => ops.push(OpCode::Mul),
                    BinaryOp::Div => ops.push(OpCode::Div),
                    BinaryOp::Pow => ops.push(OpCode::Pow),
                    BinaryOp::Eq => ops.push(OpCode::Eq),
                    BinaryOp::Ne => ops.push(OpCode::Ne),
                    BinaryOp::Gt => ops.push(OpCode::Gt),
                    BinaryOp::Lt => ops.push(OpCode::Lt),
                    BinaryOp::Ge => ops.push(OpCode::Ge),
                    BinaryOp::Le => ops.push(OpCode::Le),
                    BinaryOp::And => ops.push(OpCode::And),
                    BinaryOp::Or => ops.push(OpCode::Or),
                }
            }
            HirExpression::Unary { operand, operator } => {
                self.emit_expression(ops, operand, program);
                match operator {
                    UnaryOp::Neg => ops.push(OpCode::Neg),
                    UnaryOp::Not => ops.push(OpCode::Not),                    
                    _ => todo!(),
                }
            }
            HirExpression::FunctionCall { function_id, args, invoke } => {
                // Push arguments first (they'll be on the bottom of the stack)
                for arg in args {
                    self.emit_expression(ops, arg, program);
                }
            
                // Push function ID as a constant-like value
                // The VM will handle loading the function from the functions registry
                // Since functions are now separate, we need to load the function value
                ops.push(OpCode::LdFunc(*function_id));
            
                // Check if we can collapse the thunk at compile time
                // Thunk is only needed when arg_count < param_count (partial application)
                // If arg_count == param_count and invoke == true, use direct CallStack
                let arg_count = args.len();
                let param_count = program.functions.get(function_id)
                    .and_then(|func| {
                        // For user-defined functions, use param_var_ids length
                        if !func.definition.param_var_ids.is_empty() {
                            Some(func.definition.param_var_ids.len())
                        } else if !func.signature.params.is_empty() {
                            // For built-in functions, use signature params length
                            // (built-ins have empty param_var_ids but may have signature params)
                            Some(func.signature.params.len())
                        } else {
                            None // Can't determine param count
                        }
                    });
                
                let can_collapse = param_count
                    .map(|param_count| arg_count == param_count && *invoke)
                    .unwrap_or(false);
                
                if can_collapse {
                    // Direct call: arg count matches param count, invoke immediately
                    ops.push(OpCode::CallStack(arg_count as u32));
                } else if *invoke {
                    // Invoke immediately but needs thunk (partial application or unknown params)
                    ops.push(OpCode::Thunk(arg_count as u32));
                    ops.push(OpCode::Invoke);
                } else {
                    // Just prepare the call (don't invoke) - thunk needed for partial application
                    ops.push(OpCode::Thunk(arg_count as u32));
                }
            }
            HirExpression::PostfixInvoke { operand, args } => {
                // If there are additional arguments, emit them first (they'll be on the stack before the PreparedCall)
                if let Some(arg_list) = args {
                    for arg in arg_list {
                        self.emit_expression(ops, arg, program);
                    }
                }
                // Emit the operand (which should be a PreparedCall value at runtime)
                self.emit_expression(ops, operand, program);
                // Invoke the prepared call (VM will pop the PreparedCall and any extra args from the stack)
                ops.push(OpCode::Invoke);
            }
        }
    }
    
}
