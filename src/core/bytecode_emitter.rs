use crate::core::{
    ast::{BinaryOp, UnaryOp},
    bytecode::OpCode,
    hir_lowering::{HirAst, HirBlock, HirExpression, HirStmt, ValueKind},
};

pub struct ByteCodeEmitter {
    loop_stack: Vec<LoopInfo>, // Loop boundaries for nested loops
}

struct LoopInfo {
    start: usize,
    end: usize,
    break_positions: Vec<usize>, // Positions of break jumps to patch
    break_slot: Option<u32>,      // Variable slot for break value (None for statement loops, Some(slot) for expression loops)
}

impl ByteCodeEmitter {
    pub fn new() -> Self {
        Self {
            loop_stack: Vec::new(),
        }
    }

    /// Check if an expression is statically known to be a number type
    /// Check if an expression returns void (no value)
    /// Functions without a return type are considered void (Unknown return type)
    fn is_void_expression(&self, expr: &HirExpression, program: &HirAst) -> bool {
        match expr {
            HirExpression::FunctionCall { function_id, invoke, .. } => {
                if *invoke {
                    // Check if the function returns void (Unknown return type)
                    if let Some(func) = program.functions.get(function_id) {
                        matches!(*func.signature.return_type, ValueKind::Unknown)
                    } else {
                        false // Unknown function, assume non-void to be safe
                    }
                } else {
                    false // Thunk is not void
                }
            }
            HirExpression::PostfixInvoke { operand, .. } => {
                // Check if the operand is a function call that returns void
                if let HirExpression::FunctionCall { function_id, .. } = operand.as_ref() {
                    if let Some(func) = program.functions.get(function_id) {
                        matches!(*func.signature.return_type, ValueKind::Unknown)
                    } else {
                        false
                    }
                } else if let HirExpression::Identifier(var_id) = operand.as_ref() {
                    // Check if the variable contains a void thunk by looking up its type
                    // Search through all scopes to find the variable
                    for scope in &program.scopes.scopes {
                        if let Some(v) = scope.vars.iter().find(|v| v.id == *var_id) {
                            // Check if the variable is a thunk type with void return type
                            if let ValueKind::Thunk(type_str) = &v.kind {
                                // Extract return type from thunk type string (format: "param_type ~> return_type")
                                // For void functions, this will be "unknown ~> unknown"
                                if let Some(pos) = type_str.find("~>") {
                                    let return_type_str = type_str[pos + 2..].trim();
                                    // Check if return type is "unknown" (void)
                                    if return_type_str.eq_ignore_ascii_case("unknown") {
                                        return true;
                                    }
                                }
                            }
                        }
                    }
                    false
                } else {
                    false
                }
            }
            _ => false, // Other expressions return values
        }
    }

