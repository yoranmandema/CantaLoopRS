use crate::{bytecode::opcode::{OpCode, OPCODE_COUNT}, engine::Engine};

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
static DISPATCH: [OpHandler; OPCODE_COUNT] = [
    VM::op_ld_num,        // 0
    VM::op_ld_str,        // 1
    VM::op_ld_var,        // 2
    VM::op_ld_const,      // 3
    VM::op_ld_func,       // 4
    VM::op_add,           // 5
    VM::op_sub,           // 6
    VM::op_mul,           // 7
    VM::op_div,           // 8
    VM::op_pow,           // 9
    VM::op_eq,            // 10
    VM::op_ne,            // 11
    VM::op_gt,            // 12
    VM::op_lt,            // 13
    VM::op_ge,            // 14
    VM::op_le,            // 15
    VM::op_and,           // 16
    VM::op_or,            // 17
    VM::op_neg,           // 18
    VM::op_not,           // 19
    VM::op_st_var,        // 20
    VM::op_print,         // 21
    VM::op_call_stack,    // 22
    VM::op_thunk,         // 23
    VM::op_invoke,        // 24
    VM::op_ret,           // 25
    VM::op_jmp_if_false,  // 26
    VM::op_jmp,           // 27
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

// Heap storage for VM - managed per VM instance
pub struct ValueHeap {
    pub(crate) strings: Vec<String>,
    pub(crate) thunks: Vec<ThunkData>,
}

pub(crate) struct ThunkData {
    func_id: u32,
    args: Vec<Value>,
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
    pub fn thunk_with_heap(func_id: u32, args: Vec<Value>, heap: &mut ValueHeap) -> Self {
        let idx = heap.thunks.len();
        heap.thunks.push(ThunkData { func_id, args });
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
    pub fn as_thunk(&self, heap: &ValueHeap) -> Option<(u32, Vec<Value>)> {
        if self.tag() == TAG_THUNK {
            let idx = (self.raw & PAYLOAD_MASK) as usize;
            heap.thunks.get(idx).map(|t| (t.func_id, t.args.clone()))
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
        } else if self.as_thunk(heap).is_some() {
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
        });

        // Main execution loop - process the current frame
        while !self.call_stack.is_empty() {
            let frame_idx = self.call_stack.len() - 1;
            
            // Check if frame is finished
            {
                let frame = &self.call_stack[frame_idx];
                if frame.ip >= frame.code.len() {
                    self.call_stack.pop();
                    continue;
                }
            }

            // Execute the opcode using dispatch table
            match self.step(frame_idx) {
                StepResult::Continue => continue,  // Ret was executed
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
            let val = if idx < frame.locals.len() {
                frame.locals[idx]
            } else {
                Value::none()
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
    fn op_div(_vm: &mut VM, _frame_idx: usize, _opcode: &OpCode) -> StepResult {
        _vm.binary_div();
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
        let v = _vm.stack.pop().expect("Stack underflow");
        if let Some(n) = v.as_number() {
            _vm.stack.push(Value::number(-n));
        } else {
            panic!("Negate non-number");
        }
        StepResult::Normal
    }

    #[inline(always)]
    fn op_not(_vm: &mut VM, _frame_idx: usize, _opcode: &OpCode) -> StepResult {
        let v = _vm.stack.pop().expect("Stack underflow");
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
    fn op_jmp(_vm: &mut VM, frame_idx: usize, opcode: &OpCode) -> StepResult {
        if let OpCode::Jmp(offset) = opcode {
            _vm.call_stack[frame_idx].ip = *offset;
        }
        StepResult::Normal
    }

    fn binary_add(&mut self) {
        let b = self.stack.pop().expect("Stack underflow");
        let a = self.stack.pop().expect("Stack underflow");
        if let (Some(a_num), Some(b_num)) = (a.as_number(), b.as_number()) {
            self.stack.push(Value::number(a_num + b_num));
        } else {
            // If either operand is a string or any other type, convert both to strings and concatenate
            let mut result = a.value_to_string(&self.heap);
            result.push_str(&b.value_to_string(&self.heap));
            self.stack.push(Value::string_with_heap(result, &mut self.heap));
        }
    }

    fn binary_sub(&mut self) {
        let b = self.stack.pop().expect("Stack underflow");
        let a = self.stack.pop().expect("Stack underflow");
        if let (Some(a), Some(b)) = (a.as_number(), b.as_number()) {
            self.stack.push(Value::number(a - b));
        } else {
            panic!("Subtract operation requires both operands to be numbers");
        }
    }

    fn binary_mul(&mut self) {
        let b = self.stack.pop().expect("Stack underflow");
        let a = self.stack.pop().expect("Stack underflow");
        if let (Some(a), Some(b)) = (a.as_number(), b.as_number()) {
            self.stack.push(Value::number(a * b));
        } else {
            panic!("Multiply operation requires both operands to be numbers");
        }
    }

    fn binary_div(&mut self) {
        let b = self.stack.pop().expect("Stack underflow");
        let a = self.stack.pop().expect("Stack underflow");
        if let (Some(a), Some(b)) = (a.as_number(), b.as_number()) {
            self.stack.push(Value::number(a / b));
        } else {
            panic!("Divide operation requires both operands to be numbers");
        }
    }

    fn binary_pow(&mut self) {
        let b = self.stack.pop().expect("Stack underflow");
        let a = self.stack.pop().expect("Stack underflow");
        if let (Some(a), Some(b)) = (a.as_number(), b.as_number()) {
            self.stack.push(Value::number(a.powf(b)));
        } else {
            panic!("Power operation requires both operands to be numbers");
        }
    }

    fn comparison_eq(&mut self) {
        let b = self.stack.pop().expect("Stack underflow");
        let a = self.stack.pop().expect("Stack underflow");
        let result = if let (Some(a), Some(b)) = (a.as_number(), b.as_number()) {
            a == b
        } else if let (Some(a), Some(b)) = (a.as_string(&self.heap), b.as_string(&self.heap)) {
            a == b
        } else if let (Some(a), Some(b)) = (a.as_boolean(), b.as_boolean()) {
            a == b
        } else {
            panic!("Comparison == on incompatible types");
        };
        self.stack.push(Value::boolean(result));
    }

    fn comparison_ne(&mut self) {
        let b = self.stack.pop().expect("Stack underflow");
        let a = self.stack.pop().expect("Stack underflow");
        let result = if let (Some(a), Some(b)) = (a.as_number(), b.as_number()) {
            a != b
        } else if let (Some(a), Some(b)) = (a.as_string(&self.heap), b.as_string(&self.heap)) {
            a != b
        } else if let (Some(a), Some(b)) = (a.as_boolean(), b.as_boolean()) {
            a != b
        } else {
            panic!("Comparison != on incompatible types");
        };
        self.stack.push(Value::boolean(result));
    }

    fn comparison_gt(&mut self) {
        let b = self.stack.pop().expect("Stack underflow");
        let a = self.stack.pop().expect("Stack underflow");
        let result = if let (Some(a), Some(b)) = (a.as_number(), b.as_number()) {
            a > b
        } else if let (Some(a), Some(b)) = (a.as_string(&self.heap), b.as_string(&self.heap)) {
            a > b
        } else if let (Some(a), Some(b)) = (a.as_boolean(), b.as_boolean()) {
            let a_num = if a { 1.0 } else { 0.0 };
            let b_num = if b { 1.0 } else { 0.0 };
            a_num > b_num
        } else {
            panic!("Comparison > on incompatible types");
        };
        self.stack.push(Value::boolean(result));
    }

    fn comparison_lt(&mut self) {
        let b = self.stack.pop().expect("Stack underflow");
        let a = self.stack.pop().expect("Stack underflow");
        let result = if let (Some(a), Some(b)) = (a.as_number(), b.as_number()) {
            a < b
        } else if let (Some(a), Some(b)) = (a.as_string(&self.heap), b.as_string(&self.heap)) {
            a < b
        } else if let (Some(a), Some(b)) = (a.as_boolean(), b.as_boolean()) {
            let a_num = if a { 1.0 } else { 0.0 };
            let b_num = if b { 1.0 } else { 0.0 };
            a_num < b_num
        } else {
            panic!("Comparison < on incompatible types");
        };
        self.stack.push(Value::boolean(result));
    }

    fn comparison_ge(&mut self) {
        let b = self.stack.pop().expect("Stack underflow");
        let a = self.stack.pop().expect("Stack underflow");
        let result = if let (Some(a), Some(b)) = (a.as_number(), b.as_number()) {
            a >= b
        } else if let (Some(a), Some(b)) = (a.as_string(&self.heap), b.as_string(&self.heap)) {
            a >= b
        } else if let (Some(a), Some(b)) = (a.as_boolean(), b.as_boolean()) {
            let a_num = if a { 1.0 } else { 0.0 };
            let b_num = if b { 1.0 } else { 0.0 };
            a_num >= b_num
        } else {
            panic!("Comparison >= on incompatible types");
        };
        self.stack.push(Value::boolean(result));
    }

    fn comparison_le(&mut self) {
        let b = self.stack.pop().expect("Stack underflow");
        let a = self.stack.pop().expect("Stack underflow");
        let result = if let (Some(a), Some(b)) = (a.as_number(), b.as_number()) {
            a <= b
        } else if let (Some(a), Some(b)) = (a.as_string(&self.heap), b.as_string(&self.heap)) {
            a <= b
        } else if let (Some(a), Some(b)) = (a.as_boolean(), b.as_boolean()) {
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
        let b = self.stack.pop().expect("Stack underflow");
        let a = self.stack.pop().expect("Stack underflow");
        let result = Self::to_bool(&a) && Self::to_bool(&b);
        self.stack.push(Value::boolean(result));
    }

    fn logical_or(&mut self) {
        let b = self.stack.pop().expect("Stack underflow");
        let a = self.stack.pop().expect("Stack underflow");
        let result = Self::to_bool(&a) || Self::to_bool(&b);
        self.stack.push(Value::boolean(result));
    }

    fn execute_call_stack(&mut self, n_args: u32) {
        let n_args = n_args as usize;
        // Pop function reference
        let func_val = self.stack.pop().expect("Stack underflow");
        let func_id = func_val.as_function().expect("Expected function on stack");

        // Pop arguments
        let args: Vec<Value> = pop_n(&mut self.stack, n_args);

        // Check if it's a native function or bytecode function
        if let Some(native_func) = self.engine.functions.get(&func_id) {
            // Native function: convert args to strings and call
            let args_str: Vec<String> = args
                .into_iter()
                .map(|v| v.value_to_string(&self.heap))
                .collect();
            let result = native_func(&args_str);
            self.stack.push(Value::string_with_heap(result, &mut self.heap));
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
            let mut locals = vec![Value::none(); (max_var_id + 1) as usize];
            for (i, param_var_id) in bytecode_func.param_var_ids.iter().enumerate() {
                if i < args.len() {
                    locals[*param_var_id as usize] = args[i];
                }
            }

            // Push new frame with function bytecode - use static reference, no cloning
            self.call_stack.push(CallFrame {
                code: bytecode_func.code,  // Already &'static [OpCode], no clone needed
                ip: 0,  // Start at beginning of function
                locals: locals.into_boxed_slice(),  // Convert Vec to Box<[Value]>
            });
            // Execution will continue in the main loop with the new frame
        } else {
            panic!("Function {} not found (neither native nor bytecode)", func_id);
        }
    }

    fn execute_return(&mut self) {
        // Pop return value (or use None if stack is empty)
        let return_value = self.stack.pop().unwrap_or(Value::none());

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
        let func_id = func_val.as_function().unwrap_or_else(|| {
            panic!("Expected function on stack for Thunk, got: {:?} (raw: 0x{:x})", func_val, func_val.raw);
        });

        // Pop arguments (they're on the stack in order, so we need to reverse them)
        let args: Vec<Value> = pop_n(&mut self.stack, n_args);

        // Create a Thunk value
        let prepared_call = Value::thunk_with_heap(func_id, args, &mut self.heap);

        // Push the prepared call onto the stack
        self.stack.push(prepared_call);
    }

    fn execute_invoke(&mut self) {
        // Pop the prepared call from the stack
        // Note: For currying (e.g., add5!(10)), extra arguments may be on the stack
        // before the Thunk. We need to check if we need more args and pop them.
        let call = self.stack.pop().expect("Expected prepared call on stack");

        let (func_id, mut args) = call.as_thunk(&self.heap)
            .expect("Invoke expects Thunk value");

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
                    self.stack.push(Value::thunk_with_heap(func_id, args, &mut self.heap));
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
            self.stack.push(Value::thunk_with_heap(func_id, args, &mut self.heap));
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
                .map(|v| v.value_to_string(&self.heap))
                .collect();
            let result = native_func(&args_str);
            self.stack.push(Value::string_with_heap(result, &mut self.heap));
        } else if let Some(bytecode_func) = self.engine.bytecode_functions.get(&func_id) {
            // Bytecode function: push new call frame
            
            // Determine the maximum var_id we'll need for locals
            let max_var_id = bytecode_func.param_var_ids.iter()
                .max()
                .copied()
                .unwrap_or(0);
            
            // Initialize locals vector with arguments bound to parameter slots
            let mut locals = vec![Value::none(); (max_var_id + 1) as usize];
            for (i, param_var_id) in bytecode_func.param_var_ids.iter().enumerate() {
                if i < args.len() {
                    locals[*param_var_id as usize] = args[i];
                }
            }

            // Push new frame with function bytecode - use static reference, no cloning
            self.call_stack.push(CallFrame {
                code: bytecode_func.code,  // Already &'static [OpCode], no clone needed
                ip: 0,  // Start at beginning of function
                locals: locals.into_boxed_slice(),  // Convert Vec to Box<[Value]>
            });
            // Execution will continue in the main loop with the new frame
        } else {
            panic!("Function {} not found (neither native nor bytecode)", func_id);
        }
    }
}
