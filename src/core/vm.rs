use std::collections::HashMap;

/// Compute a stable type ID from a struct name using a simple hash function.
/// This avoids collisions that would occur with just using the string length.
/// Uses a djb2-style hash algorithm for good distribution.
pub fn compute_struct_type_id(struct_name: &str) -> u32 {
    let mut hash: u32 = 5381;
    for byte in struct_name.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(byte as u32);
    }
    hash
}

use crate::core::bytecode::{OpCode, OPCODE_COUNT};
use crate::core::engine::{Arity, BytecodeFunction, Engine};
use crate::core::hir_lowering::HirAst;

/// Special function ID for composition.
/// When a thunk has this func_id, it represents a composition of two functions.
const COMPOSE_ID: u32 = 0xFFFF_FFFF;

// Function pointer type for opcode handlers
// Note: Handlers take frame_idx to access frame through VM, avoiding borrow conflicts
type OpHandler = fn(&mut VM, usize, &OpCode) -> StepResult;

// Result of executing a step - indicates control flow behavior
#[derive(Debug, Clone, Copy)]
enum StepResult {
    Normal,   // Normal execution, IP was incremented
    Continue, // Special case (e.g., Ret), needs to restart loop
}

// Dispatch table for monomorphic, branch-predictable opcode execution
// CRITICAL: Array indices MUST match opcode discriminant values from bytecode_opcode.rs
static DISPATCH: [OpHandler; OPCODE_COUNT] = [
    VM::op_ld_num,        // 0: LdNum
    VM::op_ld_str,        // 1: LdStr
    VM::op_ld_bool,       // 2: LdBool
    VM::op_ld_var,        // 3: LdVar
    VM::op_ld_const,      // 4: LdConst
    VM::op_ld_func,       // 5: LdFunc
    VM::op_add,           // 6: Add
    VM::op_sub,           // 7: Sub
    VM::op_mul,           // 8: Mul
    VM::op_div,           // 9: Div
    VM::op_mod,           // 10: Mod
    VM::op_pow,           // 11: Pow
    VM::op_add_num,       // 12: AddNum
    VM::op_mul_num,       // 13: MulNum
    VM::op_sub_num,       // 14: SubNum
    VM::op_eq,            // 15: Eq
    VM::op_ne,            // 16: Ne
    VM::op_gt,            // 17: Gt
    VM::op_lt,            // 18: Lt
    VM::op_ge,            // 19: Ge
    VM::op_le,            // 20: Le
    VM::op_and,           // 21: And
    VM::op_or,            // 22: Or
    VM::op_neg,           // 23: Neg
    VM::op_not,           // 24: Not
    VM::op_st_var,        // 25: StVar
    VM::op_pop,           // 26: Pop
    VM::op_print,         // 27: Print
    VM::op_call_stack,    // 28: CallStack
    VM::op_thunk,         // 29: Thunk
    VM::op_make_partial,  // 30: MakePartial
    VM::op_compose_thunk, // 31: ComposeThunk
    VM::op_invoke,        // 32: Invoke
    VM::op_ret,           // 33: Ret
    VM::op_ret_invoke,    // 34: RetInvoke
    VM::op_jmp_if_false,  // 35: JmpIfFalse
    VM::op_jmp_if_true,   // 36: JmpIfTrue
    VM::op_jmp,           // 37: Jmp
    VM::op_make_array,    // 38: MakeArray
    VM::op_array_iter,    // 39: ArrayIter
    VM::op_array_next,    // 40: ArrayNext
    VM::op_index,   // 41: ArrayIndex
    VM::op_array_slice,   // 42: ArraySlice
    VM::op_make_struct,   // 43: MakeStruct
    VM::op_get_field,     // 44: GetField
    VM::op_map,           // 45: Map
    VM::op_filter,        // 46: Filter
    VM::op_fold,          // 47: Fold
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
const PAYLOAD_MASK: u64 = 0x0000_FFFF_FFFF_FFFF; // Bits 0-47 for payload (exclude tag bits 48-51)
const QNAN_BASE: u64 = 0x7FF8_0000_0000_0000; // Quiet NaN base (exponent=0x7FF, bit 51 set)
const TAG_MASK: u64 = 0xF << 48; // Bits 48-51 for tag
const TAG_CLEAR_MASK: u64 = !TAG_MASK; // Mask to clear tag bits
const QNAN_BIT_51: u64 = 1 << 51; // Bit 51 must be set for quiet NaN

const TAG_STRING: u64 = 0x1;
const TAG_BOOLEAN: u64 = 0x2;
const TAG_FUNCTION: u64 = 0x3;
const TAG_THUNK: u64 = 0x4;
const TAG_NONE: u64 = 0x5;
const TAG_ARRAY: u64 = 0x6;
const TAG_ARRAY_ITER: u64 = 0x7;
const TAG_STRUCT: u64 = 0x8;

/// Represents something that can be called (function, thunk, or closure).
///
/// This is a handle that native code can store and use to request execution.
/// Native code never executes callables directly - it requests execution via Invokable.
#[derive(Clone, Copy)]
pub struct Callable {
    value: Value,
}

/// Represents an execution request from native code to the VM.
///
/// Native code creates Invokable requests to ask the VM to execute a callable.
/// The VM processes these requests and performs the actual execution.
pub struct Invokable {
    callable: Value,
    args: Vec<Value>,
}

/// Heap storage for VM-managed data structures.
///
/// Stores strings, thunks, arrays, structs, and iterators that cannot fit in the 64-bit Value representation.
/// Managed per VM instance to avoid global state.
pub struct ValueHeap {
    pub(crate) strings: Vec<String>,
    pub(crate) thunks: Vec<ThunkData>,
    pub(crate) arrays: Vec<Vec<Value>>,
    pub(crate) array_iters: Vec<ArrayIterator>,
    pub(crate) structs: Vec<StructData>,
    pub(crate) type_registry: Option<std::collections::HashMap<u32, (String, Vec<String>)>>, // Maps type_id -> (struct_name, field_names)
    pub(crate) engine: Option<std::sync::Arc<crate::core::engine::Engine>>, // Engine reference for stdlib functions to invoke thunks
    pub(crate) bytecode_functions: std::collections::HashMap<u32, crate::core::engine::BytecodeFunction>, // Bytecode functions for invoking from stdlib
    pub(crate) execution_requests: Vec<Invokable>, // Execution requests from native code
}

/// Array iterator state
pub(crate) struct ArrayIterator {
    pub array_idx: usize,
    pub current_idx: usize,
}

/// Struct instance data stored in the heap
pub struct StructData {
    pub type_id: u32, // Struct type ID (index into struct definitions)
    pub fields: Vec<Value>, // Field values in order
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

impl Callable {
    /// Create a Callable from a Value.
    /// The Value must be a function, thunk, or closure.
    pub fn from_value(value: Value, heap: &ValueHeap) -> Result<Self, String> {
        if value.as_function().is_some() || value.is_thunk() {
            Ok(Callable { value })
        } else {
            Err(format!("Value is not callable: {:?}", value))
        }
    }

    /// Get the underlying Value.
    pub fn value(&self) -> Value {
        self.value
    }

    /// Check if this callable is a function.
    pub fn is_function(&self) -> bool {
        self.value.as_function().is_some()
    }

    /// Check if this callable is a thunk.
    pub fn is_thunk(&self) -> bool {
        self.value.is_thunk()
    }
}

impl Invokable {
    /// Create a new execution request.
    pub fn new(callable: Value, args: Vec<Value>) -> Self {
        Invokable { callable, args }
    }

    /// Get the callable to execute.
    pub fn callable(&self) -> Value {
        self.callable
    }

    /// Get the arguments to pass to the callable.
    pub fn args(&self) -> &[Value] {
        &self.args
    }
}

impl ValueHeap {
    fn new() -> Self {
        Self {
            strings: Vec::new(),
            thunks: Vec::new(),
            arrays: Vec::new(),
            array_iters: Vec::new(),
            structs: Vec::new(),
            type_registry: None,
            engine: None,
            bytecode_functions: std::collections::HashMap::new(),
            execution_requests: Vec::new(),
        }
    }

    /// Extract a Callable from a Value argument in native functions.
    /// This is a convenience function for native code that receives callable arguments.
    /// 
    /// # Example
    /// ```ignore
    /// // In a native function:
    /// let callback = heap.as_callable(&args[0])?;
    /// heap.request_execution(callback.value(), vec![Value::number(42.0)]);
    /// ```
    pub fn as_callable(&self, value: &Value) -> Result<Callable, String> {
        Callable::from_value(*value, self)
    }

    /// Request execution of a callable with the given arguments.
    /// 
    /// This is the primary way for native code to request execution of callables.
    /// The VM will process these requests after the native function returns.
    /// Native code never executes callables directly - it requests execution via this method.
    /// 
    /// # Example
    /// ```ignore
    /// // In a native function:
    /// let callback = heap.as_callable(&args[0])?;
    /// heap.request_execution(callback.value(), vec![Value::number(42.0)]);
    /// ```
    pub fn request_execution(&mut self, callable: Value, args: Vec<Value>) {
        self.execution_requests.push(Invokable::new(callable, args));
    }

    /// Take all pending execution requests.
    /// The VM calls this to process requests from native code.
    pub(crate) fn take_execution_requests(&mut self) -> Vec<Invokable> {
        std::mem::take(&mut self.execution_requests)
    }

    fn set_type_registry(&mut self, registry: std::collections::HashMap<u32, (String, Vec<String>)>) {
        self.type_registry = Some(registry);
    }

    pub(crate) fn set_engine(&mut self, engine: std::sync::Arc<crate::core::engine::Engine>) {
        self.engine = Some(engine);
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
            raw: (QNAN_BASE & TAG_CLEAR_MASK)
                | (TAG_BOOLEAN << 48)
                | QNAN_BIT_51
                | (if b { 1 } else { 0 }),
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
    pub fn array_with_heap(elements: Vec<Value>, heap: &mut ValueHeap) -> Self {
        let idx = heap.arrays.len();
        heap.arrays.push(elements);
        Self {
            raw: (QNAN_BASE & TAG_CLEAR_MASK) | (TAG_ARRAY << 48) | QNAN_BIT_51 | (idx as u64),
        }
    }

    #[inline(always)]
    pub fn array_iter_with_heap(array_idx: usize, heap: &mut ValueHeap) -> Self {
        let idx = heap.array_iters.len();
        heap.array_iters.push(ArrayIterator {
            array_idx,
            current_idx: 0,
        });
        Self {
            raw: (QNAN_BASE & TAG_CLEAR_MASK) | (TAG_ARRAY_ITER << 48) | QNAN_BIT_51 | (idx as u64),
        }
    }

    #[inline(always)]
    pub fn struct_with_heap(type_id: u32, fields: Vec<Value>, heap: &mut ValueHeap) -> Self {
        let idx = heap.structs.len();
        heap.structs.push(StructData { type_id, fields });
        Self {
            raw: (QNAN_BASE & TAG_CLEAR_MASK) | (TAG_STRUCT << 48) | QNAN_BIT_51 | (idx as u64),
        }
    }

    #[inline(always)]
    pub fn as_array<'a>(&self, heap: &'a ValueHeap) -> Option<&'a Vec<Value>> {
        if self.tag() == TAG_ARRAY {
            let idx = (self.raw & PAYLOAD_MASK) as usize;
            heap.arrays.get(idx)
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn as_array_mut<'a>(&self, heap: &'a mut ValueHeap) -> Option<&'a mut Vec<Value>> {
        if self.tag() == TAG_ARRAY {
            let idx = (self.raw & PAYLOAD_MASK) as usize;
            heap.arrays.get_mut(idx)
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn as_array_iter_mut<'a>(&self, heap: &'a mut ValueHeap) -> Option<&'a mut ArrayIterator> {
        if self.tag() == TAG_ARRAY_ITER {
            let idx = (self.raw & PAYLOAD_MASK) as usize;
            heap.array_iters.get_mut(idx)
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn as_struct<'a>(&self, heap: &'a ValueHeap) -> Option<&'a StructData> {
        if self.tag() == TAG_STRUCT {
            let idx = (self.raw & PAYLOAD_MASK) as usize;
            heap.structs.get(idx)
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn is_struct(&self) -> bool {
        self.tag() == TAG_STRUCT
    }

    #[inline(always)]
    fn tag(&self) -> u64 {
        // Check if it's a NaN (exponent bits 0x7FF)
        if (self.raw & 0x7FF0_0000_0000_0000) == 0x7FF0_0000_0000_0000 {
            // Extract tag from bits 48-51
            // Bit 51 is always set for quiet NaN (0x8 in the 4-bit value)
            // Tags 0-7 use bits 48-50 (3 bits), with bit 51 set
            // Tag 8 (STRUCT) uses all 4 bits: 0x8 (bit 51 set, bits 48-50 = 0)
            // But wait - if bit 51 is always set, then 0x8 means bit 51 set + bits 48-50 = 0
            // So we need to distinguish: if bits 48-51 == 0x8, it's TAG_STRUCT (8)
            // Otherwise, extract bits 48-50 for tags 0-7
            let tag_bits = (self.raw >> 48) & 0xF; // Extract bits 48-51 (4 bits)
            if tag_bits == 0x8 {
                8 // TAG_STRUCT: bit 51 set, bits 48-50 = 0
            } else {
                tag_bits & 0x7 // Tags 0-7: extract bits 48-50 (bit 51 is set but not part of tag value)
            }
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

    /// Invoke a thunk with additional arguments.
    /// This is useful for stdlib functions that need to invoke thunks.
    /// Only works for native functions (not bytecode functions).
    /// Requires the Engine to be stored in the ValueHeap (via set_engine).
    pub fn invoke_thunk(self, args: &[Value], heap: &mut ValueHeap) -> Value {
        if !self.is_thunk() {
            panic!("invoke_thunk called on non-thunk value");
        }

        // Extract thunk data first (before borrowing engine)
        let (func_id, mut bound) = if let Some((id, b)) = self.as_thunk(heap) {
            (id, b)
        } else {
            panic!("Cannot invoke composed thunk from stdlib - requires VM");
        };

        // Fill holes with provided arguments
        let mut arg_iter = args.iter();
        for slot in bound.iter_mut() {
            if slot.is_none() {
                if let Some(arg) = arg_iter.next() {
                    *slot = Some(*arg);
                }
            }
        }

        // Check if all holes are filled
        if bound.iter().any(|s| s.is_none()) {
            panic!("Thunk still has holes after applying arguments");
        }

        // Extract final arguments
        let final_args: Vec<Value> = bound.into_iter().filter_map(|opt| opt).collect();

        // Get Engine from heap (clone Arc to avoid borrow conflicts)
        let engine_arc = heap.engine.as_ref().expect("Engine must be set in ValueHeap to invoke thunks").clone();
        
        // Check if it's a native function (using cloned Arc, so we can borrow heap mutably below)
        let func_id_copy = func_id; // Copy func_id to avoid borrowing engine_arc in closure
        if let Some(native_func) = engine_arc.functions.get(&func_id_copy) {
            // Call the native function (now we can borrow heap mutably since engine_arc is owned)
            (native_func.func)(final_args, heap)
        } else if let Some(_bytecode_func) = heap.bytecode_functions.get(&func_id_copy) {
            // It's a bytecode function - we can't execute it from stdlib without the VM
            // This is a limitation: bytecode functions need the VM to execute
            // For now, return an error - this needs VM execution which isn't available from stdlib
            panic!("Cannot invoke bytecode function from stdlib - function {} requires VM. Closures in map/filter need VM execution.", func_id);
        } else {
            panic!("Function {} not found (neither native nor bytecode)", func_id);
        }
    }

    pub fn value_to_string(self, heap: &ValueHeap) -> String {
        self.value_to_string_with_hir(heap, None)
    }

    pub fn value_to_string_with_hir(self, heap: &ValueHeap, hir: Option<&crate::core::hir_lowering::HirAst>) -> String {
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
        } else if let Some(arr) = self.as_array(heap) {
            let elements: Vec<String> = arr.iter().map(|v| v.value_to_string_with_hir(heap, hir)).collect();
            format!("[{}]", elements.join(","))
        } else if let Some(struct_data) = self.as_struct(heap) {
            // Try to find the struct definition from HIR or type registry
            let (struct_name, field_names_opt) = if let Some(hir) = hir {
                // Find struct by matching type_id (computed from struct_name hash)
                if let Some((struct_name, struct_def)) = hir.structs.iter()
                    .find(|(name, _)| compute_struct_type_id(name) == struct_data.type_id) {
                    let field_names: Vec<String> = struct_def.fields.iter().map(|(name, _)| name.clone()).collect();
                    (struct_name.clone(), Some(field_names))
                } else {
                    // Try type registry as fallback
                    let (name, field_names) = heap.type_registry.as_ref()
                        .and_then(|reg| reg.get(&struct_data.type_id))
                        .cloned()
                        .unwrap_or_else(|| (format!("Struct_{}", struct_data.type_id), vec![]));
                    (name, if !field_names.is_empty() { Some(field_names) } else { None })
                }
            } else {
                // No HIR - try type registry
                let (name, field_names) = heap.type_registry.as_ref()
                    .and_then(|reg| reg.get(&struct_data.type_id))
                    .cloned()
                    .unwrap_or_else(|| (format!("Struct_{}", struct_data.type_id), vec![]));
                (name, if !field_names.is_empty() { Some(field_names) } else { None })
            };
            
            // Format field values
            let field_strings: Vec<String> = if let Some(field_names) = field_names_opt {
                // We have field names - use them
                field_names.iter()
                    .zip(struct_data.fields.iter())
                    .map(|(field_name, field_value)| {
                        format!("{}: {}", field_name, field_value.value_to_string_with_hir(heap, hir))
                    })
                    .collect()
            } else {
                // No field names - use generic field names
                struct_data.fields.iter()
                    .enumerate()
                    .map(|(i, v)| format!("field_{}: {}", i, v.value_to_string_with_hir(heap, hir)))
                    .collect()
            };
            
            format!("{} {{ {} }}", struct_name, field_strings.join(", "))
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
    code: &'static [OpCode], // Bytecode to execute (either top-level ops or function code) - cached, not cloned
    ip: usize,               // Instruction pointer (current position in code)
    locals: Box<[Value]>, // Local variable slots (indexed by var_id) - Box to avoid Vec allocation
    stack_depth: usize,   // Stack depth when this frame was entered (for cleanup on return)
}

pub struct VM {
    engine: std::sync::Arc<Engine>,                     // For native functions only (Arc so it can be stored in ValueHeap)
    bytecode_functions: HashMap<u32, BytecodeFunction>, // Compiled bytecode functions
    hir: HirAst,            // For constant lookups (cloned, but constants are small)
    type_registry: HashMap<u32, (String, Vec<String>)>, // Maps type_id -> (struct_name, field_names) for pretty printing
    ops: &'static [OpCode], // Top-level bytecode - cached, not cloned
    stack: Vec<Value>,
    call_stack: Vec<CallFrame>,
    heap: ValueHeap,
}

impl VM {
    pub fn new(
        mut engine: std::sync::Arc<Engine>,
        bytecode_functions: HashMap<u32, BytecodeFunction>,
        hir: HirAst,
        ops: Vec<OpCode>,
    ) -> Self {
        // Leak the bytecode to get a 'static reference - this is acceptable since
        // bytecode is created once and lives for the entire program lifetime
        let ops_box = Box::new(ops);
        let ops_slice: &'static [OpCode] = Box::leak(ops_box);

        // Build type registry: map type_id (computed from struct_name hash) -> (struct_name, field_names)
        let mut type_registry = HashMap::new();
        for (struct_name, struct_def) in &hir.structs {
            let type_id = compute_struct_type_id(struct_name);
            let field_names: Vec<String> = struct_def.fields.iter().map(|(name, _)| name.clone()).collect();
            type_registry.insert(type_id, (struct_name.clone(), field_names));
        }

        let mut heap = ValueHeap::new();
        heap.set_type_registry(type_registry.clone());
        heap.set_engine(engine.clone()); // Store Engine in heap for stdlib functions to invoke thunks
        heap.bytecode_functions = bytecode_functions.clone(); // Store bytecode functions in heap for stdlib functions to invoke

        Self {
            engine,
            bytecode_functions,
            hir,
            type_registry,
            ops: ops_slice,
            stack: Vec::new(),
            call_stack: Vec::new(),
            heap,
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
            locals: Box::new([]), // Empty locals for top-level
            stack_depth: 0,       // Top-level starts with empty stack
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
                    return; // The target function has returned
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
                        return; // The target function has returned
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
                            return; // The target function has returned
                        }
                    }
                    continue;
                }
                StepResult::Normal => {} // Normal execution, IP already incremented
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
            _vm.stack
                .push(Value::string_with_heap(s.clone(), &mut _vm.heap));
        }
        StepResult::Normal
    }

    #[inline(always)]
    fn op_ld_bool(_vm: &mut VM, _frame_idx: usize, opcode: &OpCode) -> StepResult {
        if let OpCode::LdBool(b) = opcode {
            _vm.stack.push(Value::boolean(*b));
        }
        StepResult::Normal
    }

    #[inline(always)]
    fn op_ld_var(_vm: &mut VM, frame_idx: usize, opcode: &OpCode) -> StepResult {
        if let OpCode::LdVar(id) = opcode {
            let idx = *id as usize;
            
            // Walk the call stack backwards to find the variable
            // This allows closures to capture variables from outer scopes
            let mut val = Value::none();
            let mut found = false;
            
            // Check frames from current to oldest (allowing closure capture)
            for i in (0..=frame_idx).rev() {
                if i >= _vm.call_stack.len() {
                    continue;
                }
                let frame = &_vm.call_stack[i];
                if idx < frame.locals.len() {
                    let frame_val = frame.locals[idx];
                    if !frame_val.is_none() {
                        // Found a non-None value in this frame
                        val = frame_val;
                        found = true;
                        break;
                    } else if i == frame_idx {
                        // In current frame, if variable exists but is None, 
                        // continue searching parent frames (might be a parameter slot)
                        // but remember this slot exists
                        found = true; // Mark as found so we don't return None unnecessarily
                    }
                }
            }
            
            // If we found the variable slot but it was None (in current frame),
            // return None. Otherwise return the value we found (or None if not found at all)
            if !found && frame_idx < _vm.call_stack.len() {
                let frame = &_vm.call_stack[frame_idx];
                if idx < frame.locals.len() {
                    val = frame.locals[idx]; // Return None from current frame
                }
            }
            
            _vm.stack.push(val);
        }
        StepResult::Normal
    }

    #[inline(always)]
    fn op_ld_const(_vm: &mut VM, _frame_idx: usize, opcode: &OpCode) -> StepResult {
        if let OpCode::LdConst(id) = opcode {
            use crate::core::compileSession::CompileSession;
            let const_val = CompileSession::get_constant_from_hir(&_vm.hir, *id, &mut _vm.heap);
            _vm.stack.push(const_val);
        }
        StepResult::Normal
    }

    #[inline(always)]
    fn op_ld_func(_vm: &mut VM, _frame_idx: usize, opcode: &OpCode) -> StepResult {
        if let OpCode::LdFunc(id) = opcode {
            let func_val = _vm.engine.as_ref().get_function(*id);
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
            _vm.stack
                .push(Value::number(if n == 0.0 { 1.0 } else { 0.0 }));
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
        let s = v.value_to_string_with_hir(&_vm.heap, Some(&_vm.hir));
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
        if let OpCode::MakePartial {
            func_id,
            bound_mask,
            hole_count: _,
        } = opcode
        {
            // Get function signature to know total parameter count
            let total_params = if let Some(func) = _vm.bytecode_functions.get(func_id) {
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

            // Pop bound arguments from stack
            // The emitter iterates bound.iter().rev() and pushes values in reverse order.
            // Since stack is LIFO, when we pop, we get values in the reverse order of how
            // they were pushed, which matches the bound array order (no reverse needed).
            // Example: bound = [None, Some(0), Some(1)]
            //   Emitter: bound.iter().rev() pushes 1.0, then 0.0
            //   Stack: [1.0, 0.0] (0.0 on top)
            //   Pop: 0.0, then 1.0 → popped_values = [0.0, 1.0] ✓ (matches bound order)
            let mut popped_values = Vec::new();
            for _ in 0..args_to_pop {
                popped_values.push(_vm.stack.pop().expect("Stack underflow"));
            }

            // Build bound_args vector: None for holes, Some(value) for bound args
            // Position i corresponds to function parameter i
            // The bound_mask tells us which positions are bound (bit i set = position i is bound)
            // popped_values contains the bound values in the order they appear in the bound array
            // We assign them sequentially to the bound positions in parameter order
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
        let popped_first = _vm.stack.pop().expect("Stack underflow"); // Pop first (top of stack)
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

    fn op_make_array(_vm: &mut VM, _frame_idx: usize, opcode: &OpCode) -> StepResult {
        if let OpCode::MakeArray(count) = opcode {
            let mut elements = Vec::new();
            for _ in 0..*count {
                elements.push(_vm.stack.pop().expect("Stack underflow in MakeArray"));
            }
            elements.reverse(); // Stack is LIFO, so reverse to get correct order
            let array = Value::array_with_heap(elements, &mut _vm.heap);
            _vm.stack.push(array);
        }
        StepResult::Normal
    }

    fn op_array_iter(_vm: &mut VM, _frame_idx: usize, _opcode: &OpCode) -> StepResult {
        let array_val = _vm.stack.pop().expect("Stack underflow in ArrayIter");
        if array_val.as_array(&_vm.heap).is_some() {
            // Get the array index from the value
            let array_idx = (array_val.raw & PAYLOAD_MASK) as usize;
            let iter = Value::array_iter_with_heap(array_idx, &mut _vm.heap);
            _vm.stack.push(iter);
        } else {
            panic!("ArrayIter expects array value");
        }
        StepResult::Normal
    }

    fn op_index(vm: &mut VM, _frame_idx: usize, _opcode: &OpCode) -> StepResult {
        let index_val = vm
            .stack
            .pop()
            .expect("Stack underflow in op_index (index)");
        let value = vm
            .stack
            .pop()
            .expect("Stack underflow in op_index (value)");
    
        fn compute_index(index_val: &Value, len: usize, kind: &str) -> usize {
            let idx = index_val
                .as_number()
                .unwrap_or_else(|| {
                    panic!("{} index must be a number, got: {:?}", kind, index_val)
                }) as i64;
    
            let len_i64 = len as i64;
            let adjusted = if idx < 0 { len_i64 + idx } else { idx };
    
            if adjusted < 0 || adjusted >= len_i64 {
                panic!(
                    "{} index out of bounds: {} (length: {})",
                    kind, idx, len
                );
            }
    
            adjusted as usize
        }
    
        if let Some(string) = value.as_string(&vm.heap) {
            let index = compute_index(&index_val, string.chars().count(), "String");

            let ch = string
                .chars()
                .nth(index)
                .expect("String index out of bounds");
            
            let ch_string = ch.to_string();
            vm.stack.push(Value::string_with_heap(ch_string, &mut vm.heap));
        } else if let Some(array) = value.as_array(&vm.heap) {
            let index = compute_index(&index_val, array.len(), "Array");
            vm.stack.push(array[index]);
        } else {
            panic!(
                "Index operation expects an indexable value (array or string), got: {:?}",
                value
            );
        }

        StepResult::Normal
    }


    fn op_array_slice(_vm: &mut VM, _frame_idx: usize, _opcode: &OpCode) -> StepResult {
        // Stack order (top to bottom): inclusive_flag, step, end, start, array
        let inclusive_flag_val = _vm.stack.pop().expect("Stack underflow in ArraySlice");
        let step_val = _vm.stack.pop().expect("Stack underflow in ArraySlice");
        let end_val = _vm.stack.pop().expect("Stack underflow in ArraySlice");
        let start_val = _vm.stack.pop().expect("Stack underflow in ArraySlice");
        let array_val = _vm.stack.pop().expect("Stack underflow in ArraySlice");

        // Sentinel value for "not specified": -999999999.0
        const SENTINEL_NONE: f64 = -999999999.0;

        if let Some(array) = array_val.as_array(&_vm.heap) {
            let array = array.clone(); // Clone to avoid borrow issues
            let array_len = array.len() as i64;

            // Extract inclusive_end flag
            let inclusive_end = if let Some(flag_num) = inclusive_flag_val.as_number() {
                flag_num != 0.0
            } else {
                false
            };

            // Extract start index (None sentinel means from start = 0)
            let start_idx = if let Some(start_num) = start_val.as_number() {
                if start_num == SENTINEL_NONE {
                    0
                } else {
                    let idx = start_num as i64;
                    // Handle negative indexing
                    let adjusted = if idx < 0 { array_len + idx } else { idx };
                    adjusted.max(0).min(array_len) as usize
                }
            } else {
                0
            };

            // Extract end index (None sentinel means to end = array_len)
            let end_idx = if let Some(end_num) = end_val.as_number() {
                if end_num == SENTINEL_NONE {
                    array_len as usize
                } else {
                    let idx = end_num as i64;
                    // Handle negative indexing
                    let adjusted = if idx < 0 { array_len + idx } else { idx };
                    if inclusive_end {
                        // Inclusive: include the end index, so add 1
                        (adjusted + 1).max(0).min((array_len + 1) as i64) as usize
                    } else {
                        // Exclusive: don't include end index
                        adjusted.max(0).min(array_len) as usize
                    }
                }
            } else {
                array_len as usize
            };

            // Extract step (None sentinel means step = 1)
            let step = if let Some(step_num) = step_val.as_number() {
                if step_num == SENTINEL_NONE {
                    1
                } else {
                    step_num as i64
                }
            } else {
                1
            };

            // Build sliced array
            let mut result = Vec::new();
            if step > 0 {
                // Forward slice
                let mut i = start_idx as i64;
                while i < end_idx as i64 {
                    result.push(array[i as usize]);
                    i += step;
                }
            } else if step < 0 {
                // Reverse slice (step is negative)
                let mut i = (end_idx as i64).saturating_sub(1);
                while i >= start_idx as i64 {
                    result.push(array[i as usize]);
                    i += step; // step is negative, so this decrements
                    if i < start_idx as i64 {
                        break;
                    }
                }
            } else {
                // Step of 0 is invalid
                panic!("Array slice step cannot be zero");
            }

            let sliced_array = Value::array_with_heap(result, &mut _vm.heap);
            _vm.stack.push(sliced_array);
        } else {
            panic!("ArraySlice expects array value, got: {:?}", array_val);
        }

        StepResult::Normal
    }

    fn op_array_next(_vm: &mut VM, _frame_idx: usize, _opcode: &OpCode) -> StepResult {
        let iter_val = _vm.stack.pop().expect("Stack underflow in ArrayNext");
        if iter_val.tag() == TAG_ARRAY_ITER {
            let iter_idx = (iter_val.raw & PAYLOAD_MASK) as usize;
            let array_idx = _vm.heap.array_iters[iter_idx].array_idx;
            let current_idx = _vm.heap.array_iters[iter_idx].current_idx;
            let array = &_vm.heap.arrays[array_idx];
            if current_idx < array.len() {
                let element = array[current_idx];
                _vm.heap.array_iters[iter_idx].current_idx += 1;
                let has_more = _vm.heap.array_iters[iter_idx].current_idx < array.len();
                // Push element first, then has_more
                _vm.stack.push(element);
                _vm.stack.push(Value::boolean(has_more));
            } else {
                // No more elements
                _vm.stack.push(Value::none()); // Use none as sentinel
                _vm.stack.push(Value::boolean(false));
            }
        } else {
            panic!("ArrayNext expects array iterator value");
        }
        StepResult::Normal
    }

    fn op_make_struct(_vm: &mut VM, _frame_idx: usize, opcode: &OpCode) -> StepResult {
        if let OpCode::MakeStruct { type_id, field_count } = opcode {
            let mut fields = Vec::new();
            for _ in 0..*field_count {
                fields.push(_vm.stack.pop().expect("Stack underflow in MakeStruct"));
            }
            fields.reverse(); // Stack is LIFO, so reverse to get correct order
            let struct_val = Value::struct_with_heap(*type_id, fields, &mut _vm.heap);
            _vm.stack.push(struct_val);
        }
        StepResult::Normal
    }

    fn op_map(vm: &mut VM, _frame_idx: usize, _opcode: &OpCode) -> StepResult {
        // Stack: [array, function] with function on top
        // Pop function first, then array
        let func_val = vm.stack.pop().expect("Stack underflow in Map (function)");
        let array_val = vm.stack.pop().expect("Stack underflow in Map (array)");
        
        // Get the array and clone elements to avoid borrow conflicts
        let array_idx = (array_val.raw & PAYLOAD_MASK) as usize;
        let array_data: Vec<Value> = vm.heap.arrays[array_idx].iter().copied().collect();
        
        // Get function ID
        let func_id = if let Some(id) = func_val.as_function() {
            id
        } else if let Some((id, _)) = func_val.as_thunk(&vm.heap) {
            id
        } else {
            panic!("Map expects function or thunk, got: {:?}", func_val);
        };
        
        // Map each element
        let mut result = Vec::new();
        for element in array_data.iter() {
            // Call function with element
            let mapped = vm.call_function(func_id, vec![*element]);
            result.push(mapped);
        }
        
        // Push result array
        let result_array = Value::array_with_heap(result, &mut vm.heap);
        vm.stack.push(result_array);
        StepResult::Normal
    }

    fn op_filter(vm: &mut VM, _frame_idx: usize, _opcode: &OpCode) -> StepResult {
        // Stack: [array, predicate] with predicate on top
        // Pop predicate first, then array
        let func_val = vm.stack.pop().expect("Stack underflow in Filter (predicate)");
        let array_val = vm.stack.pop().expect("Stack underflow in Filter (array)");
        
        // Get the array and clone elements to avoid borrow conflicts
        let array_idx = (array_val.raw & PAYLOAD_MASK) as usize;
        let array_data: Vec<Value> = vm.heap.arrays[array_idx].iter().copied().collect();
        
        // Get function ID
        let func_id = if let Some(id) = func_val.as_function() {
            id
        } else if let Some((id, _)) = func_val.as_thunk(&vm.heap) {
            id
        } else {
            panic!("Filter expects function or thunk, got: {:?}", func_val);
        };
        
        // Filter elements
        let mut result = Vec::new();
        for element in array_data.iter() {
            // Call predicate with element
            let keep = vm.call_function(func_id, vec![*element]);
            if keep.as_boolean().unwrap_or(false) {
                result.push(*element);
            }
        }
        
        // Push result array
        let result_array = Value::array_with_heap(result, &mut vm.heap);
        vm.stack.push(result_array);
        StepResult::Normal
    }

    fn op_fold(vm: &mut VM, _frame_idx: usize, _opcode: &OpCode) -> StepResult {
        // Stack: [array, initial_value, function] with function on top
        // Pop function first, then initial_value, then array
        let func_val = vm.stack.pop().expect("Stack underflow in Fold (function)");
        let init_val = vm.stack.pop().expect("Stack underflow in Fold (initial_value)");
        let array_val = vm.stack.pop().expect("Stack underflow in Fold (array)");
        
        // Get the array and clone elements to avoid borrow conflicts
        let array_idx = (array_val.raw & PAYLOAD_MASK) as usize;
        let array_data: Vec<Value> = vm.heap.arrays[array_idx].iter().copied().collect();
        
        // Get function ID
        let func_id = if let Some(id) = func_val.as_function() {
            id
        } else if let Some((id, _)) = func_val.as_thunk(&vm.heap) {
            id
        } else {
            panic!("Fold expects function or thunk, got: {:?}", func_val);
        };
        
        // Fold: start with initial value, apply function to each element
        let mut accumulator = init_val;
        for element in array_data.iter() {
            // Call function(accumulator, element)
            accumulator = vm.call_function(func_id, vec![accumulator, *element]);
        }
        
        // Push result
        vm.stack.push(accumulator);
        StepResult::Normal
    }

    fn op_get_field(_vm: &mut VM, _frame_idx: usize, opcode: &OpCode) -> StepResult {
        if let OpCode::GetField(field_index) = opcode {
            let struct_val = _vm.stack.pop().expect("Stack underflow in GetField");
            if let Some(struct_data) = struct_val.as_struct(&_vm.heap) {
                let field_value = struct_data.fields.get(*field_index as usize)
                    .copied()
                    .expect(&format!("Field index {} out of bounds", field_index));
                _vm.stack.push(field_value);
            } else {
                panic!("GetField expects struct value, got: {:?}", struct_val);
            }
        }
        StepResult::Normal
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
            self.stack
                .push(Value::string_with_heap(result, &mut self.heap));
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
                let lhs_type = if lhs.as_number().is_some() {
                    "number"
                } else if lhs.as_string(&self.heap).is_some() {
                    "string"
                } else if lhs.as_boolean().is_some() {
                    "boolean"
                } else if lhs.as_function().is_some() {
                    "function"
                } else if lhs.is_thunk() {
                    "thunk"
                } else if lhs.is_none() {
                    "none"
                } else {
                    "unknown"
                };
                let rhs_type = if rhs.as_number().is_some() {
                    "number"
                } else if rhs.as_string(&self.heap).is_some() {
                    "string"
                } else if rhs.as_boolean().is_some() {
                    "boolean"
                } else if rhs.as_function().is_some() {
                    "function"
                } else if rhs.is_thunk() {
                    "thunk"
                } else if rhs.is_none() {
                    "none"
                } else {
                    "unknown"
                };
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
            _ => panic!("Divide operation requires both operands to be numbers, got {:?} {:?}", lhs, rhs),
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
    fn init_locals_from_args(
        bytecode_func: &crate::core::engine::BytecodeFunction,
        args: &[Value],
    ) -> Box<[Value]> {
        let max_var_id = bytecode_func
            .param_var_ids
            .iter()
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
        let native_func = self
            .engine
            .functions
            .get(&func_id)
            .expect("Native function should exist");

        let result = (native_func.func)(args, &mut self.heap);
        
        // Note: Execution requests from native code are queued but not processed immediately.
        // They will be processed at a safe point (e.g., after the current execution cycle completes).
        // This prevents stack corruption and allows native code to request execution without
        // interfering with the current VM state.
        
        result
    }

    /// Process execution requests queued by native code.
    /// Native code can request execution of callables, and the VM performs the actual execution.
    /// This should be called at a safe point (e.g., between execution cycles, not during native function calls).
    /// 
    /// For now, execution requests are queued but not automatically processed.
    /// Future implementations may process them asynchronously or at specific safe points.
    #[allow(dead_code)]
    fn process_execution_requests(&mut self) {
        let requests = self.heap.take_execution_requests();
        for request in requests {
            let callable = request.callable();
            let args = request.args().to_vec();
            
            // Execute the callable based on its type
            if let Some(func_id) = callable.as_function() {
                let result = self.call_function(func_id, args);
                // Note: Results are not automatically pushed to stack here.
                // The caller should handle results appropriately.
                let _ = result; // Suppress unused warning for now
            } else if callable.is_thunk() {
                // Invoke the thunk with the provided arguments
                let result = self.invoke_thunk(callable, args);
                let _ = result; // Suppress unused warning for now
            } else {
                panic!("Execution request for non-callable value: {:?}", callable);
            }
        }
    }

    /// Create a new call frame for a bytecode function.
    /// Returns the frame count before pushing (for execute_until_empty target).
    fn create_bytecode_frame(
        &mut self,
        bytecode_func: &crate::core::engine::BytecodeFunction,
        args: Vec<Value>,
    ) -> usize {
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

    #[allow(dead_code)]
    /// Fill a hole in a thunk with a value.
    /// Returns the updated bound args with the hole filled.
    fn fill_thunk_hole(bound_args: &[Option<Value>], value: Value) -> Vec<Option<Value>> {
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

    #[allow(dead_code)]
    /// Apply an argument to a thunk or function, creating a new thunk.
    /// Handles regular thunks and functions.
    fn apply_arg_to_thunk(&mut self, thunk_or_func: Value, arg: Value) -> Value {
        if let Some(ThunkData::Regular { func_id, bound }) = thunk_or_func.as_thunk_ref(&self.heap)
        {
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
            } else if let Some(bytecode_func) = self.bytecode_functions.get(&func_id) {
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
            if let Some(ThunkData::Regular { func_id, bound }) = second_val.as_thunk_ref(&self.heap)
            {
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
                    let final_args: Vec<Value> = new_bound
                        .into_iter()
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
            } else if let Some(ThunkData::Composed {
                first: nested_first,
                second: nested_second,
            }) = second_val.as_thunk_ref(&self.heap)
            {
                // Composed thunk - recursively apply first_result to nested_first
                let nested_first_val = *nested_first;
                let nested_second_val = *nested_second;
                // Recursively apply first_result to nested_first
                let prepared_nested = if let Some((nested_func_id, nested_args)) =
                    nested_first_val.as_thunk(&self.heap)
                {
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
                    let arity = if let Some(native_func) =
                        self.engine.functions.get(&nested_func_id)
                    {
                        Self::min_arity(&native_func.arity)
                    } else if let Some(bytecode_func) = self.bytecode_functions.get(&nested_func_id)
                    {
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
                    // Since nested_first_val is a composed thunk, we need to apply first_result to its first part
                    if let Some(ThunkData::Composed {
                        first: deep_first,
                        second: deep_second,
                    }) = nested_first_val.as_thunk_ref(&self.heap)
                    {
                        let deep_first_val = *deep_first;
                        let deep_second_val = *deep_second;
                        // Apply first_result to deep_first
                        if let Some((deep_func_id, deep_args)) = deep_first_val.as_thunk(&self.heap)
                        {
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
                            let arity = if let Some(native_func) =
                                self.engine.functions.get(&deep_func_id)
                            {
                                Self::min_arity(&native_func.arity)
                            } else if let Some(bytecode_func) =
                                self.bytecode_functions.get(&deep_func_id)
                            {
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
                            // Deep first is also a composed thunk
                            // We need to apply first_result to deep_first_val, not get its result
                            // The correct way: apply first_result to the first part of nested_first_val
                            // Since we already have deep_first_val and deep_second_val, we can do:
                            let deep_first_prepared = if let Some((df_id, df_args)) =
                                deep_first_val.as_thunk(&self.heap)
                            {
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
                                let arity =
                                    if let Some(native_func) = self.engine.functions.get(&df_id) {
                                        Self::min_arity(&native_func.arity)
                                    } else if let Some(bytecode_func) =
                                        self.bytecode_functions.get(&df_id)
                                    {
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
                            Value::composed_thunk_with_heap(
                                deep_first_prepared,
                                deep_second_val,
                                &mut self.heap,
                            )
                        }
                    } else {
                        panic!("Expected nested_first_val to be a composed thunk");
                    }
                };
                let recomposed = Value::composed_thunk_with_heap(
                    prepared_nested,
                    nested_second_val,
                    &mut self.heap,
                );
                return self.invoke_thunk_value_recursive(recomposed);
            } else if let Some(func_id) = second_val.as_function() {
                // It's a function - create thunk with first_result as arg (and holes for remaining args)
                let arity = if let Some(native_func) = self.engine.functions.get(&func_id) {
                    Self::min_arity(&native_func.arity)
                } else if let Some(bytecode_func) = self.bytecode_functions.get(&func_id) {
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
                let final_args: Vec<Value> = bound.iter().filter_map(|opt| *opt).collect();
                let result = self.call_function(*func_id, final_args);
                return result;
            } else {
                panic!(
                    "Thunk with {} holes cannot be invoked without arguments",
                    hole_count
                );
            }
        } else if let Some((func_id, args)) = thunk.as_thunk(&self.heap) {
            // Regular thunk: check if all holes are filled
            let hole_count = args.iter().filter(|opt| opt.is_none()).count();
            if hole_count == 0 {
                // All holes filled - extract values and invoke
                let final_args: Vec<Value> = args.into_iter().filter_map(|opt| opt).collect();
                let result = self.call_function(func_id, final_args);
                return result;
            } else {
                panic!(
                    "Thunk with {} holes cannot be invoked without arguments",
                    hole_count
                );
            }
        } else {
            panic!("Invalid thunk value");
        }
    }

    /// Invoke a thunk with additional arguments, filling holes left-to-right.
    /// Returns a new thunk if not all holes are filled, otherwise invokes the function.
    #[allow(dead_code)]
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
                let final_args: Vec<Value> = filled
                    .into_iter()
                    .map(|opt| opt.expect("All holes should be filled"))
                    .collect();
                self.call_function(*func_id, final_args)
            }
        } else if let Some(ThunkData::Composed { first, second }) =
            thunk_val.as_thunk_ref(&self.heap)
        {
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
        } else {
            // Bytecode function: create a frame and execute until it returns
            // Extract clone in separate scope to ensure borrow is dropped
            let (bytecode_func, required_params) = {
                let func = self
                    .bytecode_functions
                    .get(&func_id)
                    .expect(&format!("Function {} not found", func_id));
                (func.clone(), func.param_var_ids.len())
            };

            // Safety check: ensure we have enough arguments
            if args.len() < required_params {
                // Extract debug info in separate scope to ensure all borrows are dropped
                let args_debug: Vec<String> =
                    { args.iter().map(|v| v.value_to_string(&self.heap)).collect() };
                panic!(
                    "Attempted to invoke function {} with {} args but it requires {}. Args: {:?}",
                    func_id,
                    args.len(),
                    required_params,
                    args_debug
                );
            }

            // Push new frame with function bytecode - borrow is definitely dropped now
            let initial_frame_count = self.create_bytecode_frame(&bytecode_func, args);

            // Execute until the function we just pushed returns
            // Use the shared execution method to avoid nested loops
            self.execute_until_empty(Some(initial_frame_count));

            // The function returned, result should be on the stack
            // CRITICAL: Pop the return value and restore stack to pre-thunk depth
            let stack_base = self.stack.len() - 1; // Account for return value
            let result = if self.stack.len() > stack_base {
                self.stack
                    .pop()
                    .expect("Function should have returned a value")
            } else {
                Value::none() // TODO: check if this is correct
            };
            // HARD RESET: Truncate stack to pre-thunk depth (removes all intermediate stack junk)
            self.stack.truncate(stack_base);
            result
        }
    }

    #[allow(dead_code)]
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
        } else {
            // Bytecode function: create a frame and execute until it returns
            // Extract clone in separate scope to ensure borrow is dropped
            let (bytecode_func, required_params) = {
                let func = self
                    .bytecode_functions
                    .get(&func_id)
                    .expect(&format!("Function {} not found", func_id));
                (func.clone(), func.param_var_ids.len())
            };

            // Safety check: ensure we have enough arguments
            if args.len() < required_params {
                // Extract debug info in separate scope to ensure all borrows are dropped
                let args_debug: Vec<String> =
                    { args.iter().map(|v| v.value_to_string(&self.heap)).collect() };
                panic!(
                    "Attempted to invoke function {} with {} args but it requires {}. Args: {:?}",
                    func_id,
                    args.len(),
                    required_params,
                    args_debug
                );
            }

            // Push new frame with function bytecode - borrow is definitely dropped now
            let initial_frame_count = self.create_bytecode_frame(&bytecode_func, args);

            // Execute until the function we just pushed returns
            // Use the shared execution method to avoid nested loops
            self.execute_until_empty(Some(initial_frame_count));

            // The function returned, result should be on the stack
            // CRITICAL: Pop the return value and restore stack to pre-thunk depth
            let result = if self.stack.len() > stack_base {
                self.stack
                    .pop()
                    .expect("Function should have returned a value")
            } else {
                Value::none() // TODO: check if this is correct
            };
            // HARD RESET: Truncate stack to pre-thunk depth (removes all intermediate stack junk)
            self.stack.truncate(stack_base);
            result
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
                if let Some(ThunkData::Composed {
                    first: nested_first,
                    second: nested_second,
                }) = first_val.as_thunk_ref(&self.heap)
                {
                    // Nested composition: apply args to the entire nested composed thunk
                    // CRITICAL: We need to apply the argument to the nested composition as a whole,
                    // not just to nested_first. The nested composition should be treated as a single unit.
                    // We'll create a thunk that applies the argument to the nested composition,
                    // then recompose with the outer second function.
                    // Apply new_args to the nested composition by creating a thunk that applies the args
                    // to nested_first, then we'll recompose properly
                    let nested_first_val = *nested_first;
                    let nested_second_val = *nested_second;
                    // Apply new_args to nested_first_val (the first part of the nested composition)
                    let prepared_nested_first = if new_args.len() == 1 {
                        // Single argument - apply it to nested_first_val
                        // Check for regular thunk with holes
                        if let Some(ThunkData::Regular { func_id, bound }) =
                            nested_first_val.as_thunk_ref(&self.heap)
                        {
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
                        } else if let Some((nf_id, nf_args)) = nested_first_val.as_thunk(&self.heap)
                        {
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
                            let arity = if let Some(native_func) = self.engine.functions.get(&nf_id)
                            {
                                Self::min_arity(&native_func.arity)
                            } else if let Some(bytecode_func) = self.bytecode_functions.get(&nf_id)
                            {
                                bytecode_func.param_var_ids.len()
                            } else {
                                1
                            };
                            let mut bound = vec![Some(new_args[0])];
                            while bound.len() < arity {
                                bound.push(None);
                            }
                            Value::thunk_with_heap(nf_id, bound, &mut self.heap)
                        } else if let Some(ThunkData::Composed {
                            first: deep_first,
                            second: deep_second,
                        }) = nested_first_val.as_thunk_ref(&self.heap)
                        {
                            // nested_first_val is itself a composed thunk - recursively apply the arg
                            let deep_first_val = *deep_first;
                            let deep_second_val = *deep_second;
                            let deep_prepared = if let Some(ThunkData::Regular { func_id, bound }) =
                                deep_first_val.as_thunk_ref(&self.heap)
                            {
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
                            } else if let Some((df_id, df_args)) =
                                deep_first_val.as_thunk(&self.heap)
                            {
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
                                let arity =
                                    if let Some(native_func) = self.engine.functions.get(&df_id) {
                                        Self::min_arity(&native_func.arity)
                                    } else if let Some(bytecode_func) =
                                        self.bytecode_functions.get(&df_id)
                                    {
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
                            Value::composed_thunk_with_heap(
                                deep_prepared,
                                deep_second_val,
                                &mut self.heap,
                            )
                        } else {
                            panic!("nested_first_val must be a thunk, function, or composed thunk, got: {:?}", nested_first_val);
                        }
                    } else {
                        panic!("Multiple args to nested composed thunk not yet supported");
                    };
                    // Recompose with nested_second_val to get the nested composition with args applied
                    let recomposed_nested = Value::composed_thunk_with_heap(
                        prepared_nested_first,
                        nested_second_val,
                        &mut self.heap,
                    );
                    // Now recompose with the outer second function
                    let composed = Value::composed_thunk_with_heap(
                        recomposed_nested,
                        second_val,
                        &mut self.heap,
                    );
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
                    if let Some(ThunkData::Regular { func_id, bound }) =
                        second_val.as_thunk_ref(&self.heap)
                    {
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
                    } else if let Some((second_func_id, second_args)) =
                        second_val.as_thunk(&self.heap)
                    {
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
                        let thunk =
                            Value::thunk_with_heap(second_func_id, combined_args, &mut self.heap);
                        self.stack.push(thunk);
                        return;
                    } else if let Some(second_func_id) = second_val.as_function() {
                        // second_val is a function - create a thunk with first_val and new_args
                        let arity =
                            if let Some(native_func) = self.engine.functions.get(&second_func_id) {
                                Self::min_arity(&native_func.arity)
                            } else if let Some(bytecode_func) =
                                self.bytecode_functions.get(&second_func_id)
                            {
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
                let (first_func_id, first_existing_args) =
                    if let Some((func_id, args)) = first_val.as_thunk(&self.heap) {
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
                let first_with_args =
                    Value::thunk_with_heap(first_func_id, first_final_args, &mut self.heap);

                // Recompose with the second function
                let composed =
                    Value::composed_thunk_with_heap(first_with_args, second_val, &mut self.heap);

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
                let prepared_call =
                    Value::thunk_with_heap(thunk_func_id, final_args, &mut self.heap);

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
            } else if let Some(bytecode_func) = self.bytecode_functions.get(&func_id) {
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
            panic!(
                "Expected function or thunk on stack for Thunk, got: {:?} (raw: 0x{:x})",
                func_val, func_val.raw
            );
        }
    }

    fn execute_ret_invoke(&mut self, frame_idx: usize) {
        // Tail-call elimination: reuse current frame instead of pushing a new one
        // Pop the prepared call from the stack
        let call = self.stack.pop().expect("Expected prepared call on stack");

        if !call.is_thunk() {
            panic!("RetInvoke expects Thunk value");
        }
        let (func_id, mut args) = call
            .as_thunk(&self.heap)
            .expect("RetInvoke expects Thunk value");

        // Get the required number of parameters for this function
        let required_params = if self.engine.functions.contains_key(&func_id) {
            // For native functions, use the arity
            Self::min_arity(&self.engine.functions.get(&func_id).unwrap().arity)
        } else if let Some(bytecode_func) = self.bytecode_functions.get(&func_id) {
            bytecode_func.param_var_ids.len()
        } else {
            panic!(
                "Function {} not found (neither native nor bytecode)",
                func_id
            );
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
                    self.stack
                        .push(Value::thunk_with_heap(func_id, args, &mut self.heap));
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
            self.stack
                .push(Value::thunk_with_heap(func_id, args, &mut self.heap));
            return;
        }

        // Extract values from Option<Value> for function call
        let final_args: Vec<Value> = args.into_iter().filter_map(|opt| opt).collect();

        if is_native {
            // Native functions: call directly and push result
            let result = self.call_native_function(func_id, final_args);
            self.stack.push(result);
            // For native functions, we still need to return, so pop the frame
            self.call_stack.pop();
        } else {
            // Bytecode functions: reuse current frame
            let bytecode_func = self
                .bytecode_functions
                .get(&func_id)
                .expect("Bytecode function should exist");

            if bytecode_func.code.is_empty() {
                panic!("Function {} has empty bytecode body", func_id);
            }

            // Initialize locals from arguments
            let locals = Self::init_locals_from_args(bytecode_func, &final_args);

            // Reuse the current frame: replace code, reset IP, replace locals
            let frame = &mut self.call_stack[frame_idx];
            frame.code = bytecode_func.code;
            frame.ip = 0; // Jump to beginning of callee
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
            if let Some(ThunkData::Regular { func_id, bound }) = second_val.as_thunk_ref(&self.heap)
            {
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
            } else if let Some(ThunkData::Composed {
                first: nested_first,
                second: nested_second,
            }) = second_val.as_thunk_ref(&self.heap)
            {
                // Composed thunk - recursively apply first_result to nested_first
                let nested_first_val = *nested_first;
                let nested_second_val = *nested_second;
                // Recursively apply first_result to nested_first
                let prepared_nested = if let Some(ThunkData::Regular { func_id, bound }) =
                    nested_first_val.as_thunk_ref(&self.heap)
                {
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
                } else if let Some((nested_func_id, nested_args)) =
                    nested_first_val.as_thunk(&self.heap)
                {
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
                    let arity = if let Some(native_func) =
                        self.engine.functions.get(&nested_func_id)
                    {
                        Self::min_arity(&native_func.arity)
                    } else if let Some(bytecode_func) = self.bytecode_functions.get(&nested_func_id)
                    {
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
                let recomposed = Value::composed_thunk_with_heap(
                    prepared_nested,
                    nested_second_val,
                    &mut self.heap,
                );
                let result = self.invoke_thunk_value_recursive(recomposed);
                self.stack.push(result);
                return;
            } else if let Some(func_id) = second_val.as_function() {
                // It's a function - create thunk with first_result as arg
                let arity = if let Some(native_func) = self.engine.functions.get(&func_id) {
                    Self::min_arity(&native_func.arity)
                } else if let Some(bytecode_func) = self.bytecode_functions.get(&func_id) {
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
        let (func_id, mut args) = call
            .as_thunk(&self.heap)
            .expect("Invoke expects regular Thunk value");

        // Get the required number of parameters for this function
        // Extract in separate scope to drop borrow before mutable operations
        let (required_params, is_native) = {
            if self.engine.functions.contains_key(&func_id) {
                // For native functions, use the arity
                let arity = &self.engine.functions.get(&func_id).unwrap().arity;
                (Self::min_arity(arity), true)
            } else if let Some(bytecode_func) = self.bytecode_functions.get(&func_id) {
                (bytecode_func.param_var_ids.len(), false)
            } else {
                panic!(
                    "Function {} not found (neither native nor bytecode)",
                    func_id
                );
            }
        };

        // Check if we need more arguments and pop them from the stack
        // This handles additional currying beyond what Thunk(n_args) already handled.
        let mut extra_args = Vec::new();
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
                    self.stack
                        .push(Value::thunk_with_heap(func_id, args, &mut self.heap));
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
            self.stack
                .push(Value::thunk_with_heap(func_id, args, &mut self.heap));
            return;
        }

        // Extract values from Option<Value> for function call
        let final_args: Vec<Value> = args.into_iter().filter_map(|opt| opt).collect();

        // TRAMPOLINE: Instead of calling invoke_thunk_sync recursively, push a frame and let the VM loop handle it
        if is_native {
            // Native functions: call directly (no recursion, just a simple function call)
            let result = self.call_native_function(func_id, final_args);
            self.stack.push(result);
        } else {
            // Bytecode functions: push frame and let VM loop handle execution (trampoline)
            // Extract clone in separate scope to ensure borrow is dropped
            let bytecode_func = {
                let func = self
                    .bytecode_functions
                    .get(&func_id)
                    .expect("Bytecode function should exist");
                func.clone()
            };
            // Now borrow is dropped, safe to mutate self
            self.create_bytecode_frame(&bytecode_func, final_args);
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
        } else {
            // Bytecode function: push new call frame
            // Extract clone in separate scope to ensure borrow is dropped
            let (bytecode_func, required_params) = {
                let func = self
                    .bytecode_functions
                    .get(&func_id)
                    .expect(&format!("Function {} not found", func_id));
                (func.clone(), func.param_var_ids.len())
            };

            // Safety check: ensure we have enough arguments
            if args.len() < required_params {
                panic!(
                    "Attempted to invoke function {} with {} args but it requires {}",
                    func_id,
                    args.len(),
                    required_params
                );
            }
            // Now borrow is dropped, safe to mutate self
            self.create_bytecode_frame(&bytecode_func, args);
            // Execution will continue in the main loop with the new frame
        }
    }
}