    fn is_statically_number(&self, expr: &HirExpression, program: &HirAst) -> bool {
        match expr {
            HirExpression::Number(_) => true,
            HirExpression::Identifier(var_id) => {
                // Check if variable is statically known to be a number
                // Search through all scopes to find the variable
                for scope in &program.scopes.scopes {
                    if let Some(v) = scope.vars.iter().find(|v| v.id == *var_id) {
                        return matches!(v.kind, ValueKind::Number);
                    }
                }
                false
            }
            HirExpression::Constant(const_id) => {
                // Check if constant is a number
                if let Some(const_val) = program.constants.iter().find(|c| c.id == *const_id) {
                    matches!(const_val.kind, ValueKind::Number)
                } else {
                    false
                }
            }
            HirExpression::Binary { lhs, rhs, operator } => {
                // For binary ops, check if both operands are numbers and the operator is numeric
                match operator {
                    BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Pow => {
                        self.is_statically_number(lhs, program) && self.is_statically_number(rhs, program)
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    /*
        Technically 'programs' could have dependencies, multiple files etc.

        For now we assume a program only has one scope
    */
    pub fn emit_program(&mut self, program: &HirAst) -> Vec<OpCode> {
        let mut ops: Vec<OpCode> = Vec::new();
        for block in &program.blocks {
            self.emit_block(&mut ops, block, program);
        }
        ops
    }

    // Emits bytecode for a block
    pub(crate) fn emit_block(&mut self, ops: &mut Vec<OpCode>, block: &HirBlock, program: &HirAst) {
        for statement in &block.statements {
            self.emit_statement(ops, statement, program);
        }
    }

    pub fn emit_statement(&mut self, ops: &mut Vec<OpCode>, statement: &HirStmt, program: &HirAst) {
        match statement {
            HirStmt::Assign { slot, value } => {
                self.emit_stmt_assign(ops, *slot, value, program);
            }
            HirStmt::AssignIncrement { slot, value } => {
                self.emit_stmt_assign_increment(ops, *slot, value, program);
            }
            HirStmt::AssignDecrement { slot, value } => {
                self.emit_stmt_assign_decrement(ops, *slot, value, program);
            }
            HirStmt::If { arms, else_block } => {
                self.emit_stmt_if(ops, arms, else_block, program);
            }
            HirStmt::Match { expression, cases } => {
                self.emit_stmt_match(ops, expression, cases, program);
            }
            HirStmt::Return { value } => {
                self.emit_stmt_return(ops, value, program);
            }
            HirStmt::Loop { init_vars, body, break_slot } => {
                self.emit_stmt_loop(ops, init_vars, body, *break_slot, program);
            }
            HirStmt::Break { value } => {
                self.emit_stmt_break(ops, value, program);
            }
            HirStmt::Continue => {
                self.emit_stmt_continue(ops);
            }
            HirStmt::Expression(expr) => {
                self.emit_stmt_expression(ops, expr, program);
            }
            HirStmt::Nop => {
                // No-op statement (used for use statements which are compile-time only)
            }
        }        
    }

    // Statement emission helpers

    fn emit_stmt_assign(&mut self, ops: &mut Vec<OpCode>, slot: u32, value: &HirExpression, program: &HirAst) {
        self.emit_expression(ops, value, program);
        ops.push(OpCode::StVar(slot));
    }

    fn emit_stmt_assign_increment(&mut self, ops: &mut Vec<OpCode>, slot: u32, value: &HirExpression, program: &HirAst) {
        ops.push(OpCode::LdVar(slot));
        self.emit_expression(ops, value, program);
        ops.push(OpCode::Add);
        ops.push(OpCode::StVar(slot));
    }

    fn emit_stmt_assign_decrement(&mut self, ops: &mut Vec<OpCode>, slot: u32, value: &HirExpression, program: &HirAst) {
        ops.push(OpCode::LdVar(slot));
        self.emit_expression(ops, value, program);
        ops.push(OpCode::Sub);
        ops.push(OpCode::StVar(slot));
    }

    fn emit_stmt_if(&mut self, ops: &mut Vec<OpCode>, arms: &[(HirExpression, HirBlock)], else_block: &HirBlock, program: &HirAst) {
        let mut jump_to_end_positions = Vec::new();
        let mut jmp_if_false_positions = Vec::new();
        let mut condition_start_positions = Vec::new();
        
        // Emit each arm
        for (condition, block) in arms {
            condition_start_positions.push(ops.len());
            self.emit_expression(ops, condition, program);
            jmp_if_false_positions.push(ops.len());
            ops.push(OpCode::JmpIfFalse(0)); // Placeholder
            self.emit_block(ops, block, program);
            jump_to_end_positions.push(ops.len());
            ops.push(OpCode::Jmp(0)); // Placeholder
        }
        
        // Patch JmpIfFalse instructions: each should jump to the next condition or else block
        let else_block_start = ops.len();
        for (i, &jmp_pos) in jmp_if_false_positions.iter().enumerate() {
            let target = if i + 1 < condition_start_positions.len() {
                condition_start_positions[i + 1]
            } else {
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

    fn emit_stmt_match(&mut self, ops: &mut Vec<OpCode>, expression: &HirExpression, cases: &[(Option<HirExpression>, HirBlock)], program: &HirAst) {
        // Emit the match expression (evaluate it once)
        self.emit_expression(ops, expression, program);
        // Store it in a temporary variable slot to avoid re-evaluating it
        let temp_slot = 999999u32;
        ops.push(OpCode::StVar(temp_slot));
        
        let mut jump_to_end_positions = Vec::new();
        let mut jmp_if_false_info = Vec::new(); // (jmp_pos, case_index)
        let mut case_block_starts = Vec::new();
        
        // Emit each case
        for (case_idx, (pattern, block)) in cases.iter().enumerate() {
            if let Some(pattern_expr) = pattern {
                ops.push(OpCode::LdVar(temp_slot));
                self.emit_pattern_expression(ops, pattern_expr, expression, program);
                jmp_if_false_info.push((ops.len(), case_idx));
                ops.push(OpCode::JmpIfFalse(0)); // Placeholder
            }
            
            case_block_starts.push(ops.len());
            self.emit_block(ops, block, program);
            jump_to_end_positions.push(ops.len());
            ops.push(OpCode::Jmp(0)); // Placeholder
        }
        
        // Patch JmpIfFalse instructions
        let end_pos = ops.len();
        for (jmp_pos, case_idx) in jmp_if_false_info {
            let target = if case_idx + 1 < case_block_starts.len() {
                case_block_starts[case_idx + 1]
            } else {
                end_pos
            };
            ops[jmp_pos] = OpCode::JmpIfFalse(target);
        }
        
        // Patch all jumps to end
        for &jmp_pos in &jump_to_end_positions {
            ops[jmp_pos] = OpCode::Jmp(end_pos);
        }
    }

    fn emit_stmt_return(&mut self, ops: &mut Vec<OpCode>, value: &HirExpression, program: &HirAst) {
        // Check if this is a tail call: return f(x)!
        if let HirExpression::FunctionCall { function_id, args, invoke: true } = value {
            // Tail call - emit optimized RetInvoke
            for arg in args {
                self.emit_expression(ops, arg, program);
            }
            ops.push(OpCode::LdFunc(*function_id));
            let arg_count = args.len() as u32;
            ops.push(OpCode::Thunk(arg_count));
            ops.push(OpCode::RetInvoke);
        } else {
            // Normal return
            self.emit_expression(ops, value, program);
            ops.push(OpCode::Ret);
        }
    }

    fn emit_stmt_loop(&mut self, ops: &mut Vec<OpCode>, init_vars: &[(u32, HirExpression)], body: &HirBlock, break_slot: Option<u32>, program: &HirAst) {
        // Emit initialization code for loop variables
        for (var_id, init_expr) in init_vars {
            self.emit_expression(ops, init_expr, program);
            ops.push(OpCode::StVar(*var_id));
        }
        
        let loop_start = ops.len();
        
        self.loop_stack.push(LoopInfo {
            start: loop_start,
            end: 0, // Will be patched later
            break_positions: Vec::new(),
            break_slot,
        });
        
        // OPTIMIZATION: Detect pattern "if (condition) { break [value]; }" at start of loop
        let (condition_opt, break_value_opt, remaining_body) = if let Some(first_stmt) = body.statements.first() {
            if let HirStmt::If { arms, else_block } = first_stmt {
                if arms.len() == 1 
                    && else_block.statements.is_empty()
                    && arms[0].1.statements.len() == 1
                    && matches!(&arms[0].1.statements[0], HirStmt::Break { .. }) {
                    let condition = arms[0].0.clone();
                    let break_value = if let HirStmt::Break { value } = &arms[0].1.statements[0] {
                        value.clone()
                    } else {
                        None
                    };
                    let remaining: Vec<_> = body.statements[1..].to_vec();
                    (Some(condition), break_value, remaining)
                } else {
                    (None, None, body.statements.clone())
                }
            } else {
                (None, None, body.statements.clone())
            }
        } else {
            (None, None, body.statements.clone())
        };
        
        if let Some(condition) = condition_opt {
            // Optimized pattern
            self.emit_expression(ops, &condition, program);
            let jmp_if_true_pos = ops.len();
            ops.push(OpCode::JmpIfTrue(0)); // Placeholder
            
            for stmt in &remaining_body {
                self.emit_statement(ops, stmt, program);
            }
            ops.push(OpCode::Jmp(loop_start));
            
            let break_handler_start = ops.len();
            if let Some(break_value) = break_value_opt {
                self.emit_expression(ops, &break_value, program);
                if let Some(loop_info) = self.loop_stack.last() {
                    if let Some(break_slot) = loop_info.break_slot {
                        ops.push(OpCode::StVar(break_slot));
                    }
                }
            }
            let break_handler_end = ops.len();
            ops.push(OpCode::Jmp(0)); // Placeholder
            
            let loop_end = ops.len();
            ops[jmp_if_true_pos] = OpCode::JmpIfTrue(break_handler_start);
            ops[break_handler_end] = OpCode::Jmp(loop_end);
            
            let break_slot_val = if let Some(loop_info) = self.loop_stack.last_mut() {
                loop_info.end = loop_end;
                for &break_pos in &loop_info.break_positions {
                    ops[break_pos] = OpCode::Jmp(loop_end);
                }
                loop_info.break_slot
            } else {
                None
            };
            
            if let Some(slot) = break_slot_val {
                ops.push(OpCode::LdVar(slot));
            }
        } else {
            // Normal path
            self.emit_block(ops, body, program);
            ops.push(OpCode::Jmp(loop_start));
            
            let loop_end = ops.len();
            let break_slot_val = if let Some(loop_info) = self.loop_stack.last_mut() {
                loop_info.end = loop_end;
                for &break_pos in &loop_info.break_positions {
                    ops[break_pos] = OpCode::Jmp(loop_end);
                }
                loop_info.break_slot
            } else {
                None
            };
            
            if let Some(slot) = break_slot_val {
                ops.push(OpCode::LdVar(slot));
            }
        }
        
        self.loop_stack.pop();
    }

    fn emit_stmt_break(&mut self, ops: &mut Vec<OpCode>, value: &Option<HirExpression>, program: &HirAst) {
        if let Some(expr) = value {
            self.emit_expression(ops, expr, program);
            if let Some(loop_info) = self.loop_stack.last() {
                if let Some(break_slot) = loop_info.break_slot {
                    ops.push(OpCode::StVar(break_slot));
                }
            }
        }
        
        if let Some(loop_info) = self.loop_stack.last_mut() {
            let break_pos = ops.len();
            ops.push(OpCode::Jmp(0)); // Placeholder
            loop_info.break_positions.push(break_pos);
        } else {
            panic!("break statement outside of loop");
        }
    }

    fn emit_stmt_continue(&mut self, ops: &mut Vec<OpCode>) {
        if let Some(loop_info) = self.loop_stack.last() {
            ops.push(OpCode::Jmp(loop_info.start));
        } else {
            panic!("continue statement outside of loop");
        }
    }

    fn emit_stmt_expression(&mut self, ops: &mut Vec<OpCode>, expr: &HirExpression, program: &HirAst) {
        // Skip dummy constants (used for function declarations which don't need bytecode)
        if let HirExpression::Constant(0) = expr {
            return;
        }
        self.emit_expression(ops, expr, program);
        if !self.is_void_expression(expr, program) {
            ops.push(OpCode::Pop);
        }
    }

    // Expression emission helpers

    fn emit_expr_number(&mut self, ops: &mut Vec<OpCode>, n: f64) {
        ops.push(OpCode::LdNum(n));
    }

    fn emit_expr_string(&mut self, ops: &mut Vec<OpCode>, s: &String) {
        ops.push(OpCode::LdStr(s.clone()));
    }

    fn emit_expr_identifier(&mut self, ops: &mut Vec<OpCode>, slot: u32) {
        ops.push(OpCode::LdVar(slot));
    }

    fn emit_expr_constant(&mut self, ops: &mut Vec<OpCode>, id: u32) {
        ops.push(OpCode::LdConst(id));
    }

    fn emit_expr_binary(&mut self, ops: &mut Vec<OpCode>, lhs: &HirExpression, rhs: &HirExpression, operator: &BinaryOp, program: &HirAst) {
        self.emit_expression(ops, lhs, program);
        self.emit_expression(ops, rhs, program);
        let use_optimized = self.is_statically_number(lhs, program) && self.is_statically_number(rhs, program);
        match operator {
            BinaryOp::Add => {
                if use_optimized {
                    ops.push(OpCode::AddNum);
                } else {
                    ops.push(OpCode::Add);
                }
            }
            BinaryOp::Sub => {
                if use_optimized {
                    ops.push(OpCode::SubNum);
                } else {
                    ops.push(OpCode::Sub);
                }
            }
            BinaryOp::Mul => {
                if use_optimized {
                    ops.push(OpCode::MulNum);
                } else {
                    ops.push(OpCode::Mul);
                }
            }
            BinaryOp::Div => ops.push(OpCode::Div),
            BinaryOp::Mod => ops.push(OpCode::Mod),
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

    fn emit_expr_unary(&mut self, ops: &mut Vec<OpCode>, operand: &HirExpression, operator: &UnaryOp, program: &HirAst) {
        self.emit_expression(ops, operand, program);
        match operator {
            UnaryOp::Neg => ops.push(OpCode::Neg),
            UnaryOp::Not => ops.push(OpCode::Not),
            _ => todo!(),
        }
    }

    fn emit_expr_function_call(&mut self, ops: &mut Vec<OpCode>, function_id: u32, args: &[HirExpression], invoke: bool, program: &HirAst) {
        // Push arguments first (they'll be on the bottom of the stack)
        for arg in args {
            self.emit_expression(ops, arg, program);
        }
        ops.push(OpCode::LdFunc(function_id));
        let arg_count = args.len() as u32;
        // All function calls go through thunks for consistency
        if invoke {
            // Invoked call: create thunk and immediately execute
            ops.push(OpCode::Thunk(arg_count));
            ops.push(OpCode::Invoke);
        } else {
            // Not invoked: create thunk for lazy evaluation
            ops.push(OpCode::Thunk(arg_count));
        }
    }

    fn emit_expr_partial_call(&mut self, ops: &mut Vec<OpCode>, func_id: u32, bound: &[Option<HirExpression>], program: &HirAst) {
        // Push bound arguments onto the stack in the order they appear
        // (they'll be popped in reverse, so we need to push in reverse order)
        let mut bound_values = Vec::new();
        for arg_opt in bound.iter().rev() {
            if let Some(arg_expr) = arg_opt {
                self.emit_expression(ops, arg_expr, program);
                bound_values.push(true);
            } else {
                bound_values.push(false);
            }
        }
        bound_values.reverse(); // Now in correct order (position 0 = first arg, etc.)
        
        // Build bound_mask: bit i is 1 if argument position i is bound, 0 if it's a hole
        let mut bound_mask: u64 = 0;
        for (i, is_bound) in bound_values.iter().enumerate() {
            if *is_bound {
                bound_mask |= 1 << i;
            }
        }
        
        // Count holes
        let hole_count = bound.iter().filter(|arg| arg.is_none()).count() as u32;
        
        // Emit MakePartial opcode
        ops.push(OpCode::MakePartial {
            func_id,
            bound_mask,
            hole_count,
        });
    }

    fn emit_expr_postfix_invoke(&mut self, ops: &mut Vec<OpCode>, operand: &HirExpression, args: &Option<Vec<HirExpression>>, program: &HirAst) {
        // Handle nested PostfixInvoke expressions.
        // NOTE: There's a known limitation in the HIR structure for nested invocations like
        // `mul2!(add10!(i))!`. The outer operand (mul2) is not properly represented in the HIR,
        // so we work around this by handling nested PostfixInvoke when args is None.
        if args.is_none() {
            if let HirExpression::PostfixInvoke { operand: ref inner_operand, args: ref inner_args } = operand {
                if let Some(ref inner_arg_list) = inner_args {
                    // Emit the inner PostfixInvoke expression (e.g., add10!(i))
                    for arg in inner_arg_list {
                        self.emit_expression(ops, arg, program);
                    }
                    self.emit_expression(ops, inner_operand, program);
                    ops.push(OpCode::Thunk(inner_arg_list.len() as u32));
                    ops.push(OpCode::Invoke);
                    // Inner result is now on the stack.
                    // Workaround: emit operand (the inner PostfixInvoke) as the outer operand.
                    // This is not ideal but necessary due to HIR structure limitations.
                    self.emit_expression(ops, operand, program);
                    ops.push(OpCode::Thunk(1));
                    ops.push(OpCode::Invoke);
                    return;
                }
            }
        }
        
        // If there are additional arguments, emit them first
        if let Some(arg_list) = args {
            let arg_count = arg_list.len() as u32;
            for arg in arg_list {
                self.emit_expression(ops, arg, program);
            }
            self.emit_expression(ops, operand, program);
            ops.push(OpCode::Thunk(arg_count));
            ops.push(OpCode::Invoke);
        } else {
            // No additional arguments - check if operand is a PartialCall
            if let HirExpression::PartialCall { .. } = operand {
                // Just emit the PartialCall - don't invoke it
                self.emit_expression(ops, operand, program);
            } else if let HirExpression::FunctionCall { invoke: true, .. } = operand {
                // FunctionCall with invoke: true already emits Thunk+Invoke
                self.emit_expression(ops, operand, program);
            } else {
                // For other operands (thunks, functions, etc.), invoke them
                self.emit_expression(ops, operand, program);
                ops.push(OpCode::Invoke);
            }
        }
    }

    fn emit_expr_compose_thunk(&mut self, ops: &mut Vec<OpCode>, first: &HirExpression, second: &HirExpression, program: &HirAst) {
        // Emit both expressions onto the stack
        // Stack after both are emitted: [second, first]
        self.emit_expression(ops, second, program);
        self.emit_expression(ops, first, program);
        // ComposeThunk pops second, then first, and pushes composed thunk
        ops.push(OpCode::ComposeThunk);
    }

    fn emit_expr_loop(&mut self, ops: &mut Vec<OpCode>, init_vars: &[(u32, HirExpression)], body: &HirBlock, break_slot: Option<u32>, program: &HirAst) {
        // Emit initialization code for loop variables
        for (var_id, init_expr) in init_vars {
            self.emit_expression(ops, init_expr, program);
            ops.push(OpCode::StVar(*var_id));
        }
        
        let loop_start = ops.len();
        
        self.loop_stack.push(LoopInfo {
            start: loop_start,
            end: 0, // Will be patched later
            break_positions: Vec::new(),
            break_slot,
        });
        
        // Emit the loop body
        self.emit_block(ops, body, program);
        
        // Emit jump back to loop start
        ops.push(OpCode::Jmp(loop_start));
        
        // Record the end of the loop (after the jump back)
        let loop_end = ops.len();
        
        // Patch all break statements that jumped to this loop
        let break_slot_val = if let Some(loop_info) = self.loop_stack.last_mut() {
            loop_info.end = loop_end;
            for &break_pos in &loop_info.break_positions {
                ops[break_pos] = OpCode::Jmp(loop_end);
            }
            loop_info.break_slot
        } else {
            None
        };
        
        // Push the break_slot value (for expression-valued loops)
        if let Some(slot) = break_slot_val {
            ops.push(OpCode::LdVar(slot));
        }
        
        // Pop the loop from stack
        self.loop_stack.pop();
    }

    /// Emit a pattern expression for match statements.
    /// For binary pattern expressions (like == 0, > 10), the left-hand side is the match expression
    /// which is already on the stack, so we skip emitting it and only emit the right-hand side and operator.
    /// Handles nested patterns like "> 0 and < 10" recursively.
    fn emit_pattern_expression(&mut self, ops: &mut Vec<OpCode>, pattern_expr: &HirExpression, match_expr: &HirExpression, program: &HirAst) {
        match pattern_expr {
            HirExpression::Binary { lhs, rhs, operator } => {
                match operator {
                    BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Gt | BinaryOp::Lt | BinaryOp::Ge | BinaryOp::Le => {
                        // Comparison operator: lhs is match_expr (already on stack), just emit rhs and operator
                        self.emit_expression(ops, rhs, program);
                        match operator {
                            BinaryOp::Eq => ops.push(OpCode::Eq),
                            BinaryOp::Ne => ops.push(OpCode::Ne),
                            BinaryOp::Gt => ops.push(OpCode::Gt),
                            BinaryOp::Lt => ops.push(OpCode::Lt),
                            BinaryOp::Ge => ops.push(OpCode::Ge),
                            BinaryOp::Le => ops.push(OpCode::Le),
                            _ => unreachable!(),
                        }
                    }
                    BinaryOp::And | BinaryOp::Or => {
                        // For "and"/"or" patterns like "> 0 and < 10":
                        // lhs is (match_expr > 0), rhs is (match_expr < 10)
                        // Emit lhs (which will handle match_expr > 0)
                        self.emit_pattern_expression(ops, lhs, match_expr, program);
                        // Reload match_expr for the rhs comparison
                        ops.push(OpCode::LdVar(999999u32));
                        // Emit rhs (which will handle match_expr < 10)
                        self.emit_pattern_expression(ops, rhs, match_expr, program);
                        // Emit the and/or operator
                        match operator {
                            BinaryOp::And => ops.push(OpCode::And),
                            BinaryOp::Or => ops.push(OpCode::Or),
                            _ => unreachable!(),
                        }
                    }
                    _ => {
                        // For other operators, emit normally (though they shouldn't appear in patterns)
                        self.emit_expression(ops, pattern_expr, program);
                    }
                }
            }
            _ => {
                // For non-binary patterns, emit normally
                self.emit_expression(ops, pattern_expr, program);
            }
        }
    }

    fn emit_expression(&mut self, ops: &mut Vec<OpCode>, expr: &HirExpression, program: &HirAst) {
        match expr {
            HirExpression::Number(n) => self.emit_expr_number(ops, *n),
            HirExpression::String(s) => self.emit_expr_string(ops, s),
            HirExpression::Identifier(slot) => self.emit_expr_identifier(ops, *slot),
            HirExpression::Constant(id) => self.emit_expr_constant(ops, *id),
            HirExpression::Binary { lhs, rhs, operator } => {
                self.emit_expr_binary(ops, lhs, rhs, operator, program);
            }
            HirExpression::Unary { operand, operator } => {
                self.emit_expr_unary(ops, operand, operator, program);
            }
            HirExpression::FunctionCall { function_id, args, invoke } => {
                self.emit_expr_function_call(ops, *function_id, args, *invoke, program);
            }
            HirExpression::PartialCall { func_id, bound } => {
                self.emit_expr_partial_call(ops, *func_id, bound, program);
            }
            HirExpression::PostfixInvoke { operand, args } => {
                self.emit_expr_postfix_invoke(ops, operand, args, program);
            }
            HirExpression::ComposeThunk { first, second } => {
                self.emit_expr_compose_thunk(ops, first, second, program);
            }
            HirExpression::Loop { init_vars, body, break_slot } => {
                self.emit_expr_loop(ops, init_vars, body, *break_slot, program);
            }
        }
    }
    
}
