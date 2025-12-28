/// Bytecode instruction set for the CantaLoop virtual machine.
/// 
/// Instructions follow naming convention: `ld*` for loading values,
/// `st*` for storing values. The VM uses a stack-based execution model.
#[derive(Debug, Clone)]
pub enum OpCode {
    LdNum(f64),     // Load a number onto the stack
    LdStr(String),  // Load a string onto the stack

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
    ComposeThunk,      // Compose two thunks: pops g, then f, pushes composed thunk (g(f(x)))
    Invoke,            // Execute a Thunk value from the stack
    Ret,
    RetInvoke,         // Tail-call: Execute a Thunk and return (reuse current frame)
    
    // Control flow
    JmpIfFalse(usize),  // Pop value from stack, if false jump to offset
    JmpIfTrue(usize),   // Pop value from stack, if true jump to offset
    Jmp(usize),         // Unconditional jump to offset
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
            OpCode::LdVar(_) => 2,
            OpCode::LdConst(_) => 3,
            OpCode::LdFunc(_) => 4,
            OpCode::Add => 5,
            OpCode::Sub => 6,
            OpCode::Mul => 7,
            OpCode::Div => 8,
            OpCode::Mod => 35,
            OpCode::Pow => 9,
            OpCode::AddNum => 29,
            OpCode::MulNum => 30,
            OpCode::SubNum => 31,
            OpCode::Eq => 10,
            OpCode::Ne => 11,
            OpCode::Gt => 12,
            OpCode::Lt => 13,
            OpCode::Ge => 14,
            OpCode::Le => 15,
            OpCode::And => 16,
            OpCode::Or => 17,
            OpCode::Neg => 18,
            OpCode::Not => 19,
            OpCode::StVar(_) => 20,
            OpCode::Pop => 21,
            OpCode::Print => 22,
            OpCode::CallStack(_) => 23,
            OpCode::Thunk(_) => 24,
            OpCode::ComposeThunk => 34,
            OpCode::Invoke => 25,
            OpCode::Ret => 26,
            OpCode::RetInvoke => 32,
            OpCode::JmpIfFalse(_) => 27,
            OpCode::JmpIfTrue(_) => 33,
            OpCode::Jmp(_) => 28,
        }
    }
}

pub const OPCODE_COUNT: usize = 36;