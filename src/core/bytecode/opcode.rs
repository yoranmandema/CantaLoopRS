/// Bytecode instruction set for the CantaLoop virtual machine.
/// 
/// Instructions follow naming convention: `ld*` for loading values,
/// `st*` for storing values. The VM uses a stack-based execution model.
#[derive(Debug, Clone)]
pub enum OpCode {
    LdNum(f64),     // Load a number onto the stack
    LdStr(String),  // Load a string onto the stack
    LdBool(bool),  // Load a boolean onto the stack

    LdVar(u32),     // Load a variable
    LdConst(u32),   // Load a constant (immutable)
    LdFunc(u32),    // Load a function by function ID

    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    AddNum,  // Optimized: both operands are statically known to be numbers
    MulNum,  // Optimized: both operands are statically known to be numbers
    SubNum,  // Optimized: both operands are statically known to be numbers
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
    And,
    Or,
    Neg,
    Not,

    StVar(u32),     // Pop value from stack and store into variable address
    Pop,            // Pop and discard a value from the stack

    #[allow(dead_code)]
    Print,          // Inline print builtin
    CallStack(u32),
    Thunk(u32),        // Prepare a call with arg_count (doesn't execute)
    MakePartial { func_id: u32, bound_mask: u64, hole_count: u32 }, // Create partial thunk: func_id, bitmask of bound args (1=bound, 0=hole), number of holes
    ComposeThunk,      // Compose two thunks: pops g, then f, pushes composed thunk (g(f(x)))
    Invoke,            // Execute a Thunk value from the stack
    Ret,
    RetInvoke,         // Tail-call: Execute a Thunk and return (reuse current frame)
    
    // Control flow
    JmpIfFalse(usize),  // Pop value from stack, if false jump to offset
    JmpIfTrue(usize),   // Pop value from stack, if true jump to offset
    Jmp(usize),         // Unconditional jump to offset
    
    // Array operations
    MakeArray(u32),     // Create array from n values on stack (pops n values, pushes array)
    ArrayIter,          // Start iteration: pops array, pushes iterator
    ArrayNext,          // Get next element: pops iterator, pushes (has_more: bool, element: value)
    ArrayIndex,         // Index array: pops (array, index), pushes element at index
    ArraySlice,         // Slice array: pops (array, step?, end?, start?), pushes sliced array
    // ArraySlice stack order: array, then optional step, optional end, optional start (None values use sentinel)
}

impl OpCode {
    /// Returns the opcode discriminant as a u8 for dispatch table indexing.
    /// 
    /// This enables efficient opcode dispatch using a function pointer table
    /// instead of match statements.
    pub fn discriminant(&self) -> u8 {
        match self {
            OpCode::LdNum(_) => 0,
            OpCode::LdStr(_) => 1,
            OpCode::LdBool(_) => 2,
            OpCode::LdVar(_) => 3,
            OpCode::LdConst(_) => 4,
            OpCode::LdFunc(_) => 5,
            OpCode::Add => 6,
            OpCode::Sub => 7,
            OpCode::Mul => 8,
            OpCode::Div => 9,
            OpCode::Mod => 10,
            OpCode::Pow => 11,
            OpCode::AddNum => 12,
            OpCode::MulNum => 13,
            OpCode::SubNum => 14,
            OpCode::Eq => 15,
            OpCode::Ne => 16,
            OpCode::Gt => 17,
            OpCode::Lt => 18,
            OpCode::Ge => 19,
            OpCode::Le => 20,
            OpCode::And => 21,
            OpCode::Or => 22,
            OpCode::Neg => 23,
            OpCode::Not => 24,
            OpCode::StVar(_) => 25,
            OpCode::Pop => 26,
            OpCode::Print => 27,
            OpCode::CallStack(_) => 28,
            OpCode::Thunk(_) => 29,
            OpCode::MakePartial { .. } => 30,
            OpCode::ComposeThunk => 31,
            OpCode::Invoke => 32,
            OpCode::Ret => 33,
            OpCode::RetInvoke => 34,
            OpCode::JmpIfFalse(_) => 35,
            OpCode::JmpIfTrue(_) => 36,
            OpCode::Jmp(_) => 37,
            OpCode::MakeArray(_) => 38,
            OpCode::ArrayIter => 39,
            OpCode::ArrayNext => 40,
            OpCode::ArrayIndex => 41,
            OpCode::ArraySlice => 42,
        }
    }
}

pub const OPCODE_COUNT: usize = 43;