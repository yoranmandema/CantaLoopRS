use crate::core::bytecode::{OPCODE_COUNT, OpCode};
use crate::core::engine::{Arity, Engine};

/// Special function ID for composition.
/// When a thunk has this func_id, it represents a composition of two functions.
const COMPOSE_ID: u32 = 0xFFFF_FFFF;

// Function pointer type for opcode handlers
// Note: Handlers take frame_idx to access frame through VM, avoiding borrow conflicts
type OpHandler = fn(&mut VM, usize, &OpCode) -> StepResult;

// Result of executing a step - indicates control flow behavior
#[derive(Debug, Clone, Copy)]
enum StepResult {
    Normal,    // Normal execution, IP was incremented
    Continue,  // Special case (e.g., Ret), needs to restart loop
}

// Dispatch table for monomorphic, branch-predictable opcode execution
// CRITICAL: Array indices MUST match opcode discriminant values from bytecode_opcode.rs
static DISPATCH: [OpHandler; OPCODE_COUNT] = [
    VM::op_ld_num,        // 0: LdNum
    VM::op_ld_str,        // 1: LdStr
    VM::op_ld_var,        // 2: LdVar
    VM::op_ld_const,      // 3: LdConst
    VM::op_ld_func,       // 4: LdFunc
    VM::op_add,           // 5: Add
    VM::op_sub,           // 6: Sub
    VM::op_mul,           // 7: Mul
    VM::op_div,           // 8: Div
    VM::op_pow,           // 9: Pow
    VM::op_eq,            // 10: Eq
    VM::op_ne,            // 11: Ne
    VM::op_gt,            // 12: Gt
    VM::op_lt,            // 13: Lt
    VM::op_ge,            // 14: Ge
    VM::op_le,            // 15: Le
    VM::op_and,           // 16: And
    VM::op_or,            // 17: Or
    VM::op_neg,           // 18: Neg
    VM::op_not,           // 19: Not
    VM::op_st_var,        // 20: StVar
    VM::op_pop,           // 21: Pop
    VM::op_print,         // 22: Print
    VM::op_call_stack,    // 23: CallStack
    VM::op_thunk,         // 24: Thunk
    VM::op_invoke,        // 25: Invoke
    VM::op_ret,           // 26: Ret
    VM::op_jmp_if_false,  // 27: JmpIfFalse
    VM::op_jmp,           // 28: Jmp
    VM::op_add_num,       // 29: AddNum
    VM::op_mul_num,       // 30: MulNum
    VM::op_sub_num,       // 31: SubNum
    VM::op_ret_invoke,    // 32: RetInvoke
    VM::op_jmp_if_true,   // 33: JmpIfTrue
    VM::op_compose_thunk, // 34: ComposeThunk
    VM::op_mod,           // 35: Mod
    VM::op_make_partial,  // 36: MakePartial
];

// Tagged union Value using NaN boxing
// Numbers: valid f64 (not NaN)
// Other types: NaN with tag bits in payload
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Value {
    raw: u64,
}

// Type tags for NaN values (using quiet NaN with specific payload)
// QNAN has bits 0x7FF8 in the exponent, we use bits 48-51 for the tag
// Note: Bit 51 must be set for quiet NaN, so tags use bits 48-50, with bit 51 always set
const PAYLOAD_MASK: u64 = 0x0000_FFFF_FFFF_FFFF;  // Bits 0-47 for payload (exclude tag bits 48-51)
const QNAN_BASE: u64 = 0x7FF8_0000_0000_0000;  // Quiet NaN base (exponent=0x7FF, bit 51 set)
const TAG_MASK: u64 = 0xF << 48;  // Bits 48-51 for tag
const TAG_CLEAR_MASK: u64 = !TAG_MASK;  // Mask to clear tag bits
const QNAN_BIT_51: u64 = 1 << 51;  // Bit 51 must be set for quiet NaN

const TAG_STRING: u64 = 0x1;
const TAG_BOOLEAN: u64 = 0x2;
const TAG_FUNCTION: u64 = 0x3;
const TAG_THUNK: u64 = 0x4;
const TAG_NONE: u64 = 0x5;

/// Heap storage for VM-managed data structures.
/// 
/// Stores strings and thunks that cannot fit in the 64-bit Value representation.
/// Managed per VM instance to avoid global state.
pub struct ValueHeap {
    pub(crate) strings: Vec<String>,
    pub(crate) thunks: Vec<ThunkData>,
}

pub(crate) enum ThunkData {
    Regular {
        func_id: u32,
        bound: Vec<Option<Value>>, // None = hole, Some(value) = bound argument
    },
    Composed {
        first: Value,  // First thunk (f)
        second: Value, // Second thunk (g) - composition is g(f(x))
    },
}

impl ValueHeap {
    fn new() -> Self {
        Self {
            strings: Vec::new(),
            thunks: Vec::new(),
        }
    }
}

impl Value {
    #[inline(always)]
    pub fn number(n: f64) -> Self {
        // Ensure it's not NaN
        debug_assert!(!n.is_nan(), "Cannot create Value::Number from NaN");
        Self { raw: n.to_bits() }
    }

    #[inline(always)]
    pub fn string_with_heap(s: String, heap: &mut ValueHeap) -> Self {
        let idx = heap.strings.len();
        heap.strings.push(s);
        Self {
            raw: (QNAN_BASE & TAG_CLEAR_MASK) | (TAG_STRING << 48) | QNAN_BIT_51 | (idx as u64),
        }
    }

    #[inline(always)]
    pub fn boolean(b: bool) -> Self {
        Self {
            raw: (QNAN_BASE & TAG_CLEAR_MASK) | (TAG_BOOLEAN << 48) | QNAN_BIT_51 | (if b { 1 } else { 0 }),
        }
    }

    #[inline(always)]
    pub fn function(id: u32) -> Self {
        Self {
            raw: (QNAN_BASE & TAG_CLEAR_MASK) | (TAG_FUNCTION << 48) | QNAN_BIT_51 | (id as u64),
        }
    }

    #[inline(always)]
    pub fn thunk_with_heap(func_id: u32, bound: Vec<Option<Value>>, heap: &mut ValueHeap) -> Self {
        let idx = heap.thunks.len();
        heap.thunks.push(ThunkData::Regular { func_id, bound });
        Self {
            raw: (QNAN_BASE & TAG_CLEAR_MASK) | (TAG_THUNK << 48) | QNAN_BIT_51 | (idx as u64),
        }
    }

    #[inline(always)]
    pub fn composed_thunk_with_heap(first: Value, second: Value, heap: &mut ValueHeap) -> Self {
        let idx = heap.thunks.len();
        heap.thunks.push(ThunkData::Composed { first, second });
        Self {
            raw: (QNAN_BASE & TAG_CLEAR_MASK) | (TAG_THUNK << 48) | QNAN_BIT_51 | (idx as u64),
        }
    }


    #[inline(always)]
    pub fn none() -> Self {
        Self {
            raw: (QNAN_BASE & TAG_CLEAR_MASK) | (TAG_NONE << 48) | QNAN_BIT_51,
        }
    }

    #[inline(always)]
    fn tag(&self) -> u64 {
        // Check if it's a NaN (exponent bits 0x7FF)
        if (self.raw & 0x7FF0_0000_0000_0000) == 0x7FF0_0000_0000_0000 {
            // Extract tag from bits 48-51, but bit 51 is always set for quiet NaN
            // So we only look at bits 48-50 for the tag (3 bits)
            (self.raw >> 48) & 0x7
        } else {
            0 // Number
        }
    }

