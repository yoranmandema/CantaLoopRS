use crate::core::{
    ast::{BinaryOp, UnaryOp},
    bytecode::OpCode,
    hir_lowering::{HirAst, HirBlock, HirExpression, HirStmt, ReducerType, ValueKind},
};

pub struct ByteCodeEmitter {
    loop_stack: Vec<LoopInfo>, // Loop boundaries for nested loops
}

struct LoopInfo {
    start: usize,
    end: usize,
    break_positions: Vec<usize>, // Positions of break jumps to patch
    break_slot: Option<u32>, // Variable slot for break value (None for statement loops, Some(slot) for expression loops)
    continue_target: usize, // Position to jump to on continue (usually start, but for for loops it's the increment)
    continue_positions: Vec<usize>, // Positions of continue jumps to patch (for for loops)
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
            HirExpression::FunctionCall {
                function_id,
                invoke,
                ..
            } => {
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
                    BinaryOp::Add
                    | BinaryOp::Sub
                    | BinaryOp::Mul
                    | BinaryOp::Div
                    | BinaryOp::Pow => {
                        self.is_statically_number(lhs, program)
                            && self.is_statically_number(rhs, program)
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
            HirStmt::Loop {
                init_vars,
                body,
                break_slot,
            } => {
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

    fn emit_stmt_assign(
        &mut self,
        ops: &mut Vec<OpCode>,
        slot: u32,
        value: &HirExpression,
        program: &HirAst,
    ) {
        self.emit_expression(ops, value, program);
        ops.push(OpCode::StVar(slot));
    }

    fn emit_stmt_assign_increment(
        &mut self,
        ops: &mut Vec<OpCode>,
        slot: u32,
        value: &HirExpression,
        program: &HirAst,
    ) {
        ops.push(OpCode::LdVar(slot));
        self.emit_expression(ops, value, program);
        ops.push(OpCode::Add);
        ops.push(OpCode::StVar(slot));
    }

    fn emit_stmt_assign_decrement(
        &mut self,
        ops: &mut Vec<OpCode>,
        slot: u32,
        value: &HirExpression,
        program: &HirAst,
    ) {
        ops.push(OpCode::LdVar(slot));
        self.emit_expression(ops, value, program);
        ops.push(OpCode::Sub);
        ops.push(OpCode::StVar(slot));
    }

    fn emit_stmt_if(
        &mut self,
        ops: &mut Vec<OpCode>,
        arms: &[(HirExpression, HirBlock)],
        else_block: &HirBlock,
        program: &HirAst,
    ) {
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

    fn emit_stmt_match(
        &mut self,
        ops: &mut Vec<OpCode>,
        expression: &HirExpression,
        cases: &[(Option<HirExpression>, HirBlock)],
        program: &HirAst,
    ) {
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
                
                // Optimize: if pattern is a simple literal (String, Number, Boolean),
                // treat it as an equality comparison instead of calling emit_pattern_expression
                match pattern_expr {
                    HirExpression::String(_) | HirExpression::Number(_) | HirExpression::Boolean(_) => {
                        // Simple literal pattern - emit as equality comparison
                        self.emit_expression(ops, pattern_expr, program);
                        ops.push(OpCode::Eq);
                    }
                    HirExpression::Binary { operator, .. } if matches!(operator, BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Gt | BinaryOp::Lt | BinaryOp::Ge | BinaryOp::Le | BinaryOp::And | BinaryOp::Or) => {
                        // Binary comparison pattern - use optimized pattern emission
                        self.emit_pattern_expression(ops, pattern_expr, expression, program);
                    }
                    _ => {
                        // For other patterns, emit as equality comparison (fallback)
                        self.emit_expression(ops, pattern_expr, program);
                        ops.push(OpCode::Eq);
                    }
                }
                
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
        if let HirExpression::FunctionCall {
            function_id,
            args,
            invoke: true,
        } = value
        {
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

    fn emit_stmt_loop(
        &mut self,
        ops: &mut Vec<OpCode>,
        init_vars: &[(u32, HirExpression)],
        body: &HirBlock,
        break_slot: Option<u32>,
        program: &HirAst,
    ) {
        // Emit initialization code for loop variables
        for (var_id, init_expr) in init_vars {
            self.emit_expression(ops, init_expr, program);
            ops.push(OpCode::StVar(*var_id));
        }

        let loop_start = ops.len();

        // Detect if this is a for loop pattern: condition check at start, increment at end
        let is_for_loop = if let Some(first_stmt) = body.statements.first() {
            if let HirStmt::If { arms, else_block } = first_stmt {
                arms.len() == 1
                    && else_block.statements.is_empty()
                    && arms[0].1.statements.len() == 1
                    && matches!(&arms[0].1.statements[0], HirStmt::Break { .. })
            } else {
                false
            }
        } else {
            false
        };

        self.loop_stack.push(LoopInfo {
            start: loop_start,
            end: 0, // Will be patched later
            break_positions: Vec::new(),
            break_slot,
            continue_target: loop_start, // Default to start, will be updated for for loops
            continue_positions: Vec::new(), // Positions of continue jumps to patch
        });

        // OPTIMIZATION: Detect pattern "if (condition) { break [value]; }" at start of loop
        let (condition_opt, break_value_opt, remaining_body) = if let Some(first_stmt) =
            body.statements.first()
        {
            if let HirStmt::If { arms, else_block } = first_stmt {
                if arms.len() == 1
                    && else_block.statements.is_empty()
                    && arms[0].1.statements.len() == 1
                    && matches!(&arms[0].1.statements[0], HirStmt::Break { .. })
                {
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
            // Optimized pattern (for loops)
            self.emit_expression(ops, &condition, program);
            let jmp_if_true_pos = ops.len();
            ops.push(OpCode::JmpIfTrue(0)); // Placeholder

            // For for loops (optimized pattern), the last statement in remaining_body is always the increment
            // Continue should jump to the increment to skip the rest of the body
            if !remaining_body.is_empty() {
                // Check if last statement is an increment (Assign statement)
                let last_is_increment = matches!(remaining_body.last(), Some(HirStmt::Assign { .. }));
                
                if last_is_increment {
                    // This is a for loop - emit all statements except the last one (the increment)
                    let (body_stmts, increment_stmt) = remaining_body.split_at(remaining_body.len() - 1);
                    for stmt in body_stmts {
                        self.emit_statement(ops, stmt, program);
                    }
                    
                    // Track the position where the increment starts
                    let increment_start = ops.len();
                    
                    // Emit the increment (last statement)
                    if let Some(increment) = increment_stmt.first() {
                        self.emit_statement(ops, increment, program);
                    }
                    
                    // Update continue_target and patch all continue statements
                    if let Some(loop_info) = self.loop_stack.last_mut() {
                        loop_info.continue_target = increment_start;
                        // Patch all continue jumps to point to the increment
                        for &continue_pos in &loop_info.continue_positions {
                            ops[continue_pos] = OpCode::Jmp(increment_start);
                        }
                        loop_info.continue_positions.clear();
                    }
                } else {
                    // Not a for loop, emit all statements normally
                    for stmt in &remaining_body {
                        self.emit_statement(ops, stmt, program);
                    }
                }
            } else {
                // Empty remaining body (shouldn't happen for for loops, but handle it)
                // This case shouldn't occur for for loops since they always have an increment
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
                // Patch continue statements - if continue_target was set (for for loops), use it, otherwise use loop_start
                let continue_target = if loop_info.continue_target != loop_info.start {
                    loop_info.continue_target
                } else {
                    loop_info.start
                };
                for &continue_pos in &loop_info.continue_positions {
                    ops[continue_pos] = OpCode::Jmp(continue_target);
                }
                loop_info.continue_positions.clear();
                loop_info.break_slot
            } else {
                None
            };

            if let Some(slot) = break_slot_val {
                ops.push(OpCode::LdVar(slot));
            }
        } else {
            // Normal path
            // Check if last statement is an increment (for loop pattern in non-optimized form)
            let last_is_increment = if let Some(last_stmt) = body.statements.last() {
                matches!(last_stmt, HirStmt::Assign { .. })
            } else {
                false
            };

            if is_for_loop && last_is_increment {
                // For for loops, emit everything except the increment first
                let body_without_increment = &body.statements[..body.statements.len() - 1];
                for stmt in body_without_increment {
                    self.emit_statement(ops, stmt, program);
                }
                
                // Track the position where the increment starts
                let increment_start = ops.len();
                
                // Emit the increment
                if let Some(last_stmt) = body.statements.last() {
                    self.emit_statement(ops, last_stmt, program);
                }
                
                // Update continue_target and patch all continue statements
                if let Some(loop_info) = self.loop_stack.last_mut() {
                    loop_info.continue_target = increment_start;
                    // Patch all continue jumps to point to the increment
                    for &continue_pos in &loop_info.continue_positions {
                        ops[continue_pos] = OpCode::Jmp(increment_start);
                    }
                    loop_info.continue_positions.clear();
                }
            } else {
                // Regular loop, emit all statements
                self.emit_block(ops, body, program);
            }
            ops.push(OpCode::Jmp(loop_start));

            let loop_end = ops.len();
            let break_slot_val = if let Some(loop_info) = self.loop_stack.last_mut() {
                loop_info.end = loop_end;
                for &break_pos in &loop_info.break_positions {
                    ops[break_pos] = OpCode::Jmp(loop_end);
                }
                // Patch continue statements - if continue_target was set (for for loops), use it, otherwise use loop_start
                let continue_target = if loop_info.continue_target != loop_info.start {
                    loop_info.continue_target
                } else {
                    loop_info.start
                };
                for &continue_pos in &loop_info.continue_positions {
                    ops[continue_pos] = OpCode::Jmp(continue_target);
                }
                loop_info.continue_positions.clear();
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

    fn emit_stmt_break(
        &mut self,
        ops: &mut Vec<OpCode>,
        value: &Option<HirExpression>,
        program: &HirAst,
    ) {
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
        if let Some(loop_info) = self.loop_stack.last_mut() {
            // Always use placeholder - we'll patch it later when we know the continue target
            let continue_pos = ops.len();
            ops.push(OpCode::Jmp(0)); // Placeholder
            loop_info.continue_positions.push(continue_pos);
        } else {
            panic!("continue statement outside of loop");
        }
    }

    fn emit_stmt_expression(
        &mut self,
        ops: &mut Vec<OpCode>,
        expr: &HirExpression,
        program: &HirAst,
    ) {
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

    fn emit_expr_boolean(&mut self, ops: &mut Vec<OpCode>, b: bool) {
        ops.push(OpCode::LdBool(b));
    }

    fn emit_expr_binary(
        &mut self,
        ops: &mut Vec<OpCode>,
        lhs: &HirExpression,
        rhs: &HirExpression,
        operator: &BinaryOp,
        program: &HirAst,
    ) {
        self.emit_expression(ops, lhs, program);
        self.emit_expression(ops, rhs, program);
        let use_optimized =
            self.is_statically_number(lhs, program) && self.is_statically_number(rhs, program);
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

    fn emit_expr_unary(
        &mut self,
        ops: &mut Vec<OpCode>,
        operand: &HirExpression,
        operator: &UnaryOp,
        program: &HirAst,
    ) {
        self.emit_expression(ops, operand, program);
        match operator {
            UnaryOp::Neg => ops.push(OpCode::Neg),
            UnaryOp::Not => ops.push(OpCode::Not),
            _ => todo!(),
        }
    }

    fn emit_expr_function_call(
        &mut self,
        ops: &mut Vec<OpCode>,
        function_id: u32,
        args: &[HirExpression],
        invoke: bool,
        program: &HirAst,
    ) {
        // Push arguments first (they'll be on the bottom of the stack)
        for arg in args {
            self.emit_expression(ops, arg, program);
        }
        ops.push(OpCode::LdFunc(function_id));
        let arg_count = args.len() as u32;

        if invoke {
            // Always use Thunk + Invoke for proper thunk evaluation
            ops.push(OpCode::Thunk(arg_count));
            ops.push(OpCode::Invoke);
        } else {
            // Not invoked: create thunk for lazy evaluation
            ops.push(OpCode::Thunk(arg_count));
        }
    }

    fn emit_expr_partial_call(
        &mut self,
        ops: &mut Vec<OpCode>,
        func_id: u32,
        bound: &[Option<HirExpression>],
        program: &HirAst,
    ) {
        // Build bound_values array first (tracks which positions are bound)
        // This must match the order of the bound array (position 0 = first arg, etc.)
        let mut bound_values = Vec::new();
        for arg_opt in bound.iter() {
            bound_values.push(arg_opt.is_some());
        }

        // Push bound arguments onto the stack in reverse order
        // (they'll be popped in reverse, so we need to push in reverse order)
        // Iterate in reverse to push values in reverse order
        for arg_opt in bound.iter().rev() {
            if let Some(arg_expr) = arg_opt {
                self.emit_expression(ops, arg_expr, program);
            }
        }

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

    fn emit_expr_postfix_invoke(
        &mut self,
        ops: &mut Vec<OpCode>,
        operand: &HirExpression,
        args: &Option<Vec<HirExpression>>,
        program: &HirAst,
    ) {
        // Handle nested PostfixInvoke expressions.
        // When we have clos()!, the inner clos() creates a PostfixInvoke with args,
        // and the outer ! creates a PostfixInvoke with args: None
        if args.is_none() {
            if let HirExpression::PostfixInvoke {
                operand: ref inner_operand,
                args: ref inner_args,
            } = operand
            {
                if let Some(ref inner_arg_list) = inner_args {
                    // Emit the inner PostfixInvoke expression (e.g., clos())
                    // This creates a thunk from the function value
                    for arg in inner_arg_list {
                        self.emit_expression(ops, arg, program);
                    }
                    self.emit_expression(ops, inner_operand, program);
                    ops.push(OpCode::Thunk(inner_arg_list.len() as u32));
                    // Don't invoke if inner operand is an identifier (variable with function type)
                    // The outer invoke will handle it
                    if !matches!(**inner_operand, HirExpression::Identifier(_)) {
                        ops.push(OpCode::Invoke);
                    }
                    // Now invoke the thunk that was created by the inner PostfixInvoke
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
            // Always invoke when we have args - this PostfixInvoke represents a function call with !
            // The only case where we don't invoke is when this is an inner PostfixInvoke that will
            // be invoked by an outer one, which is handled in the nested case above
            ops.push(OpCode::Invoke);
        } else {
            // No additional arguments - check if operand is a PartialCall
            if let HirExpression::PartialCall { .. } = operand {
                // Just emit the PartialCall - don't invoke it
                self.emit_expression(ops, operand, program);
            } else if let HirExpression::FunctionCall { invoke: true, .. } = operand {
                // FunctionCall with invoke: true already emits Thunk+Invoke
                self.emit_expression(ops, operand, program);
            } else if let HirExpression::PostfixInvoke { operand: inner_operand, args: inner_args } = operand {
                // Nested PostfixInvoke - emit the inner one first to create the thunk
                if let Some(ref inner_arg_list) = inner_args {
                    // Emit the inner PostfixInvoke expression (e.g., clos())
                    for arg in inner_arg_list {
                        self.emit_expression(ops, arg, program);
                    }
                    self.emit_expression(ops, inner_operand, program);
                    ops.push(OpCode::Thunk(inner_arg_list.len() as u32));
                    // Don't invoke here if inner operand is an identifier (variable with function type)
                    // The outer invoke will handle it
                    if !matches!(**inner_operand, HirExpression::Identifier(_)) {
                        ops.push(OpCode::Invoke);
                    }
                    // Now invoke the thunk that was created by the inner PostfixInvoke
                    ops.push(OpCode::Invoke);
                } else {
                    // Inner has no args - just emit it and invoke
                    self.emit_expression(ops, operand, program);
                    ops.push(OpCode::Invoke);
                }
            } else {
                // For other operands (thunks, functions, etc.), invoke them
                self.emit_expression(ops, operand, program);
                ops.push(OpCode::Invoke);
            }
        }
    }

    fn emit_expr_compose_thunk(
        &mut self,
        ops: &mut Vec<OpCode>,
        first: &HirExpression,
        second: &HirExpression,
        program: &HirAst,
    ) {
        // Emit both expressions onto the stack
        // Stack after both are emitted: [second, first]
        self.emit_expression(ops, second, program);
        self.emit_expression(ops, first, program);
        // ComposeThunk pops second, then first, and pushes composed thunk
        ops.push(OpCode::ComposeThunk);
    }

    fn emit_expr_loop(
        &mut self,
        ops: &mut Vec<OpCode>,
        init_vars: &[(u32, HirExpression)],
        body: &HirBlock,
        break_slot: Option<u32>,
        program: &HirAst,
    ) {
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
            continue_target: loop_start, // For expression loops, continue goes to start
            continue_positions: Vec::new(),
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

    fn emit_expr_array(
        &mut self,
        ops: &mut Vec<OpCode>,
        elements: &[HirExpression],
        program: &HirAst,
    ) {
        // Emit all elements onto the stack
        for elem in elements {
            self.emit_expression(ops, elem, program);
        }
        // Create array from n elements
        ops.push(OpCode::MakeArray(elements.len() as u32));
    }

    fn emit_expr_array_index(
        &mut self,
        ops: &mut Vec<OpCode>,
        array: &HirExpression,
        index: &HirExpression,
        program: &HirAst,
    ) {
        // Emit array expression (pushes array onto stack)
        self.emit_expression(ops, array, program);
        // Emit index expression (pushes index onto stack)
        self.emit_expression(ops, index, program);
        // ArrayIndex opcode pops (array, index) and pushes element
        ops.push(OpCode::ArrayIndex);
    }

    fn emit_expr_array_slice(
        &mut self,
        ops: &mut Vec<OpCode>,
        array: &HirExpression,
        start: &Option<Box<HirExpression>>,
        end: &Option<Box<HirExpression>>,
        step: &Option<Box<HirExpression>>,
        inclusive_end: bool,
        program: &HirAst,
    ) {
        // Emit array expression
        self.emit_expression(ops, array, program);

        // Emit start (or None sentinel)
        if let Some(start_expr) = start {
            self.emit_expression(ops, start_expr, program);
        } else {
            // Push None sentinel - we'll use a special constant or just push a marker
            // For now, we'll use a negative number as sentinel, but None is cleaner
            // Actually, we need to push a Value::none() - let's use a special constant
            // For simplicity, we'll push a magic number that the VM recognizes as "None"
            // Actually, the VM can check for None using Value::is_none()
            // Let's use LdConst with a special constant ID that represents None
            // Or better: push -1 as a sentinel and convert None to -1 in the VM
            // Actually, the cleanest: push None as a number that represents "not specified"
            // For now, let's use a large negative number as sentinel: -999999999.0
            ops.push(OpCode::LdNum(-999999999.0)); // Sentinel for None
        }

        // Emit end (or None sentinel)
        if let Some(end_expr) = end {
            self.emit_expression(ops, end_expr, program);
        } else {
            ops.push(OpCode::LdNum(-999999999.0)); // Sentinel for None
        }

        // Emit step (or None sentinel)
        if let Some(step_expr) = step {
            self.emit_expression(ops, step_expr, program);
        } else {
            ops.push(OpCode::LdNum(-999999999.0)); // Sentinel for None
        }

        // Emit inclusive_end flag (as number: 1.0 for true, 0.0 for false)
        ops.push(OpCode::LdNum(if inclusive_end { 1.0 } else { 0.0 }));

        // ArraySlice opcode pops (array, start, end, step, inclusive_flag) and pushes sliced array
        ops.push(OpCode::ArraySlice);
    }

    /// Resolve an identifier to a function ID by looking it up in imports
    fn resolve_identifier_to_function_id(&self, var_id: u32, program: &HirAst) -> Option<u32> {
        // Look up the variable in scopes to get its name
        for scope in &program.scopes.scopes {
            if let Some(var) = scope.vars.iter().find(|v| v.id == var_id) {
                // Look up the variable name in imports
                for imports in program.module_imports.values() {
                    if let Some(func_id) = imports.get(&var.name) {
                        // Verify it's actually a function (not a constant)
                        if program.functions.contains_key(func_id) {
                            return Some(*func_id);
                        }
                    }
                }
            }
        }
        None
    }

    fn emit_expr_reducer(
        &mut self,
        ops: &mut Vec<OpCode>,
        array: &HirExpression,
        reducer_type: ReducerType,
        reducer_args: &[HirExpression],
        program: &HirAst,
    ) {
        match reducer_type {
            ReducerType::Map | ReducerType::Filter => {
                // Map and Filter: emit VM bytecode opcodes
                // Stack order: [array, function] with function on top
                // Opcode pops: function first, then array
                
                // Emit the array first (will be on bottom)
                self.emit_expression(ops, array, program);
                
                // If the array expression is a ComposeThunk, we need to invoke it to get the actual array
                if matches!(array, HirExpression::ComposeThunk { .. }) {
                    ops.push(OpCode::Invoke);
                }
                
                // Emit the mapper/predicate function (will be on top)
                if reducer_args.len() == 1 {
                    match &reducer_args[0] {
                        HirExpression::Closure { function_id } => {
                            // For closures, load the function directly
                            ops.push(OpCode::LdFunc(*function_id));
                        }
                        _ => {
                            // For other expressions (like function calls), emit them
                            self.emit_expression(ops, &reducer_args[0], program);
                        }
                    }
                } else {
                    panic!("Map and Filter require exactly 1 argument (the function)");
                }
                
                // Stack is now [array, function] with function on top
                // Emit the appropriate opcode
                match reducer_type {
                    ReducerType::Map => ops.push(OpCode::Map),
                    ReducerType::Filter => ops.push(OpCode::Filter),
                    _ => unreachable!(),
                }
            }
            ReducerType::Fold => {
                // Fold: emit VM bytecode opcode
                // Stack order: [array, initial_value, function] with function on top
                // Opcode pops: function first, then initial_value, then array
                
                // Emit the array first (will be on bottom)
                self.emit_expression(ops, array, program);
                
                // If the array expression is a ComposeThunk, we need to invoke it to get the actual array
                if matches!(array, HirExpression::ComposeThunk { .. }) {
                    ops.push(OpCode::Invoke);
                }
                
                // Emit the initial value (will be in middle)
                if reducer_args.len() >= 2 {
                    self.emit_expression(ops, &reducer_args[0], program);
                } else {
                    panic!("fold requires 2 arguments (initial_value, function)");
                }
                
                // Emit the function (will be on top)
                match &reducer_args[1] {
                    HirExpression::Closure { function_id } => {
                        ops.push(OpCode::LdFunc(*function_id));
                    }
                    HirExpression::FunctionCall { function_id, .. } => {
                        ops.push(OpCode::LdFunc(*function_id));
                    }
                    HirExpression::Identifier(var_id) => {
                        let func_id = self.resolve_identifier_to_function_id(*var_id, program)
                            .unwrap_or_else(|| {
                                panic!("fold function must be a function call or imported function identifier");
                            });
                        ops.push(OpCode::LdFunc(func_id));
                    }
                    _ => panic!("fold function must be a function call or identifier"),
                }
                
                // Stack is now [array, initial_value, function] with function on top
                ops.push(OpCode::Fold);
            }
            ReducerType::Sum | ReducerType::Reduce => {
                // Emit the array
                self.emit_expression(ops, array, program);
                
                // If the array expression is a ComposeThunk, we need to invoke it to get the actual array
                // Check if it's a ComposeThunk by pattern matching
                if matches!(array, HirExpression::ComposeThunk { .. }) {
                    // The ComposeThunk is on the stack as a thunk, invoke it to get the array
                    ops.push(OpCode::Invoke);
                }

                // Start iteration (consumes array, pushes iterator)
                ops.push(OpCode::ArrayIter);

                // Initialize accumulator BEFORE the loop
                let acc_slot = 999998u32;
                let (func_id, use_add_opcode, use_first_element) = match reducer_type {
                    ReducerType::Sum => {
                        // sum is equivalent to fold(0, add)
                        ops.push(OpCode::LdNum(0.0));
                        ops.push(OpCode::StVar(acc_slot));
                        (0, true, false)
                    }
                    ReducerType::Reduce => {
                        // reduce(fn) - use first element as initial value
                        // We'll set this up in the loop
                        (match &reducer_args[0] {
                            HirExpression::FunctionCall { function_id, .. } => *function_id,
                            HirExpression::Identifier(var_id) => {
                                self.resolve_identifier_to_function_id(*var_id, program)
                                    .unwrap_or_else(|| {
                                        panic!("reduce function must be a function call or imported function identifier");
                                    })
                            }
                            _ => panic!("reduce function must be a function call or identifier"),
                        }, false, true)
                    }
                    _ => unreachable!(),
                };

                // Store iterator in a variable
                let iter_slot = 999996u32;
                ops.push(OpCode::StVar(iter_slot));

                // For reduce, we need to get the first element first
                if use_first_element {
                    // Load iterator and get first element
                    ops.push(OpCode::LdVar(iter_slot));
                    ops.push(OpCode::ArrayNext);
                    // Stack: [has_more, element]
                    let has_more_slot = 999995u32;
                    ops.push(OpCode::StVar(has_more_slot));
                    // Stack: [element]
                    ops.push(OpCode::StVar(acc_slot));
                    // Stack: []
                    // Check if we have at least one element
                    ops.push(OpCode::LdVar(has_more_slot));
                    let check_pos = ops.len();
                    ops.push(OpCode::JmpIfFalse(0)); // Will patch
                    // If no elements, we'd need to handle empty array case
                    // For now, assume non-empty (could add empty check later)
                    let after_check = ops.len();
                    ops[check_pos] = OpCode::JmpIfFalse(after_check);
                }

                let loop_start = ops.len();

                // Reload iterator
                ops.push(OpCode::LdVar(iter_slot));
                ops.push(OpCode::ArrayNext);
                // Stack: [has_more, element]

                let elem_slot = 999997u32;
                let has_more_slot = 999995u32;
                ops.push(OpCode::StVar(has_more_slot));
                ops.push(OpCode::StVar(elem_slot));

                // Load accumulator and element
                ops.push(OpCode::LdVar(acc_slot));
                ops.push(OpCode::LdVar(elem_slot));

                // Call reducer function
                if use_add_opcode {
                    ops.push(OpCode::Add);
                } else {
                    ops.push(OpCode::LdFunc(func_id));
                    ops.push(OpCode::Thunk(2));
                    ops.push(OpCode::Invoke);
                }

                // Store result back to accumulator
                ops.push(OpCode::StVar(acc_slot));

                // Check has_more - if false, break
                ops.push(OpCode::LdVar(has_more_slot));
                let has_more_pos = ops.len();
                ops.push(OpCode::JmpIfFalse(0));

                // Jump back to loop start
                ops.push(OpCode::Jmp(loop_start));

                // Patch the break jump
                let loop_end = ops.len();
                ops[has_more_pos] = OpCode::JmpIfFalse(loop_end);

                // Load accumulator as result
                ops.push(OpCode::LdVar(acc_slot));
            }
        }
    }

    /// Emit a pattern expression for match statements.
    /// For binary pattern expressions (like == 0, > 10), the left-hand side is the match expression
    /// which is already on the stack, so we skip emitting it and only emit the right-hand side and operator.
    /// Handles nested patterns like "> 0 and < 10" recursively.
    fn emit_pattern_expression(
        &mut self,
        ops: &mut Vec<OpCode>,
        pattern_expr: &HirExpression,
        match_expr: &HirExpression,
        program: &HirAst,
    ) {
        match pattern_expr {
            HirExpression::Binary { lhs, rhs, operator } => {
                match operator {
                    BinaryOp::Eq
                    | BinaryOp::Ne
                    | BinaryOp::Gt
                    | BinaryOp::Lt
                    | BinaryOp::Ge
                    | BinaryOp::Le => {
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
                        // For "&&"/"||" patterns like "> 0 && < 10":
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
            HirExpression::Boolean(b) => self.emit_expr_boolean(ops, *b),
            HirExpression::Identifier(slot) => self.emit_expr_identifier(ops, *slot),
            HirExpression::Constant(id) => self.emit_expr_constant(ops, *id),
            HirExpression::Binary { lhs, rhs, operator } => {
                self.emit_expr_binary(ops, lhs, rhs, operator, program);
            }
            HirExpression::Unary { operand, operator } => {
                self.emit_expr_unary(ops, operand, operator, program);
            }
            HirExpression::FunctionCall {
                function_id,
                args,
                invoke,
            } => {
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
            HirExpression::Loop {
                init_vars,
                body,
                break_slot,
            } => {
                self.emit_expr_loop(ops, init_vars, body, *break_slot, program);
            }
            HirExpression::Array(elements) => {
                self.emit_expr_array(ops, elements, program);
            }
            HirExpression::ArrayIndex { array, index } => {
                self.emit_expr_array_index(ops, array, index, program);
            }
            HirExpression::ArraySlice {
                array,
                start,
                end,
                step,
                inclusive_end,
            } => {
                self.emit_expr_array_slice(ops, array, start, end, step, *inclusive_end, program);
            }
            HirExpression::Closure { function_id } => {
                // Emit code to load the closure function
                ops.push(OpCode::LdFunc(*function_id));
            }
            HirExpression::Reducer {
                array,
                reducer_type,
                reducer_args,
            } => {
                self.emit_expr_reducer(ops, array, *reducer_type, reducer_args, program);
            }
            HirExpression::StructInit { struct_name, fields } => {
                self.emit_expr_struct_init(ops, struct_name, fields, program);
            }
            HirExpression::FieldAccess { base, field_name } => {
                self.emit_expr_field_access(ops, base, field_name, program);
            }
        }
    }

    fn emit_expr_struct_init(
        &mut self,
        ops: &mut Vec<OpCode>,
        struct_name: &str,
        fields: &[(String, HirExpression)],
        program: &HirAst,
    ) {
        // Get struct definition to verify field order
        let struct_def = program.structs.get(struct_name)
            .expect(&format!("Struct {} not found", struct_name));
        
        // Create a map from field name to expression for quick lookup
        let field_map: std::collections::HashMap<_, _> = fields.iter()
            .map(|(name, expr)| (name.clone(), expr))
            .collect();
        
        // Emit field values in the order defined in the struct definition
        for (field_name, _) in &struct_def.fields {
            let field_expr = field_map.get(field_name)
                .expect(&format!("Field {} not provided in struct initialization", field_name));
            self.emit_expression(ops, field_expr, program);
        }
        
        // Now emit MakeStruct with type_id and field count
        // Use a stable hash of the struct name as type_id
        let type_id = crate::core::vm::compute_struct_type_id(struct_name);
        ops.push(OpCode::MakeStruct {
            type_id,
            field_count: struct_def.fields.len() as u32,
        });
    }

    fn emit_expr_field_access(
        &mut self,
        ops: &mut Vec<OpCode>,
        base: &HirExpression,
        field_name: &str,
        program: &HirAst,
    ) {
        // Emit the base expression
        self.emit_expression(ops, base, program);
        
        // Find the struct type from the base expression's type
        // For now, we'll need to look up the field index from the struct definition
        // This is a simplified version - in practice you'd track types through inference
        
        // We need to find which struct this is and get the field index
        // For now, we'll search all structs to find the one with this field
        let mut field_index = None;
        for (struct_name, struct_def) in &program.structs {
            if let Some(idx) = struct_def.fields.iter().position(|(name, _)| name == field_name) {
                field_index = Some(idx as u32);
                break;
            }
        }
        
        let field_idx = field_index.expect(&format!("Field {} not found in any struct", field_name));
        ops.push(OpCode::GetField(field_idx));
    }
}