    #[inline(always)]
    pub fn as_number(&self) -> Option<f64> {
        if self.tag() == 0 {
            Some(f64::from_bits(self.raw))
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn as_string(&self, heap: &ValueHeap) -> Option<String> {
        if self.tag() == TAG_STRING {
            let idx = (self.raw & PAYLOAD_MASK) as usize;
            heap.strings.get(idx).cloned()
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn as_boolean(&self) -> Option<bool> {
        if self.tag() == TAG_BOOLEAN {
            Some((self.raw & 1) != 0)
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn as_function(&self) -> Option<u32> {
        if self.tag() == TAG_FUNCTION {
            Some((self.raw & PAYLOAD_MASK) as u32)
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn is_thunk(&self) -> bool {
        self.tag() == TAG_THUNK
    }

    #[inline(always)]
    pub fn as_thunk(&self, heap: &ValueHeap) -> Option<(u32, Vec<Option<Value>>)> {
        if self.is_thunk() {
            let idx = (self.raw & PAYLOAD_MASK) as usize;
            heap.thunks.get(idx).and_then(|t| {
                match t {
                    ThunkData::Regular { func_id, bound } => Some((*func_id, bound.clone())),
                    ThunkData::Composed { .. } => None, // Composed thunks don't have a single func_id
                }
            })
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn as_thunk_ref<'a>(&self, heap: &'a ValueHeap) -> Option<&'a ThunkData> {
        if self.is_thunk() {
            let idx = (self.raw & PAYLOAD_MASK) as usize;
            heap.thunks.get(idx)
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn is_none(&self) -> bool {
        self.tag() == TAG_NONE
    }

    pub fn value_to_string(self, heap: &ValueHeap) -> String {
        if let Some(n) = self.as_number() {
            n.to_string()
        } else if let Some(s) = self.as_string(heap) {
            s
        } else if let Some(b) = self.as_boolean() {
            b.to_string()
        } else if let Some(id) = self.as_function() {
            format!("<function:{}>", id)
        } else if self.is_thunk() {
            "<prepared_call>".to_string()
        } else if self.is_none() {
            "None".to_string()
        } else {
            "Unknown".to_string()
        }
    }
}

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(n) = self.as_number() {
            write!(f, "Value::Number({})", n)
        } else if let Some(b) = self.as_boolean() {
            write!(f, "Value::Boolean({})", b)
        } else if let Some(id) = self.as_function() {
            write!(f, "Value::Function({})", id)
        } else if self.is_none() {
            write!(f, "Value::None")
        } else {
            write!(f, "Value::Tagged({:x})", self.raw)
        }
    }
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
    code: &'static [OpCode],      // Bytecode to execute (either top-level ops or function code) - cached, not cloned
    ip: usize,                     // Instruction pointer (current position in code)
    locals: Box<[Value]>,          // Local variable slots (indexed by var_id) - Box to avoid Vec allocation
    stack_depth: usize,           // Stack depth when this frame was entered (for cleanup on return)
}

pub struct VM<'a> {
    engine: &'a Engine,
    ops: &'static [OpCode],  // Top-level bytecode - cached, not cloned
    stack: Vec<Value>,
    call_stack: Vec<CallFrame>,
    heap: ValueHeap,
}

impl<'a> VM<'a> {
    pub fn new(engine: &'a Engine, ops: Vec<OpCode>) -> Self {
        // Leak the bytecode to get a 'static reference - this is acceptable since
        // bytecode is created once and lives for the entire program lifetime
        let ops_box = Box::new(ops);
        let ops_slice: &'static [OpCode] = Box::leak(ops_box);
        
        Self {
            engine,
            ops: ops_slice,
            stack: Vec::new(),
            call_stack: Vec::new(),
            heap: ValueHeap::new(),
        }
    }


    #[inline(always)]
    fn step(&mut self, frame_idx: usize) -> StepResult {
        // Clone opcode to avoid borrow conflicts (opcode data is needed in handlers)
        let ip = self.call_stack[frame_idx].ip;
        let opcode = self.call_stack[frame_idx].code[ip].clone();
        let disc = opcode.discriminant() as usize;
        self.call_stack[frame_idx].ip += 1;
        let handler = unsafe { *DISPATCH.get_unchecked(disc) };
        handler(self, frame_idx, &opcode)
    }

    pub fn run(&mut self) {
        // Initialize top-level frame - use static reference, no cloning
        self.call_stack.push(CallFrame {
            code: self.ops,
            ip: 0,
            locals: Box::new([]),  // Empty locals for top-level
            stack_depth: 0,  // Top-level starts with empty stack
        });

        // Main execution loop - process the current frame
        self.execute_until_empty(None);
    }

    /// Execute frames until the call stack is empty or until a specific frame count is reached.
    /// If `target_frame_count` is Some, execution stops when the call stack length is less than that value.
    /// This allows `invoke_thunk_sync` to execute until a specific function returns.
    fn execute_until_empty(&mut self, target_frame_count: Option<usize>) {
        // Main execution loop - process the current frame
        while !self.call_stack.is_empty() {
            // Check if we've reached the target frame count (for invoke_thunk_sync)
            // CRITICAL: Check this FIRST, before doing any frame processing
            // This ensures that when execute_return pops the frame, we immediately
            // return instead of processing the previous frame
            // target_frame_count is the frame count BEFORE the function was pushed,
            // so we return when we're back to that count (<= not <)
            if let Some(target) = target_frame_count {
                if self.call_stack.len() <= target {
                    return;  // The target function has returned
                }
            }
            
            let frame_idx = self.call_stack.len() - 1;
            
            // Check if frame is finished
            let frame_finished = {
                let frame = &self.call_stack[frame_idx];
                frame.ip >= frame.code.len()
            };
            
            if frame_finished {
                // Function reached end without explicit return - handle implicit return
                // Pop return value (or use None if stack is empty), similar to execute_return
                let mut return_value = self.stack.pop().unwrap_or(Value::none());
                
                // Auto-invoke thunks at function boundaries
                if return_value.is_thunk() {
                    return_value = self.invoke_thunk_value_recursive(return_value);
                }
                
                // Get the stack depth that was saved when this frame was entered
                let expected_stack_depth = self.call_stack[frame_idx].stack_depth;
                
                // Clean up any intermediate values that the function left on the stack
                while self.stack.len() > expected_stack_depth {
                    self.stack.pop();
                }
                
                // Pop the current frame
                self.call_stack.pop();
                
                // Push return value back on stack
                self.stack.push(return_value);
                // CRITICAL: After implicit return, check target frame count again
                // This ensures we return immediately if we've reached the target
                if let Some(target) = target_frame_count {
                    if self.call_stack.len() <= target {
                        return;  // The target function has returned
                    }
                }
                continue;
            }

            // Execute the opcode using dispatch table
            let step_result = self.step(frame_idx);
            match step_result {
                StepResult::Continue => {
                    // Ret was executed - check target frame count immediately
                    // This ensures we return immediately after execute_return
                    if let Some(target) = target_frame_count {
                        if self.call_stack.len() <= target {
                            return;  // The target function has returned
                        }
                    }
                    continue;
                },
                StepResult::Normal => {},          // Normal execution, IP already incremented
            }
        }
    }

    // Opcode handlers - each handler extracts data from opcode and executes
    #[inline(always)]
    fn op_ld_num(_vm: &mut VM, _frame_idx: usize, opcode: &OpCode) -> StepResult {
        if let OpCode::LdNum(n) = opcode {
            _vm.stack.push(Value::number(*n));
        }
        StepResult::Normal
    }

    #[inline(always)]
    fn op_ld_str(_vm: &mut VM, _frame_idx: usize, opcode: &OpCode) -> StepResult {
        if let OpCode::LdStr(s) = opcode {
            _vm.stack.push(Value::string_with_heap(s.clone(), &mut _vm.heap));
        }
        StepResult::Normal
    }

    #[inline(always)]
    fn op_ld_var(_vm: &mut VM, frame_idx: usize, opcode: &OpCode) -> StepResult {
        if let OpCode::LdVar(id) = opcode {
            let idx = *id as usize;
            let frame = &_vm.call_stack[frame_idx];
            let val = if idx < frame.locals.len() && !frame.locals[idx].is_none() {
                // Variable exists in current frame and has a value
                frame.locals[idx]
            } else {
                // Variable not found or is None in current frame - check top-level frame (frame 0)
                // This allows functions to access module-level variables
                // Note: We check top-level even if current frame has the variable as None,
                // because function locals may be pre-allocated with None for parameters
                if frame_idx > 0 && !_vm.call_stack.is_empty() {
                    let top_level_frame = &_vm.call_stack[0];
                    if idx < top_level_frame.locals.len() {
                        top_level_frame.locals[idx]
                    } else {
                        // Not in top-level either - return None from current frame if it exists, else None
                        if idx < frame.locals.len() {
                            frame.locals[idx]
                        } else {
                            Value::none()
                        }
                    }
                } else {
                    // We're in the top-level frame
                    if idx < frame.locals.len() {
                        frame.locals[idx]
                    } else {
                        Value::none()
                    }
                }
            };
            _vm.stack.push(val);
        }
        StepResult::Normal
    }

    #[inline(always)]
    fn op_ld_const(_vm: &mut VM, _frame_idx: usize, opcode: &OpCode) -> StepResult {
        if let OpCode::LdConst(id) = opcode {
            let const_val = _vm.engine.get_constant(*id, &mut _vm.heap);
            _vm.stack.push(const_val);
        }
        StepResult::Normal
    }

    #[inline(always)]
    fn op_ld_func(_vm: &mut VM, _frame_idx: usize, opcode: &OpCode) -> StepResult {
        if let OpCode::LdFunc(id) = opcode {
            let func_val = _vm.engine.get_function(*id);
            _vm.stack.push(func_val);
        }
        StepResult::Normal
    }

    #[inline(always)]
    fn op_add(_vm: &mut VM, _frame_idx: usize, _opcode: &OpCode) -> StepResult {
        _vm.binary_add();
        StepResult::Normal
    }

    #[inline(always)]
    fn op_sub(_vm: &mut VM, _frame_idx: usize, _opcode: &OpCode) -> StepResult {
        _vm.binary_sub();
        StepResult::Normal
    }

    #[inline(always)]
    fn op_mul(_vm: &mut VM, _frame_idx: usize, _opcode: &OpCode) -> StepResult {
        _vm.binary_mul();
        StepResult::Normal
    }

    #[inline(always)]
    fn op_add_num(_vm: &mut VM, _frame_idx: usize, _opcode: &OpCode) -> StepResult {
        let rhs = _vm.stack.pop().expect("Stack underflow");
        let lhs = _vm.stack.pop().expect("Stack underflow");
        // Force values in case they're thunks (defensive check - should not be needed for AddNum)
        let lhs_forced = _vm.force_value(lhs);
        let rhs_forced = _vm.force_value(rhs);
        if let (Some(a), Some(b)) = (lhs_forced.as_number(), rhs_forced.as_number()) {
            _vm.stack.push(Value::number(a + b));
        } else {
            let lhs_str = lhs_forced.value_to_string(&_vm.heap);
            let rhs_str = rhs_forced.value_to_string(&_vm.heap);
            panic!("AddNum: expected numbers but got non-number values (lhs: {:?} = {}, rhs: {:?} = {})", 
                lhs_forced, lhs_str, rhs_forced, rhs_str);
        }
        StepResult::Normal
    }

    #[inline(always)]
    fn op_mul_num(_vm: &mut VM, _frame_idx: usize, _opcode: &OpCode) -> StepResult {
        let rhs = _vm.stack.pop().expect("Stack underflow");
        let lhs = _vm.stack.pop().expect("Stack underflow");
        // Force values in case they're thunks (defensive check - should not be needed for MulNum)
        let lhs_forced = _vm.force_value(lhs);
        let rhs_forced = _vm.force_value(rhs);
        if let (Some(a), Some(b)) = (lhs_forced.as_number(), rhs_forced.as_number()) {
            _vm.stack.push(Value::number(a * b));
        } else {
            let lhs_str = lhs_forced.value_to_string(&_vm.heap);
            let rhs_str = rhs_forced.value_to_string(&_vm.heap);
            panic!("MulNum: expected numbers but got non-number values (lhs: {:?} = {}, rhs: {:?} = {})", 
                lhs_forced, lhs_str, rhs_forced, rhs_str);
        }
        StepResult::Normal
    }

    #[inline(always)]
    fn op_sub_num(_vm: &mut VM, _frame_idx: usize, _opcode: &OpCode) -> StepResult {
        let rhs = _vm.stack.pop().expect("Stack underflow");
        let lhs = _vm.stack.pop().expect("Stack underflow");
        // Force values in case they're thunks (defensive check - should not be needed for SubNum)
        let lhs_forced = _vm.force_value(lhs);
        let rhs_forced = _vm.force_value(rhs);
        if let (Some(a), Some(b)) = (lhs_forced.as_number(), rhs_forced.as_number()) {
            _vm.stack.push(Value::number(a - b));
        } else {
            let lhs_str = lhs_forced.value_to_string(&_vm.heap);
            let rhs_str = rhs_forced.value_to_string(&_vm.heap);
            panic!("SubNum: expected numbers but got non-number values (lhs: {:?} = {}, rhs: {:?} = {})", 
                lhs_forced, lhs_str, rhs_forced, rhs_str);
        }
        StepResult::Normal
    }

    #[inline(always)]
    fn op_div(_vm: &mut VM, _frame_idx: usize, _opcode: &OpCode) -> StepResult {
        _vm.binary_div();
        StepResult::Normal
    }

    #[inline(always)]
    fn op_mod(_vm: &mut VM, _frame_idx: usize, _opcode: &OpCode) -> StepResult {
        _vm.binary_mod();
        StepResult::Normal
    }

    #[inline(always)]
    fn op_pow(_vm: &mut VM, _frame_idx: usize, _opcode: &OpCode) -> StepResult {
        _vm.binary_pow();
        StepResult::Normal
    }

    #[inline(always)]
    fn op_eq(_vm: &mut VM, _frame_idx: usize, _opcode: &OpCode) -> StepResult {
        _vm.comparison_eq();
        StepResult::Normal
    }

    #[inline(always)]
    fn op_ne(_vm: &mut VM, _frame_idx: usize, _opcode: &OpCode) -> StepResult {
        _vm.comparison_ne();
        StepResult::Normal
    }

    #[inline(always)]
    fn op_gt(_vm: &mut VM, _frame_idx: usize, _opcode: &OpCode) -> StepResult {
        _vm.comparison_gt();
        StepResult::Normal
    }

    #[inline(always)]
    fn op_lt(_vm: &mut VM, _frame_idx: usize, _opcode: &OpCode) -> StepResult {
        _vm.comparison_lt();
        StepResult::Normal
    }

    #[inline(always)]
    fn op_ge(_vm: &mut VM, _frame_idx: usize, _opcode: &OpCode) -> StepResult {
        _vm.comparison_ge();
        StepResult::Normal
    }

    #[inline(always)]
    fn op_le(_vm: &mut VM, _frame_idx: usize, _opcode: &OpCode) -> StepResult {
        _vm.comparison_le();
        StepResult::Normal
    }

    #[inline(always)]
    fn op_and(_vm: &mut VM, _frame_idx: usize, _opcode: &OpCode) -> StepResult {
        _vm.logical_and();
        StepResult::Normal
    }

    #[inline(always)]
    fn op_or(_vm: &mut VM, _frame_idx: usize, _opcode: &OpCode) -> StepResult {
        _vm.logical_or();
        StepResult::Normal
    }

    #[inline(always)]
    fn op_neg(_vm: &mut VM, _frame_idx: usize, _opcode: &OpCode) -> StepResult {
        let v_val = _vm.stack.pop().expect("Stack underflow");
        let v = _vm.force_value(v_val);
        if let Some(n) = v.as_number() {
            _vm.stack.push(Value::number(-n));
        } else {
            panic!("Negate non-number");
        }
        StepResult::Normal
    }

    #[inline(always)]
    fn op_not(_vm: &mut VM, _frame_idx: usize, _opcode: &OpCode) -> StepResult {
        let v_val = _vm.stack.pop().expect("Stack underflow");
        let v = _vm.force_value(v_val);
        if let Some(n) = v.as_number() {
            _vm.stack.push(Value::number(if n == 0.0 { 1.0 } else { 0.0 }));
        } else if let Some(b) = v.as_boolean() {
            _vm.stack.push(Value::boolean(!b));
        } else {
            panic!("Not on non-number/non-boolean");
        }
        StepResult::Normal
    }

    #[inline(always)]
    fn op_pop(_vm: &mut VM, _frame_idx: usize, _opcode: &OpCode) -> StepResult {
        _vm.stack.pop().expect("Stack underflow for Pop");
        StepResult::Normal
    }

    #[inline(always)]
    fn op_st_var(_vm: &mut VM, frame_idx: usize, opcode: &OpCode) -> StepResult {
        if let OpCode::StVar(id) = opcode {
            let val = _vm.stack.pop().expect("Stack underflow");
            let idx = *id as usize;
            let frame = &mut _vm.call_stack[frame_idx];
            // Ensure locals is large enough - convert to Vec, resize, then back to Box
            if idx >= frame.locals.len() {
                let mut locals_vec: Vec<Value> = std::mem::take(&mut frame.locals).into();
                locals_vec.resize(idx + 1, Value::none());
                frame.locals = locals_vec.into_boxed_slice();
            }
            frame.locals[idx] = val;
        }
        StepResult::Normal
    }

    #[inline(always)]
    fn op_print(_vm: &mut VM, _frame_idx: usize, _opcode: &OpCode) -> StepResult {
        let v = _vm.stack.pop().expect("Stack underflow");
        let s = v.value_to_string(&_vm.heap);
        println!("{}", s);
        StepResult::Normal
    }

    #[inline(always)]
    fn op_call_stack(_vm: &mut VM, _frame_idx: usize, opcode: &OpCode) -> StepResult {
        if let OpCode::CallStack(n_args) = opcode {
            _vm.execute_call_stack(*n_args);
        }
        StepResult::Normal
    }

    #[inline(always)]
    fn op_thunk(_vm: &mut VM, _frame_idx: usize, opcode: &OpCode) -> StepResult {
        if let OpCode::Thunk(n_args) = opcode {
            _vm.execute_prepare_call(*n_args);
        }
        StepResult::Normal
    }

    #[inline(always)]
    fn op_make_partial(_vm: &mut VM, _frame_idx: usize, opcode: &OpCode) -> StepResult {
        if let OpCode::MakePartial { func_id, bound_mask, hole_count } = opcode {
            // Get function signature to know total parameter count
            let total_params = if let Some(func) = _vm.engine.bytecode_functions.get(func_id) {
                func.param_var_ids.len()
            } else if let Some(native_func) = _vm.engine.functions.get(func_id) {
                Self::min_arity(&native_func.arity)
            } else {
                // Unknown function - use bound_mask to infer: find the highest set bit
                let mut max_pos = 0;
                for i in 0..64 {
                    if (bound_mask & (1 << i)) != 0 {
                        max_pos = i;
                    }
                }
                (max_pos + 1) as usize
            };
            
            // Count how many bound arguments we need to pop
            let mut args_to_pop = 0;
            for i in 0..total_params {
                if (bound_mask & (1 << i)) != 0 {
                    args_to_pop += 1;
                }
            }
            
            // Pop bound arguments from stack (they were pushed in reverse order)
            let mut popped_values = Vec::new();
            for _ in 0..args_to_pop {
                popped_values.push(_vm.stack.pop().expect("Stack underflow"));
            }
            popped_values.reverse(); // Now in correct order (position 0 = first arg)
            
            // Build bound_args vector: None for holes, Some(value) for bound args
            // Position i corresponds to function parameter i
            let mut bound_args_vec = Vec::new();
            let mut popped_idx = 0;
            for i in 0..total_params {
                if (bound_mask & (1 << i)) != 0 {
                    if popped_idx < popped_values.len() {
                        bound_args_vec.push(Some(popped_values[popped_idx]));
                        popped_idx += 1;
                    } else {
                        bound_args_vec.push(None); // Shouldn't happen, but be safe
                    }
                } else {
                    bound_args_vec.push(None); // Hole
                }
            }
            
            // Create thunk with holes
            let thunk = Value::thunk_with_heap(*func_id, bound_args_vec, &mut _vm.heap);
            _vm.stack.push(thunk);
        }
        StepResult::Normal
    }

    fn op_compose_thunk(_vm: &mut VM, _frame_idx: usize, _opcode: &OpCode) -> StepResult {
        // Stack: [second, first] (as emitted by bytecode emitter: second pushed first, first pushed second)
        // Stack top is first, stack bottom is second
        // We want to create g(f(x)) where f is first and g is second
        // Composed { first: f, second: g } represents g(f(x))
        // So we pop first (top of stack), then second (below first)
        let popped_first = _vm.stack.pop().expect("Stack underflow");  // Pop first (top of stack)
        let popped_second = _vm.stack.pop().expect("Stack underflow"); // Pop second (below first)
        
        // Create composed thunk: g(f(x)) where first=f, second=g
        // Composed { first: f, second: g } represents g(f(x))
        // The stack is [second, first] after emission, so:
        // - popped_first is actually first (f) - the top of stack
        // - popped_second is actually second (g) - below first
        // For add10 |> add5, we want add5(add10(x)) = Composed { first: add10, second: add5 }
        // popped_first = add10, popped_second = add5
        // So we create Composed { first: popped_first, second: popped_second } = add5(add10(x)) ✓
        let composed = Value::composed_thunk_with_heap(popped_first, popped_second, &mut _vm.heap);
        _vm.stack.push(composed);
        StepResult::Normal
    }

    #[inline(always)]
    fn op_invoke(_vm: &mut VM, _frame_idx: usize, _opcode: &OpCode) -> StepResult {
        _vm.execute_invoke();
        StepResult::Normal
    }

    #[inline(always)]
    fn op_ret(_vm: &mut VM, _frame_idx: usize, _opcode: &OpCode) -> StepResult {
        _vm.execute_return();
        // After return, we've popped the frame, restart loop to handle previous frame
        StepResult::Continue
    }

    #[inline(always)]
    fn op_ret_invoke(_vm: &mut VM, frame_idx: usize, _opcode: &OpCode) -> StepResult {
        _vm.execute_ret_invoke(frame_idx);
        // After tail-call, we've reused the frame, restart loop to continue execution
        StepResult::Continue
    }

    #[inline(always)]
    fn op_jmp_if_false(_vm: &mut VM, frame_idx: usize, opcode: &OpCode) -> StepResult {
        if let OpCode::JmpIfFalse(offset) = opcode {
            let v = _vm.stack.pop().expect("Stack underflow");
            let is_false = if let Some(b) = v.as_boolean() {
                !b
            } else if let Some(n) = v.as_number() {
                n == 0.0
            } else {
                false
            };
            if is_false {
                _vm.call_stack[frame_idx].ip = *offset;
            }
        }
        StepResult::Normal
    }

    #[inline(always)]
    fn op_jmp_if_true(_vm: &mut VM, frame_idx: usize, opcode: &OpCode) -> StepResult {
        if let OpCode::JmpIfTrue(offset) = opcode {
            let v = _vm.stack.pop().expect("Stack underflow");
            let is_true = if let Some(b) = v.as_boolean() {
                b
            } else if let Some(n) = v.as_number() {
                n != 0.0
            } else {
                false
            };
            if is_true {
                _vm.call_stack[frame_idx].ip = *offset;
            }
        }
        StepResult::Normal
    }

    #[inline(always)]
    fn op_jmp(_vm: &mut VM, frame_idx: usize, opcode: &OpCode) -> StepResult {
        if let OpCode::Jmp(offset) = opcode {
            _vm.call_stack[frame_idx].ip = *offset;
        }
        StepResult::Normal
    }

    /// Force a value if it's a thunk, otherwise return it as-is.
    /// This makes binary operations strict (evaluate thunks before operating on them).
    /// Iteratively evaluates nested thunks until a non-thunk value is obtained.
    /// Uses a trampoline to avoid Rust stack overflow with deep thunk chains.
    fn force_value(&mut self, mut v: Value) -> Value {
        loop {
            if v.is_thunk() {
                // Recursively invoke thunk (handles both regular and composed)
                v = self.invoke_thunk_value_recursive(v);
            } else {
                return v;
            }
        }
    }

    fn binary_add(&mut self) {
        let rhs_val = self.stack.pop().expect("Stack underflow");
        let rhs = self.force_value(rhs_val);
        let lhs_val = self.stack.pop().expect("Stack underflow");
        let lhs = self.force_value(lhs_val);
        if let (Some(a_num), Some(b_num)) = (lhs.as_number(), rhs.as_number()) {
            self.stack.push(Value::number(a_num + b_num));
        } else {
            // If either operand is a string or any other type, convert both to strings and concatenate
            let lhs_str = lhs.value_to_string(&self.heap);
            let rhs_str = rhs.value_to_string(&self.heap);
            let mut result = lhs_str;
            result.push_str(&rhs_str);
            self.stack.push(Value::string_with_heap(result, &mut self.heap));
        }
    }

    fn binary_sub(&mut self) {
        let rhs_val = self.stack.pop().expect("Stack underflow");
        let rhs = self.force_value(rhs_val);
        let lhs_val = self.stack.pop().expect("Stack underflow");
        let lhs = self.force_value(lhs_val);
        match (lhs.as_number(), rhs.as_number()) {
            (Some(a), Some(b)) => self.stack.push(Value::number(a - b)),
            _ => panic!("Subtract operation requires both operands to be numbers"),
        }
    }

    fn binary_mul(&mut self) {
        let rhs_val = self.stack.pop().expect("Stack underflow");
        let rhs = self.force_value(rhs_val);
        let lhs_val = self.stack.pop().expect("Stack underflow");
        let lhs = self.force_value(lhs_val);
        match (lhs.as_number(), rhs.as_number()) {
            (Some(a), Some(b)) => self.stack.push(Value::number(a * b)),
            _ => {
                // Better error message showing actual values and types
                let lhs_type = if lhs.as_number().is_some() { "number" }
                    else if lhs.as_string(&self.heap).is_some() { "string" }
                    else if lhs.as_boolean().is_some() { "boolean" }
                    else if lhs.as_function().is_some() { "function" }
                    else if lhs.is_thunk() { "thunk" }
                    else if lhs.is_none() { "none" }
                    else { "unknown" };
                let rhs_type = if rhs.as_number().is_some() { "number" }
                    else if rhs.as_string(&self.heap).is_some() { "string" }
                    else if rhs.as_boolean().is_some() { "boolean" }
                    else if rhs.as_function().is_some() { "function" }
                    else if rhs.is_thunk() { "thunk" }
                    else if rhs.is_none() { "none" }
                    else { "unknown" };
                let lhs_value = lhs.value_to_string(&self.heap);
                let rhs_value = rhs.value_to_string(&self.heap);
                panic!("Multiply operation requires both operands to be numbers, got lhs={} (type={}), rhs={} (type={})", lhs_value, lhs_type, rhs_value, rhs_type);
            }
        }
    }

    fn binary_div(&mut self) {
        let rhs_val = self.stack.pop().expect("Stack underflow");
        let rhs = self.force_value(rhs_val);
        let lhs_val = self.stack.pop().expect("Stack underflow");
        let lhs = self.force_value(lhs_val);
        match (lhs.as_number(), rhs.as_number()) {
            (Some(a), Some(b)) => self.stack.push(Value::number(a / b)),
            _ => panic!("Divide operation requires both operands to be numbers"),
        }
    }

    fn binary_mod(&mut self) {
        let rhs_val = self.stack.pop().expect("Stack underflow");
        let rhs = self.force_value(rhs_val);
        let lhs_val = self.stack.pop().expect("Stack underflow");
        let lhs = self.force_value(lhs_val);
        match (lhs.as_number(), rhs.as_number()) {
            (Some(a), Some(b)) => self.stack.push(Value::number(a % b)),
            _ => panic!("Modulo operation requires both operands to be numbers"),
        }
    }

    fn binary_pow(&mut self) {
        let rhs_val = self.stack.pop().expect("Stack underflow");
        let rhs = self.force_value(rhs_val);
        let lhs_val = self.stack.pop().expect("Stack underflow");
        let lhs = self.force_value(lhs_val);
        match (lhs.as_number(), rhs.as_number()) {
            (Some(a), Some(b)) => self.stack.push(Value::number(a.powf(b))),
            _ => panic!("Power operation requires both operands to be numbers"),
        }
    }

    fn comparison_eq(&mut self) {
        let rhs_val = self.stack.pop().expect("Stack underflow");
        let rhs = self.force_value(rhs_val);
        let lhs_val = self.stack.pop().expect("Stack underflow");
        let lhs = self.force_value(lhs_val);
        let result = if let (Some(a), Some(b)) = (lhs.as_number(), rhs.as_number()) {
            a == b
        } else if let (Some(a), Some(b)) = (lhs.as_string(&self.heap), rhs.as_string(&self.heap)) {
            a == b
        } else if let (Some(a), Some(b)) = (lhs.as_boolean(), rhs.as_boolean()) {
            a == b
        } else {
            panic!("Comparison == on incompatible types");
        };
        self.stack.push(Value::boolean(result));
    }

    fn comparison_ne(&mut self) {
        let rhs_val = self.stack.pop().expect("Stack underflow");
        let rhs = self.force_value(rhs_val);
        let lhs_val = self.stack.pop().expect("Stack underflow");
        let lhs = self.force_value(lhs_val);
        let result = if let (Some(a), Some(b)) = (lhs.as_number(), rhs.as_number()) {
            a != b
        } else if let (Some(a), Some(b)) = (lhs.as_string(&self.heap), rhs.as_string(&self.heap)) {
            a != b
        } else if let (Some(a), Some(b)) = (lhs.as_boolean(), rhs.as_boolean()) {
            a != b
        } else {
            panic!("Comparison != on incompatible types");
        };
        self.stack.push(Value::boolean(result));
    }

    fn comparison_gt(&mut self) {
        let rhs_val = self.stack.pop().expect("Stack underflow");
        let rhs = self.force_value(rhs_val);
        let lhs_val = self.stack.pop().expect("Stack underflow");
        let lhs = self.force_value(lhs_val);
        let result = if let (Some(a), Some(b)) = (lhs.as_number(), rhs.as_number()) {
            a > b
        } else if let (Some(a), Some(b)) = (lhs.as_string(&self.heap), rhs.as_string(&self.heap)) {
            a > b
        } else if let (Some(a), Some(b)) = (lhs.as_boolean(), rhs.as_boolean()) {
            let a_num = if a { 1.0 } else { 0.0 };
            let b_num = if b { 1.0 } else { 0.0 };
            a_num > b_num
        } else {
            panic!("Comparison > on incompatible types");
        };
        self.stack.push(Value::boolean(result));
    }

    fn comparison_lt(&mut self) {
        let rhs_val = self.stack.pop().expect("Stack underflow");
        let rhs = self.force_value(rhs_val);
        let lhs_val = self.stack.pop().expect("Stack underflow");
        let lhs = self.force_value(lhs_val);
        let result = if let (Some(a), Some(b)) = (lhs.as_number(), rhs.as_number()) {
            a < b
        } else if let (Some(a), Some(b)) = (lhs.as_string(&self.heap), rhs.as_string(&self.heap)) {
            a < b
        } else if let (Some(a), Some(b)) = (lhs.as_boolean(), rhs.as_boolean()) {
            let a_num = if a { 1.0 } else { 0.0 };
            let b_num = if b { 1.0 } else { 0.0 };
            a_num < b_num
        } else {
            panic!("Comparison < on incompatible types");
        };
        self.stack.push(Value::boolean(result));
    }

    fn comparison_ge(&mut self) {
        let rhs_val = self.stack.pop().expect("Stack underflow");
        let rhs = self.force_value(rhs_val);
        let lhs_val = self.stack.pop().expect("Stack underflow");
        let lhs = self.force_value(lhs_val);
        let result = if let (Some(a), Some(b)) = (lhs.as_number(), rhs.as_number()) {
            a >= b
        } else if let (Some(a), Some(b)) = (lhs.as_string(&self.heap), rhs.as_string(&self.heap)) {
            a >= b
        } else if let (Some(a), Some(b)) = (lhs.as_boolean(), rhs.as_boolean()) {
            let a_num = if a { 1.0 } else { 0.0 };
            let b_num = if b { 1.0 } else { 0.0 };
            a_num >= b_num
        } else {
            panic!("Comparison >= on incompatible types");
        };
        self.stack.push(Value::boolean(result));
    }

    fn comparison_le(&mut self) {
        let rhs_val = self.stack.pop().expect("Stack underflow");
        let rhs = self.force_value(rhs_val);
        let lhs_val = self.stack.pop().expect("Stack underflow");
        let lhs = self.force_value(lhs_val);
        let result = if let (Some(a), Some(b)) = (lhs.as_number(), rhs.as_number()) {
            a <= b
        } else if let (Some(a), Some(b)) = (lhs.as_string(&self.heap), rhs.as_string(&self.heap)) {
            a <= b
        } else if let (Some(a), Some(b)) = (lhs.as_boolean(), rhs.as_boolean()) {
            let a_num = if a { 1.0 } else { 0.0 };
            let b_num = if b { 1.0 } else { 0.0 };
            a_num <= b_num
        } else {
            panic!("Comparison <= on incompatible types");
        };
        self.stack.push(Value::boolean(result));
    }

    fn to_bool(value: &Value) -> bool {
        if let Some(b) = value.as_boolean() {
            b
        } else if let Some(n) = value.as_number() {
            n != 0.0
        } else {
            panic!("Cannot convert to boolean");
        }
    }

    fn logical_and(&mut self) {
        let rhs_val = self.stack.pop().expect("Stack underflow");
        let rhs = self.force_value(rhs_val);
        let lhs_val = self.stack.pop().expect("Stack underflow");
        let lhs = self.force_value(lhs_val);
        let result = Self::to_bool(&lhs) && Self::to_bool(&rhs);
        self.stack.push(Value::boolean(result));
    }

    fn logical_or(&mut self) {
        let rhs_val = self.stack.pop().expect("Stack underflow");
        let rhs = self.force_value(rhs_val);
        let lhs_val = self.stack.pop().expect("Stack underflow");
        let lhs = self.force_value(lhs_val);
        let result = Self::to_bool(&lhs) || Self::to_bool(&rhs);
        self.stack.push(Value::boolean(result));
    }

    // ============================================================================
    // Helper functions for reducing code duplication
    // ============================================================================

    /// Initialize local variables from function parameters.
    /// Returns a Box<[Value]> suitable for CallFrame locals.
    fn init_locals_from_args(bytecode_func: &crate::core::engine::BytecodeFunction, args: &[Value]) -> Box<[Value]> {
        let max_var_id = bytecode_func.param_var_ids.iter()
            .max()
            .copied()
            .unwrap_or(0);
        
        let mut locals = vec![Value::none(); (max_var_id + 1) as usize];
        for (i, param_var_id) in bytecode_func.param_var_ids.iter().enumerate() {
            if i < args.len() {
                // Pass arguments as-is (including thunks) - they'll be forced when used
                locals[*param_var_id as usize] = args[i];
            }
        }
        locals.into_boxed_slice()
    }

    /// Extract the minimum arity value from an Arity enum.
    /// For Fixed(n), returns n. For Variadic { min }, returns min.
    fn min_arity(arity: &Arity) -> usize {
        match arity {
            Arity::Fixed(n) => *n,
            Arity::Variadic { min } => *min,
        }
    }

    /// Call a native function with the given arguments and return the result as a Value.
    fn call_native_function(&mut self, func_id: u32, args: Vec<Value>) -> Value {
        let native_func = self.engine.functions.get(&func_id)
            .expect("Native function should exist");
        (native_func.func)(args, &mut self.heap)
    }

    /// Create a new call frame for a bytecode function.
    /// Returns the frame count before pushing (for execute_until_empty target).
    fn create_bytecode_frame(&mut self, bytecode_func: &crate::core::engine::BytecodeFunction, args: Vec<Value>) -> usize {
        if bytecode_func.code.is_empty() {
            panic!("Function has empty bytecode body");
        }
        
        let locals = Self::init_locals_from_args(bytecode_func, &args);
        let stack_depth = self.stack.len();
        let initial_frame_count = self.call_stack.len();
        
        self.call_stack.push(CallFrame {
            code: bytecode_func.code,
            ip: 0,
            locals,
            stack_depth,
        });
        
        initial_frame_count
    }

    /// Fill a hole in a thunk with a value.
    /// Returns the updated bound args with the hole filled.
    fn fill_thunk_hole(
        bound_args: &[Option<Value>],
        value: Value,
    ) -> Vec<Option<Value>> {
        let mut filled = bound_args.to_vec();
        let mut filled_hole = false;
        
        for slot in filled.iter_mut() {
            if slot.is_none() && !filled_hole {
                *slot = Some(value);
                filled_hole = true;
                break;
            }
        }
        
        if !filled_hole {
            panic!("Cannot apply value to thunk - no holes available");
        }
        
        filled
    }

    /// Apply an argument to a thunk or function, creating a new thunk.
    /// Handles regular thunks and functions.
    fn apply_arg_to_thunk(&mut self, thunk_or_func: Value, arg: Value) -> Value {
        if let Some(ThunkData::Regular { func_id, bound }) = thunk_or_func.as_thunk_ref(&self.heap) {
            // Regular thunk: fill the first hole or add to the end
            let mut filled = bound.clone();
            let mut filled_hole = false;
            
            // Try to fill the first hole
            for slot in filled.iter_mut() {
                if slot.is_none() {
                    *slot = Some(arg);
                    filled_hole = true;
                    break;
                }
            }
            
            // If no holes, we need to know the function's arity to determine if we should add to the end
            // For now, if all slots are filled, we'll add to the end (for variadic functions)
            if !filled_hole {
                filled.push(Some(arg));
            }
            
            Value::thunk_with_heap(*func_id, filled, &mut self.heap)
        } else if let Some((func_id, existing_args)) = thunk_or_func.as_thunk(&self.heap) {
            // Regular thunk: fill the first hole or add to the end
            let mut filled = existing_args.clone();
            let mut filled_hole = false;
            
            // Try to fill the first hole
            for slot in filled.iter_mut() {
                if slot.is_none() {
                    *slot = Some(arg);
                    filled_hole = true;
                    break;
                }
            }
            
            // If no holes, add to the end
            if !filled_hole {
                filled.push(Some(arg));
            }
            
            Value::thunk_with_heap(func_id, filled, &mut self.heap)
        } else if let Some(func_id) = thunk_or_func.as_function() {
            // Function: create thunk with arg (and holes for remaining args)
            // Get function arity to create proper holes
            let arity = if let Some(native_func) = self.engine.functions.get(&func_id) {
                Self::min_arity(&native_func.arity)
            } else if let Some(bytecode_func) = self.engine.bytecode_functions.get(&func_id) {
                bytecode_func.param_var_ids.len()
            } else {
                1 // Unknown - assume unary
            };
            
            let mut bound = vec![Some(arg)];
            // Add holes for remaining arguments
            while bound.len() < arity {
                bound.push(None);
            }
            
            Value::thunk_with_heap(func_id, bound, &mut self.heap)
        } else {
            panic!("Cannot apply argument to non-thunk/non-function value");
        }
    }

    fn execute_call_stack(&mut self, n_args: u32) {
        // Convert CallStack to thunk-based invocation for consistency.
        // This maintains backward compatibility with old bytecode while using the unified thunk system.
        let n_args = n_args as usize;
        // Pop function reference
        let func_val = self.stack.pop().expect("Stack underflow");
        let func_id = func_val.as_function().expect("Expected function on stack");

        // Pop arguments
        let args: Vec<Value> = pop_n(&mut self.stack, n_args);

        // Create a thunk with all arguments bound (no holes)
        let bound: Vec<Option<Value>> = args.into_iter().map(Some).collect();
        let thunk = Value::thunk_with_heap(func_id, bound, &mut self.heap);
        
        // Invoke the thunk (which will immediately execute since all holes are filled)
        let result = self.invoke_thunk_value_recursive(thunk);
        self.stack.push(result);
    }

    fn execute_return(&mut self) {
        // CRITICAL: The return value should be on top of the stack.
        // Pop return value (or use None if stack is empty)
        let mut return_value = self.stack.pop().unwrap_or(Value::none());
        let return_str = return_value.value_to_string(&self.heap);

        // Auto-invoke thunks at function boundaries (thunks are lazy internally but strict at boundaries)
        if return_value.is_thunk() {
            return_value = self.invoke_thunk_value_recursive(return_value);
        }

        // Get the stack depth that was saved when this frame was entered
        let expected_stack_depth = if let Some(frame) = self.call_stack.last() {
            frame.stack_depth
        } else {
            0
        };

        // Clean up any intermediate values that the function left on the stack
        // The stack should be at expected_stack_depth + 1 (the return value we just popped)
        // but there might be extra values, so we clean up to expected_stack_depth
        while self.stack.len() > expected_stack_depth {
            self.stack.pop();
        }

        // Pop the current frame (this removes it from call_stack)
        self.call_stack.pop();

        // Push return value back on stack
        self.stack.push(return_value);
        // Execution will continue in the main loop with the previous frame
    }

    /// Recursively invoke a thunk Value (handles both regular and composed thunks)
    fn invoke_thunk_value_recursive(&mut self, thunk: Value) -> Value {
        if !thunk.is_thunk() {
            return thunk;
        }

        if let Some(ThunkData::Composed { first, second }) = thunk.as_thunk_ref(&self.heap) {
            // Composed thunk: invoke first, then second with result
            // For g(f(x)): invoke f first, then invoke g with f's result
            // Clone the Values to avoid borrow checker issues
            let first_val = *first;
            let second_val = *second;
            let first_result = self.invoke_thunk_value_recursive(first_val);
            // Now invoke second with first_result as argument
            // CRITICAL: Don't use execute_prepare_call here because it may call invoke_thunk_sync
            // which truncates the stack, removing first_result. Instead, manually combine
            // the arguments and create the thunk directly.
            // Check if second_val is a regular thunk with holes
            if let Some(ThunkData::Regular { func_id, bound }) = second_val.as_thunk_ref(&self.heap) {
                // Regular thunk - apply first_result to fill the first hole
                let mut new_bound = bound.clone();
                let mut filled_hole = false;
                for slot in new_bound.iter_mut() {
                    if slot.is_none() && !filled_hole {
                        *slot = Some(first_result);
                        filled_hole = true;
                        break;
                    }
                }
                if !filled_hole {
                    panic!("Cannot apply value to thunk - no holes available");
                }
                // Check if all holes are filled
                let remaining_holes = new_bound.iter().filter(|opt| opt.is_none()).count();
                if remaining_holes == 0 {
                    // All holes filled - extract values and invoke directly
                    let final_args: Vec<Value> = new_bound.into_iter()
                        .map(|opt| opt.expect("All holes should be filled"))
                        .collect();
                    return self.call_function(*func_id, final_args);
                } else {
                    // Still has holes - create a new thunk
                    let thunk = Value::thunk_with_heap(*func_id, new_bound, &mut self.heap);
                    return self.invoke_thunk_value_recursive(thunk);
                }
            } else if let Some((thunk_func_id, thunk_args)) = second_val.as_thunk(&self.heap) {
                // Regular thunk - fill first hole or add to end
                let mut final_args = thunk_args;
                let mut filled_hole = false;
                // Try to fill the first hole
                for slot in final_args.iter_mut() {
                    if slot.is_none() {
                        *slot = Some(first_result);
                        filled_hole = true;
                        break;
                    }
                }
                // If no holes, add to end
                if !filled_hole {
                    final_args.push(Some(first_result));
                }
                let prepared = Value::thunk_with_heap(thunk_func_id, final_args, &mut self.heap);
                // Recursively invoke the prepared thunk
                let result = self.invoke_thunk_value_recursive(prepared);
                return result;
            } else if let Some(ThunkData::Composed { first: nested_first, second: nested_second }) = second_val.as_thunk_ref(&self.heap) {
                // Composed thunk - recursively apply first_result to nested_first
                let nested_first_val = *nested_first;
                let nested_second_val = *nested_second;
                // Recursively apply first_result to nested_first
                let prepared_nested = if let Some((nested_func_id, nested_args)) = nested_first_val.as_thunk(&self.heap) {
                    let mut nested_final_args = nested_args;
                    let mut filled_hole = false;
                    // Try to fill the first hole
                    for slot in nested_final_args.iter_mut() {
                        if slot.is_none() {
                            *slot = Some(first_result);
                            filled_hole = true;
                            break;
                        }
                    }
                    // If no holes, add to end
                    if !filled_hole {
                        nested_final_args.push(Some(first_result));
                    }
                    Value::thunk_with_heap(nested_func_id, nested_final_args, &mut self.heap)
                } else if let Some(nested_func_id) = nested_first_val.as_function() {
                    let arity = if let Some(native_func) = self.engine.functions.get(&nested_func_id) {
                        Self::min_arity(&native_func.arity)
                    } else if let Some(bytecode_func) = self.engine.bytecode_functions.get(&nested_func_id) {
                        bytecode_func.param_var_ids.len()
                    } else {
                        1
                    };
                    let mut bound = vec![Some(first_result)];
                    while bound.len() < arity {
                        bound.push(None);
                    }
                    Value::thunk_with_heap(nested_func_id, bound, &mut self.heap)
                } else {
                    // Nested first is itself a composed thunk - use invoke_thunk_value_recursive to handle it
                    // This avoids the stack truncation issue in execute_prepare_call
                    // We need to apply first_result to nested_first_val, which is a composed thunk
                    // So we recursively call invoke_thunk_value_recursive which will handle it correctly
                    let nested_result = self.invoke_thunk_value_recursive(nested_first_val);
                    // Now we have nested_result, but we need to apply first_result to nested_first_val
                    // Actually, we need to create a thunk that applies first_result to nested_first_val
                    // Since nested_first_val is a composed thunk, we need to apply first_result to its first part
                    if let Some(ThunkData::Composed { first: deep_first, second: deep_second }) = nested_first_val.as_thunk_ref(&self.heap) {
                        let deep_first_val = *deep_first;
                        let deep_second_val = *deep_second;
                        // Apply first_result to deep_first
                        if let Some((deep_func_id, deep_args)) = deep_first_val.as_thunk(&self.heap) {
                            let mut deep_final_args = deep_args;
                            let mut filled_hole = false;
                            for slot in deep_final_args.iter_mut() {
                                if slot.is_none() {
                                    *slot = Some(first_result);
                                    filled_hole = true;
                                    break;
                                }
                            }
                            if !filled_hole {
                                deep_final_args.push(Some(first_result));
                            }
                            Value::thunk_with_heap(deep_func_id, deep_final_args, &mut self.heap)
                            } else if let Some(deep_func_id) = deep_first_val.as_function() {
                            let arity = if let Some(native_func) = self.engine.functions.get(&deep_func_id) {
                                Self::min_arity(&native_func.arity)
                            } else if let Some(bytecode_func) = self.engine.bytecode_functions.get(&deep_func_id) {
                                bytecode_func.param_var_ids.len()
                            } else {
                                1
                            };
                            let mut bound = vec![Some(first_result)];
                            while bound.len() < arity {
                                bound.push(None);
                            }
                            Value::thunk_with_heap(deep_func_id, bound, &mut self.heap)
                        } else {
                            // Deep first is also a composed thunk - recurse
                            let deep_prepared = self.invoke_thunk_value_recursive(deep_first_val);
                            // But we need to apply first_result to deep_first_val, not get its result
                            // This is getting too complex - let's use a simpler approach
                            // Create a thunk that will apply first_result when invoked
                            // Actually, the simplest is to just recurse on the whole thing
                            // Skip the temp_composed approach - it's incorrect
                            // No wait, that's wrong. Let me think...
                            // We need: apply first_result to nested_first_val where nested_first_val is a composed thunk
                            // The correct way: apply first_result to the first part of nested_first_val
                            // Since we already have deep_first_val and deep_second_val, we can do:
                            let deep_first_prepared = if let Some((df_id, df_args)) = deep_first_val.as_thunk(&self.heap) {
                                let mut df_final = df_args;
                                let mut filled_hole = false;
                                for slot in df_final.iter_mut() {
                                    if slot.is_none() {
                                        *slot = Some(first_result);
                                        filled_hole = true;
                                        break;
                                    }
                                }
                                if !filled_hole {
                                    df_final.push(Some(first_result));
                                }
                                Value::thunk_with_heap(df_id, df_final, &mut self.heap)
                            } else if let Some(df_id) = deep_first_val.as_function() {
                                let arity = if let Some(native_func) = self.engine.functions.get(&df_id) {
                                    Self::min_arity(&native_func.arity)
                                } else if let Some(bytecode_func) = self.engine.bytecode_functions.get(&df_id) {
                                    bytecode_func.param_var_ids.len()
                                } else {
                                    1
                                };
                                let mut bound = vec![Some(first_result)];
                                while bound.len() < arity {
                                    bound.push(None);
                                }
                                Value::thunk_with_heap(df_id, bound, &mut self.heap)
                            } else {
                                // deep_first is also composed - this is getting too nested
                                // For now, just panic and we can handle it later if needed
                                panic!("Triple-nested composed thunks not yet supported");
                            };
                            Value::composed_thunk_with_heap(deep_first_prepared, deep_second_val, &mut self.heap)
                        }
                    } else {
                        panic!("Expected nested_first_val to be a composed thunk");
                    }
                };
                let recomposed = Value::composed_thunk_with_heap(prepared_nested, nested_second_val, &mut self.heap);
                return self.invoke_thunk_value_recursive(recomposed);
            } else if let Some(func_id) = second_val.as_function() {
                // It's a function - create thunk with first_result as arg (and holes for remaining args)
                let arity = if let Some(native_func) = self.engine.functions.get(&func_id) {
                    Self::min_arity(&native_func.arity)
                } else if let Some(bytecode_func) = self.engine.bytecode_functions.get(&func_id) {
                    bytecode_func.param_var_ids.len()
                } else {
                    1
                };
                let mut bound = vec![Some(first_result)];
                while bound.len() < arity {
                    bound.push(None);
                }
                let prepared = Value::thunk_with_heap(func_id, bound, &mut self.heap);
                return self.invoke_thunk_value_recursive(prepared);
            } else {
                panic!("Second part of composed thunk must be a function or thunk");
            }
        } else if let Some(ThunkData::Regular { func_id, bound }) = thunk.as_thunk_ref(&self.heap) {
            // Regular thunk: check if all holes are filled
            let hole_count = bound.iter().filter(|opt| opt.is_none()).count();
            if hole_count == 0 {
                // No holes - extract values and invoke
                let final_args: Vec<Value> = bound.iter()
                    .filter_map(|opt| *opt)
                    .collect();
                let result = self.call_function(*func_id, final_args);
                return result;
            } else {
                panic!("Thunk with {} holes cannot be invoked without arguments", hole_count);
            }
        } else if let Some((func_id, args)) = thunk.as_thunk(&self.heap) {
            // Regular thunk: check if all holes are filled
            let hole_count = args.iter().filter(|opt| opt.is_none()).count();
            if hole_count == 0 {
                // All holes filled - extract values and invoke
                let final_args: Vec<Value> = args.into_iter()
                    .filter_map(|opt| opt)
                    .collect();
                let result = self.call_function(func_id, final_args);
                return result;
            } else {
                panic!("Thunk with {} holes cannot be invoked without arguments", hole_count);
            }
        } else {
            panic!("Invalid thunk value");
        }
    }

    /// Invoke a thunk with additional arguments, filling holes left-to-right.
    /// Returns a new thunk if not all holes are filled, otherwise invokes the function.
    fn invoke_thunk(&mut self, thunk_val: Value, args: Vec<Value>) -> Value {
        if !thunk_val.is_thunk() {
            panic!("invoke_thunk called on non-thunk value");
        }
        
        if let Some(ThunkData::Regular { func_id, bound }) = thunk_val.as_thunk_ref(&self.heap) {
            let mut filled = bound.clone();
            let mut arg_iter = args.into_iter();
            
            // Fill holes left-to-right
            for slot in filled.iter_mut() {
                if slot.is_none() {
                    if let Some(arg) = arg_iter.next() {
                        *slot = Some(arg);
                    }
                }
            }
            
            // Check if all holes are filled
            if filled.iter().any(|s| s.is_none()) {
                // Still has holes - return new thunk
                Value::thunk_with_heap(*func_id, filled, &mut self.heap)
            } else {
                // All holes filled - extract values and invoke
                let final_args: Vec<Value> = filled.into_iter()
                    .map(|opt| opt.expect("All holes should be filled"))
                    .collect();
                self.call_function(*func_id, final_args)
            }
        } else if let Some(ThunkData::Composed { first, second }) = thunk_val.as_thunk_ref(&self.heap) {
            // For composition: g(f(x))
            // First invoke first with args, then invoke second with result
            let first_val = *first;
            let second_val = *second;
            
            let first_result = self.invoke_thunk(first_val, args);
            
            // Now invoke second with first_result
            self.invoke_thunk(second_val, vec![first_result])
        } else {
            panic!("Invalid thunk structure");
        }
    }
    
    /// Call a function (native or bytecode) with the given arguments.
    fn call_function(&mut self, func_id: u32, args: Vec<Value>) -> Value {
        if func_id == COMPOSE_ID {
            panic!("COMPOSE_ID should not be used directly in call_function");
        }
        
        if self.engine.functions.contains_key(&func_id) {
            // Native function: invoke and get result directly
            self.call_native_function(func_id, args)
        } else if let Some(bytecode_func) = self.engine.bytecode_functions.get(&func_id) {
            // Bytecode function: create a frame and execute until it returns
            // Safety check: ensure we have enough arguments
            let required_params = bytecode_func.param_var_ids.len();
            if args.len() < required_params {
                let args_debug: Vec<String> = args.iter().map(|v| v.value_to_string(&self.heap)).collect();
                panic!("Attempted to invoke function {} with {} args but it requires {}. Args: {:?}", 
                    func_id, args.len(), required_params, args_debug);
            }
            
            // Push new frame with function bytecode
            let initial_frame_count = self.create_bytecode_frame(bytecode_func, args);

            // Execute until the function we just pushed returns
            // Use the shared execution method to avoid nested loops
            self.execute_until_empty(Some(initial_frame_count));

            // The function returned, result should be on the stack
            // CRITICAL: Pop the return value and restore stack to pre-thunk depth
            let stack_base = self.stack.len() - 1; // Account for return value
            let result = if self.stack.len() > stack_base {
                self.stack.pop().expect("Function should have returned a value")
            } else {
                Value::none() // TODO: check if this is correct
            };
            // HARD RESET: Truncate stack to pre-thunk depth (removes all intermediate stack junk)
            self.stack.truncate(stack_base);
            result
        } else {
            panic!("Function {} not found (neither native nor bytecode)", func_id);
        }
    }

    fn invoke_thunk_sync(&mut self, func_id: u32, args: Vec<Value>) -> Value {
        // CRITICAL: Capture stack depth before thunk execution
        // This ensures we restore the stack to its pre-thunk state after execution
        let stack_base = self.stack.len();
        
        // CRITICAL: The args parameter should already contain both captured_args + runtime_args.
        // This is because when a thunk is invoked, execute_prepare_call combines the thunk's
        // captured args with any runtime args from the stack. So args here is the full argument
        // list that should be passed to the function.
        // Example: add10 = add(10) creates a thunk with captured_args=[10]
        //          add10!(5) should combine [10] (captured) + [5] (runtime) = [10, 5]
        //          So invoke_thunk_sync should receive args=[10, 5], not just [5]
        
        // Check if it's a native function or bytecode function
        if self.engine.functions.contains_key(&func_id) {
            // Native function: invoke and get result directly
            self.call_native_function(func_id, args)
        } else if let Some(bytecode_func) = self.engine.bytecode_functions.get(&func_id) {
            // Bytecode function: create a frame and execute until it returns
            // Safety check: ensure we have enough arguments
            let required_params = bytecode_func.param_var_ids.len();
            if args.len() < required_params {
                let args_debug: Vec<String> = args.iter().map(|v| v.value_to_string(&self.heap)).collect();
                panic!("Attempted to invoke function {} with {} args but it requires {}. Args: {:?}", 
                    func_id, args.len(), required_params, args_debug);
            }
            
            // Push new frame with function bytecode
            let initial_frame_count = self.create_bytecode_frame(bytecode_func, args);

            // Execute until the function we just pushed returns
            // Use the shared execution method to avoid nested loops
            self.execute_until_empty(Some(initial_frame_count));

            // The function returned, result should be on the stack
            // CRITICAL: Pop the return value and restore stack to pre-thunk depth
            let result = if self.stack.len() > stack_base {
                self.stack.pop().expect("Function should have returned a value")
            } else {
                Value::none() // TODO: check if this is correct
            };
            // HARD RESET: Truncate stack to pre-thunk depth (removes all intermediate stack junk)
            self.stack.truncate(stack_base);
            result
        } else {
            panic!("Function {} not found (neither native nor bytecode)", func_id);
        }
    }

    fn execute_prepare_call(&mut self, n_args: u32) {
        let n_args = n_args as usize;
        
        // Pop function reference (could be a function or a thunk)
        // The function/thunk is on top of the stack, with new args below it
        let func_val = self.stack.pop().expect("Stack underflow");
        
        // Handle both functions and thunks
        // CRITICAL: Check for thunks FIRST, because a thunk should never be treated as a function
        // If we check functions first, we might incorrectly treat a thunk as a function
        if func_val.is_thunk() {
            // Check if it's a regular thunk with holes
            if let Some(ThunkData::Regular { func_id, bound }) = func_val.as_thunk_ref(&self.heap) {
                // Pop new arguments from the stack (these fill the holes)
                let new_args: Vec<Value> = pop_n(&mut self.stack, n_args);
                
                // Fill holes with new arguments (left-to-right order)
                let mut filled = bound.clone();
                let mut arg_iter = new_args.into_iter();
                
                for slot in filled.iter_mut() {
                    if slot.is_none() {
                        if let Some(arg) = arg_iter.next() {
                            *slot = Some(arg);
                        }
                    }
                }
                
                // Create a thunk with filled arguments (may still have holes if not enough args provided)
                let thunk = Value::thunk_with_heap(*func_id, filled, &mut self.heap);
                self.stack.push(thunk);
                return;
            }
            // Check if it's a composed thunk
            if let Some(ThunkData::Composed { first, second }) = func_val.as_thunk_ref(&self.heap) {
                // For composed thunks with arguments: (f |> g)(x)
                // We need to apply the arguments to the first function, then recompose
                // Pop new arguments from the stack
                let new_args: Vec<Value> = pop_n(&mut self.stack, n_args);
                
                // Clone the Values to avoid borrow checker issues
                let first_val = *first;
                let second_val = *second;
                
                // Check if first_val is itself a composed thunk
                if let Some(ThunkData::Composed { first: nested_first, second: nested_second }) = first_val.as_thunk_ref(&self.heap) {
                    // Nested composition: apply args to the entire nested composed thunk
                    // CRITICAL: We need to apply the argument to the nested composition as a whole,
                    // not just to nested_first. The nested composition should be treated as a single unit.
                    // We'll create a thunk that applies the argument to the nested composition,
                    // then recompose with the outer second function.
                    let nested_composed = first_val; // The nested composition itself
                    // Apply new_args to the nested composition by creating a thunk that applies the args
                    // to nested_first, then we'll recompose properly
                    let nested_first_val = *nested_first;
                    let nested_second_val = *nested_second;
                    // Apply new_args to nested_first_val (the first part of the nested composition)
                    let prepared_nested_first = if new_args.len() == 1 {
                        // Single argument - apply it to nested_first_val
                        // Check for regular thunk with holes
                        if let Some(ThunkData::Regular { func_id, bound }) = nested_first_val.as_thunk_ref(&self.heap) {
                            // Regular thunk: fill the first hole with the argument
                            let mut filled = bound.clone();
                            let mut filled_hole = false;
                            for slot in filled.iter_mut() {
                                if slot.is_none() && !filled_hole {
                                    *slot = Some(new_args[0]);
                                    filled_hole = true;
                                    break;
                                }
                            }
                            if !filled_hole {
                                panic!("No holes available in thunk");
                            }
                            // Create a thunk with filled argument
                            Value::thunk_with_heap(*func_id, filled, &mut self.heap)
                        } else if let Some((nf_id, nf_args)) = nested_first_val.as_thunk(&self.heap) {
                            // Regular thunk: fill first hole or add to end
                            let mut nf_final = nf_args;
                            let mut filled_hole = false;
                            for slot in nf_final.iter_mut() {
                                if slot.is_none() {
                                    *slot = Some(new_args[0]);
                                    filled_hole = true;
                                    break;
                                }
                            }
                            if !filled_hole {
                                nf_final.push(Some(new_args[0]));
                            }
                            Value::thunk_with_heap(nf_id, nf_final, &mut self.heap)
                        } else if let Some(nf_id) = nested_first_val.as_function() {
                            // Function: create thunk with new arg (and holes for remaining args)
                            let arity = if let Some(native_func) = self.engine.functions.get(&nf_id) {
                                Self::min_arity(&native_func.arity)
                            } else if let Some(bytecode_func) = self.engine.bytecode_functions.get(&nf_id) {
                                bytecode_func.param_var_ids.len()
                            } else {
                                1
                            };
                            let mut bound = vec![Some(new_args[0])];
                            while bound.len() < arity {
                                bound.push(None);
                            }
                            Value::thunk_with_heap(nf_id, bound, &mut self.heap)
                        } else if let Some(ThunkData::Composed { first: deep_first, second: deep_second }) = nested_first_val.as_thunk_ref(&self.heap) {
                            // nested_first_val is itself a composed thunk - recursively apply the arg
                            let deep_first_val = *deep_first;
                            let deep_second_val = *deep_second;
                            let deep_prepared = if let Some(ThunkData::Regular { func_id, bound }) = deep_first_val.as_thunk_ref(&self.heap) {
                                // Deep first is a regular thunk with holes
                                let mut filled = bound.clone();
                                let mut filled_hole = false;
                                for slot in filled.iter_mut() {
                                    if slot.is_none() && !filled_hole {
                                        *slot = Some(new_args[0]);
                                        filled_hole = true;
                                        break;
                                    }
                                }
                                if !filled_hole {
                                    panic!("No holes available in deep thunk");
                                }
                                Value::thunk_with_heap(*func_id, filled, &mut self.heap)
                            } else if let Some((df_id, df_args)) = deep_first_val.as_thunk(&self.heap) {
                                let mut df_final = df_args;
                                let mut filled_hole = false;
                                for slot in df_final.iter_mut() {
                                    if slot.is_none() {
                                        *slot = Some(new_args[0]);
                                        filled_hole = true;
                                        break;
                                    }
                                }
                                if !filled_hole {
                                    df_final.push(Some(new_args[0]));
                                }
                                Value::thunk_with_heap(df_id, df_final, &mut self.heap)
                            } else if let Some(df_id) = deep_first_val.as_function() {
                                let arity = if let Some(native_func) = self.engine.functions.get(&df_id) {
                                    Self::min_arity(&native_func.arity)
                                } else if let Some(bytecode_func) = self.engine.bytecode_functions.get(&df_id) {
                                    bytecode_func.param_var_ids.len()
                                } else {
                                    1
                                };
                                let mut bound = vec![Some(new_args[0])];
                                while bound.len() < arity {
                                    bound.push(None);
                                }
                                Value::thunk_with_heap(df_id, bound, &mut self.heap)
                            } else {
                                panic!("Triple-nested composed thunks not yet supported in execute_prepare_call");
                            };
                            Value::composed_thunk_with_heap(deep_prepared, deep_second_val, &mut self.heap)
                        } else {
                            panic!("nested_first_val must be a thunk, function, or composed thunk, got: {:?}", nested_first_val);
                        }
                    } else {
                        panic!("Multiple args to nested composed thunk not yet supported");
                    };
                    // Recompose with nested_second_val to get the nested composition with args applied
                    let recomposed_nested = Value::composed_thunk_with_heap(prepared_nested_first, nested_second_val, &mut self.heap);
                    // Now recompose with the outer second function
                    let composed = Value::composed_thunk_with_heap(recomposed_nested, second_val, &mut self.heap);
                    self.stack.push(composed);
                    return;
                }
                
                // Apply the new args to the first function
                // Handle case where first_val is a value (number, string, etc.) rather than a thunk/function
                // In this case, we treat first_val as an argument to apply to second_val
                if !first_val.is_thunk() && first_val.as_function().is_none() {
                    // first_val is a concrete value (number, string, etc.)
                    // Apply it to second_val as an argument
                    // Check if second_val is a regular thunk with holes
                    if let Some(ThunkData::Regular { func_id, bound }) = second_val.as_thunk_ref(&self.heap) {
                        // Apply first_val to fill the first hole in the thunk
                        let mut filled = bound.clone();
                        let mut filled_hole = false;
                        for slot in filled.iter_mut() {
                            if slot.is_none() && !filled_hole {
                                *slot = Some(first_val);
                                filled_hole = true;
                                break;
                            }
                        }
                        if !filled_hole {
                            panic!("Cannot apply value to thunk - no holes available");
                        }
                        // Fill remaining holes with new_args
                        let mut arg_iter = new_args.into_iter();
                        for slot in filled.iter_mut() {
                            if slot.is_none() {
                                if let Some(arg) = arg_iter.next() {
                                    *slot = Some(arg);
                                }
                            }
                        }
                        // Create a thunk (may still have holes if not enough args)
                        let thunk = Value::thunk_with_heap(*func_id, filled, &mut self.heap);
                        self.stack.push(thunk);
                        return;
                    } else if let Some((second_func_id, second_args)) = second_val.as_thunk(&self.heap) {
                        // second_val is a regular thunk - fill first hole or add to end
                        let mut combined_args = second_args;
                        let mut filled_hole = false;
                        // Try to fill the first hole with first_val
                        for slot in combined_args.iter_mut() {
                            if slot.is_none() {
                                *slot = Some(first_val);
                                filled_hole = true;
                                break;
                            }
                        }
                        // Fill remaining holes with new_args
                        for slot in combined_args.iter_mut() {
                            if slot.is_none() {
                                // We'll fill these in the next loop
                            }
                        }
                        // If no holes were filled, add first_val to end
                        if !filled_hole {
                            combined_args.push(Some(first_val));
                        }
                        // Fill remaining holes with new_args
                        let mut arg_iter = new_args.into_iter();
                        for slot in combined_args.iter_mut() {
                            if slot.is_none() {
                                if let Some(arg) = arg_iter.next() {
                                    *slot = Some(arg);
                                }
                            }
                        }
                        let thunk = Value::thunk_with_heap(second_func_id, combined_args, &mut self.heap);
                        self.stack.push(thunk);
                        return;
                    } else if let Some(second_func_id) = second_val.as_function() {
                        // second_val is a function - create a thunk with first_val and new_args
                        let arity = if let Some(native_func) = self.engine.functions.get(&second_func_id) {
                            Self::min_arity(&native_func.arity)
                        } else if let Some(bytecode_func) = self.engine.bytecode_functions.get(&second_func_id) {
                            bytecode_func.param_var_ids.len()
                        } else {
                            1 + new_args.len()
                        };
                        let mut bound = vec![Some(first_val)];
                        bound.extend(new_args.into_iter().map(Some));
                        while bound.len() < arity {
                            bound.push(None);
                        }
                        let thunk = Value::thunk_with_heap(second_func_id, bound, &mut self.heap);
                        self.stack.push(thunk);
                        return;
                    } else {
                        panic!("Second part of composed thunk must be a function or thunk when first is a value");
                    }
                }
                
                // first_val is a thunk or function - apply new args to it
                let (first_func_id, first_existing_args) = if let Some((func_id, args)) = first_val.as_thunk(&self.heap) {
                    (func_id, args)
                } else if let Some(func_id) = first_val.as_function() {
                    (func_id, Vec::new())
                } else {
                    panic!("First part of composed thunk must be a function or thunk");
                };
                
                // Combine first's existing args with new args
                // Fill holes in first_existing_args with new_args
                let mut first_final_args = first_existing_args;
                let mut arg_iter = new_args.into_iter();
                // Fill holes first
                for slot in first_final_args.iter_mut() {
                    if slot.is_none() {
                        if let Some(arg) = arg_iter.next() {
                            *slot = Some(arg);
                        }
                    }
                }
                // Add any remaining new_args to the end
                for arg in arg_iter {
                    first_final_args.push(Some(arg));
                }
                
                
                // Create a thunk for the first function with all args applied
                let first_with_args = Value::thunk_with_heap(first_func_id, first_final_args, &mut self.heap);
                
                // Recompose with the second function
                let composed = Value::composed_thunk_with_heap(first_with_args, second_val, &mut self.heap);
                
                // Push the recomposed thunk onto the stack
                self.stack.push(composed);
                return;
            }
            // Regular thunk
            if let Some((thunk_func_id, thunk_args)) = func_val.as_thunk(&self.heap) {
                // It's a thunk - extract the function and existing args
                // Note: as_thunk already clones the args, so we don't need to clone again
                // CRITICAL: If this thunk has existing args, we MUST preserve them when combining with new args
                // The existing args represent a partial application that we're continuing
                let mut existing_args = thunk_args;
                
                // Pop new arguments from the stack
                let new_args: Vec<Value> = pop_n(&mut self.stack, n_args);
                
                // Fill holes in existing_args with new_args, then add any remaining to the end
                let mut arg_iter = new_args.into_iter();
                // Fill holes first
                for slot in existing_args.iter_mut() {
                    if slot.is_none() {
                        if let Some(arg) = arg_iter.next() {
                            *slot = Some(arg);
                        }
                    }
                }
                // Add any remaining new_args to the end
                for arg in arg_iter {
                    existing_args.push(Some(arg));
                }
                
                let final_args = existing_args;
                

                // Create a Thunk value with the function and all args
                let prepared_call = Value::thunk_with_heap(thunk_func_id, final_args, &mut self.heap);

                // Push the prepared call onto the stack
                self.stack.push(prepared_call);
                return;
            } else {
                panic!("Expected thunk but as_thunk returned None");
            }
        } else if let Some(func_id) = func_val.as_function() {
            // It's a function - use it directly with no existing args
            // Pop new arguments from the stack
            let new_args: Vec<Value> = pop_n(&mut self.stack, n_args);
            
            // Get the function's arity to create thunk with proper holes
            let arity = if let Some(native_func) = self.engine.functions.get(&func_id) {
                Self::min_arity(&native_func.arity)
            } else if let Some(bytecode_func) = self.engine.bytecode_functions.get(&func_id) {
                bytecode_func.param_var_ids.len()
            } else {
                // Unknown function - assume all args are provided (no holes)
                new_args.len()
            };
            
            // Create a thunk with bound arguments and holes for missing ones
            let mut bound: Vec<Option<Value>> = new_args.into_iter().map(Some).collect();
            // Add holes for remaining arguments
            while bound.len() < arity {
                bound.push(None);
            }
            
            // Create a Thunk value with the function, bound args, and holes
            let prepared_call = Value::thunk_with_heap(func_id, bound, &mut self.heap);

            // Push the prepared call onto the stack
            self.stack.push(prepared_call);
            return;
        } else {
            panic!("Expected function or thunk on stack for Thunk, got: {:?} (raw: 0x{:x})", func_val, func_val.raw);
        }
    }

    fn execute_ret_invoke(&mut self, frame_idx: usize) {
        // Tail-call elimination: reuse current frame instead of pushing a new one
        // Pop the prepared call from the stack
        let call = self.stack.pop().expect("Expected prepared call on stack");

        if !call.is_thunk() {
            panic!("RetInvoke expects Thunk value");
        }
        let (func_id, mut args) = call.as_thunk(&self.heap)
            .expect("RetInvoke expects Thunk value");

        // Get the required number of parameters for this function
        let required_params = if self.engine.functions.contains_key(&func_id) {
            // For native functions, use the arity
            Self::min_arity(&self.engine.functions.get(&func_id).unwrap().arity)
        } else if let Some(bytecode_func) = self.engine.bytecode_functions.get(&func_id) {
            bytecode_func.param_var_ids.len()
        } else {
            panic!("Function {} not found (neither native nor bytecode)", func_id);
        };

        // Check if we need more arguments and pop them from the stack
        let mut extra_args = Vec::new();
        let is_native = self.engine.functions.contains_key(&func_id);
        let filled_count = args.iter().filter(|opt| opt.is_some()).count();
        if !is_native {
            // Only bytecode functions support currying (extra arguments)
            while filled_count + extra_args.len() < required_params {
                if self.stack.is_empty() {
                    // Not enough arguments available, create a new Thunk (still partial)
                    // Fill holes with extra_args
                    let mut arg_iter = extra_args.into_iter();
                    for slot in args.iter_mut() {
                        if slot.is_none() {
                            if let Some(arg) = arg_iter.next() {
                                *slot = Some(arg);
                            }
                        }
                    }
                    // Add any remaining extra_args to the end
                    for arg in arg_iter {
                        args.push(Some(arg));
                    }
                    self.stack.push(Value::thunk_with_heap(func_id, args, &mut self.heap));
                    return;
                }
                // Pop an additional argument from the stack
                extra_args.push(self.stack.pop().unwrap());
            }
        }
        // Reverse extra_args to get correct order (stack is LIFO)
        extra_args.reverse();
        // Fill holes with extra_args
        let mut arg_iter = extra_args.into_iter();
        for slot in args.iter_mut() {
            if slot.is_none() {
                if let Some(arg) = arg_iter.next() {
                    *slot = Some(arg);
                }
            }
        }
        // Add any remaining extra_args to the end
        for arg in arg_iter {
            args.push(Some(arg));
        }

        // Ensure we have enough arguments before invoking
        let final_filled_count = args.iter().filter(|opt| opt.is_some()).count();
        if final_filled_count < required_params {
            // Still not enough args, create a new Thunk (still partial application)
            self.stack.push(Value::thunk_with_heap(func_id, args, &mut self.heap));
            return;
        }

        // Extract values from Option<Value> for function call
        let final_args: Vec<Value> = args.into_iter()
            .filter_map(|opt| opt)
            .collect();

        if is_native {
            // Native functions: call directly and push result
            let result = self.call_native_function(func_id, final_args);
            self.stack.push(result);
            // For native functions, we still need to return, so pop the frame
            self.call_stack.pop();
        } else {
            // Bytecode functions: reuse current frame
            let bytecode_func = self.engine.bytecode_functions.get(&func_id)
                .expect("Bytecode function should exist");
            
            if bytecode_func.code.is_empty() {
                panic!("Function {} has empty bytecode body", func_id);
            }
            
            // Initialize locals from arguments
            let locals = Self::init_locals_from_args(bytecode_func, &final_args);

            // Reuse the current frame: replace code, reset IP, replace locals
            let frame = &mut self.call_stack[frame_idx];
            frame.code = bytecode_func.code;
            frame.ip = 0;  // Jump to beginning of callee
            frame.locals = locals;
            // Execution continues in the VM loop with the reused frame
        }
    }

    fn execute_invoke(&mut self) {
        // Pop the prepared call from the stack
        // Note: When Thunk(n_args) is used, it already combines existing args from a thunk
        // with new args from the stack. So when we get here, the thunk should already have
        // all the args it needs. However, we still need to handle the case where extra args
        // might be on the stack for additional currying.
        let call = self.stack.pop().expect("Expected prepared call on stack");

        if !call.is_thunk() {
            panic!("Invoke expects Thunk value");
        }

        // Check if this is a composed thunk
        if let Some(ThunkData::Composed { first, second }) = call.as_thunk_ref(&self.heap) {
            // For composed thunks (g(f(x))):
            // 1. Invoke the first thunk (f) - it should be a fully applied thunk
            // 2. Invoke the second thunk (g) with the result of the first as argument
            // CRITICAL: Use invoke_thunk_value_recursive which handles composed thunks
            // correctly without stack truncation issues
            let first_val = *first;
            let second_val = *second;
            let first_result = self.invoke_thunk_value_recursive(first_val);
            // Now invoke the second thunk with first_result as argument
            // Use the same manual argument combination approach as invoke_thunk_value_recursive
            // Check for regular thunk with holes
            if let Some(ThunkData::Regular { func_id, bound }) = second_val.as_thunk_ref(&self.heap) {
                // Regular thunk: fill the first hole with first_result
                let mut filled = bound.clone();
                let mut filled_hole = false;
                for slot in filled.iter_mut() {
                    if slot.is_none() && !filled_hole {
                        *slot = Some(first_result);
                        filled_hole = true;
                        break;
                    }
                }
                if !filled_hole {
                    panic!("No holes available in thunk");
                }
                // Create a thunk and invoke it
                let thunk = Value::thunk_with_heap(*func_id, filled, &mut self.heap);
                let result = self.invoke_thunk_value_recursive(thunk);
                self.stack.push(result);
                return;
            } else if let Some((thunk_func_id, thunk_args)) = second_val.as_thunk(&self.heap) {
                // Regular thunk - fill first hole or add to end
                let mut final_args = thunk_args;
                let mut filled_hole = false;
                for slot in final_args.iter_mut() {
                    if slot.is_none() {
                        *slot = Some(first_result);
                        filled_hole = true;
                        break;
                    }
                }
                if !filled_hole {
                    final_args.push(Some(first_result));
                }
                let prepared = Value::thunk_with_heap(thunk_func_id, final_args, &mut self.heap);
                // Recursively invoke the prepared thunk
                let result = self.invoke_thunk_value_recursive(prepared);
                self.stack.push(result);
                return;
            } else if let Some(ThunkData::Composed { first: nested_first, second: nested_second }) = second_val.as_thunk_ref(&self.heap) {
                // Composed thunk - recursively apply first_result to nested_first
                let nested_first_val = *nested_first;
                let nested_second_val = *nested_second;
                // Recursively apply first_result to nested_first
                let prepared_nested = if let Some(ThunkData::Regular { func_id, bound }) = nested_first_val.as_thunk_ref(&self.heap) {
                    // Nested first is a regular thunk: fill the first hole with first_result
                    let mut filled = bound.clone();
                    let mut filled_hole = false;
                    for slot in filled.iter_mut() {
                        if slot.is_none() && !filled_hole {
                            *slot = Some(first_result);
                            filled_hole = true;
                            break;
                        }
                    }
                    if !filled_hole {
                        panic!("No holes available in nested thunk");
                    }
                    Value::thunk_with_heap(*func_id, filled, &mut self.heap)
                } else if let Some((nested_func_id, nested_args)) = nested_first_val.as_thunk(&self.heap) {
                    let mut nested_final_args = nested_args;
                    let mut filled_hole = false;
                    for slot in nested_final_args.iter_mut() {
                        if slot.is_none() {
                            *slot = Some(first_result);
                            filled_hole = true;
                            break;
                        }
                    }
                    if !filled_hole {
                        nested_final_args.push(Some(first_result));
                    }
                    Value::thunk_with_heap(nested_func_id, nested_final_args, &mut self.heap)
                } else if let Some(nested_func_id) = nested_first_val.as_function() {
                    let arity = if let Some(native_func) = self.engine.functions.get(&nested_func_id) {
                        Self::min_arity(&native_func.arity)
                    } else if let Some(bytecode_func) = self.engine.bytecode_functions.get(&nested_func_id) {
                        bytecode_func.param_var_ids.len()
                    } else {
                        1
                    };
                    let mut bound = vec![Some(first_result)];
                    while bound.len() < arity {
                        bound.push(None);
                    }
                    Value::thunk_with_heap(nested_func_id, bound, &mut self.heap)
                } else {
                    panic!("nested_first_val must be a thunk or function");
                };
                let recomposed = Value::composed_thunk_with_heap(prepared_nested, nested_second_val, &mut self.heap);
                let result = self.invoke_thunk_value_recursive(recomposed);
                self.stack.push(result);
                return;
            } else if let Some(func_id) = second_val.as_function() {
                // It's a function - create thunk with first_result as arg
                let arity = if let Some(native_func) = self.engine.functions.get(&func_id) {
                    Self::min_arity(&native_func.arity)
                } else if let Some(bytecode_func) = self.engine.bytecode_functions.get(&func_id) {
                    bytecode_func.param_var_ids.len()
                } else {
                    1
                };
                let mut bound = vec![Some(first_result)];
                while bound.len() < arity {
                    bound.push(None);
                }
                let prepared = Value::thunk_with_heap(func_id, bound, &mut self.heap);
                let result = self.invoke_thunk_value_recursive(prepared);
                self.stack.push(result);
                return;
            } else {
                panic!("Second part of composed thunk must be a function or thunk");
            }
        }

        // Handle regular thunk (not composed)
        let (func_id, mut args) = call.as_thunk(&self.heap)
            .expect("Invoke expects regular Thunk value");

        // Get the required number of parameters for this function
        let required_params = if self.engine.functions.contains_key(&func_id) {
            // For native functions, use the arity
            Self::min_arity(&self.engine.functions.get(&func_id).unwrap().arity)
        } else if let Some(bytecode_func) = self.engine.bytecode_functions.get(&func_id) {
            bytecode_func.param_var_ids.len()
        } else {
            panic!("Function {} not found (neither native nor bytecode)", func_id);
        };

        // Check if we need more arguments and pop them from the stack
        // This handles additional currying beyond what Thunk(n_args) already handled.
        let mut extra_args = Vec::new();
        let is_native = self.engine.functions.contains_key(&func_id);
        let filled_count = args.iter().filter(|opt| opt.is_some()).count();
        if !is_native {
            // Only bytecode functions support currying (extra arguments)
            while filled_count + extra_args.len() < required_params {
                if self.stack.is_empty() {
                    // Not enough arguments available, create a new Thunk (still partial)
                    // Fill holes with extra_args
                    let mut arg_iter = extra_args.into_iter();
                    for slot in args.iter_mut() {
                        if slot.is_none() {
                            if let Some(arg) = arg_iter.next() {
                                *slot = Some(arg);
                            }
                        }
                    }
                    // Add any remaining extra_args to the end
                    for arg in arg_iter {
                        args.push(Some(arg));
                    }
                    self.stack.push(Value::thunk_with_heap(func_id, args, &mut self.heap));
                    return;
                }
                // Pop an additional argument from the stack
                extra_args.push(self.stack.pop().unwrap());
            }
        }
        // Reverse extra_args to get correct order (stack is LIFO)
        extra_args.reverse();
        // Fill holes with extra_args
        let mut arg_iter = extra_args.into_iter();
        for slot in args.iter_mut() {
            if slot.is_none() {
                if let Some(arg) = arg_iter.next() {
                    *slot = Some(arg);
                }
            }
        }
        // Add any remaining extra_args to the end
        for arg in arg_iter {
            args.push(Some(arg));
        }

        // Ensure we have enough arguments before invoking
        let final_filled_count = args.iter().filter(|opt| opt.is_some()).count();
        if final_filled_count < required_params {
            // Still not enough args, create a new Thunk (still partial application)
            self.stack.push(Value::thunk_with_heap(func_id, args, &mut self.heap));
            return;
        }

        // Extract values from Option<Value> for function call
        let final_args: Vec<Value> = args.into_iter()
            .filter_map(|opt| opt)
            .collect();

        // TRAMPOLINE: Instead of calling invoke_thunk_sync recursively, push a frame and let the VM loop handle it
        if is_native {
            // Native functions: call directly (no recursion, just a simple function call)
            let result = self.call_native_function(func_id, final_args);
            self.stack.push(result);
        } else {
            // Bytecode functions: push frame and let VM loop handle execution (trampoline)
            let bytecode_func = self.engine.bytecode_functions.get(&func_id)
                .expect("Bytecode function should exist");
            self.create_bytecode_frame(bytecode_func, final_args);
            // Execution continues in the VM loop - no recursion!
        }
    }

    #[allow(dead_code)]
    fn invoke_function(&mut self, func_id: u32, args: Vec<Value>) {
        // Check if it's a native function or bytecode function
        // IMPORTANT: Check native functions first, but bytecode functions should be the fallback
        if self.engine.functions.contains_key(&func_id) {
            // Native function: convert args to strings and call
            let result = self.call_native_function(func_id, args);
            self.stack.push(result);
        } else if let Some(bytecode_func) = self.engine.bytecode_functions.get(&func_id) {
            // Bytecode function: push new call frame
            // Safety check: ensure we have enough arguments
            let required_params = bytecode_func.param_var_ids.len();
            if args.len() < required_params {
                panic!("Attempted to invoke function {} with {} args but it requires {}", 
                    func_id, args.len(), required_params);
            }
            self.create_bytecode_frame(bytecode_func, args);
            // Execution will continue in the main loop with the new frame
        } else {
            panic!("Function {} not found (neither native nor bytecode)", func_id);
        }
    }
}
