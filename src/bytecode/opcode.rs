/*
    Naming conventions:
    Loading data into address: ld*
*/
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
    Pow,
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

    Print,          // Inline print builtin
    CallStack(u32),
    Thunk(u32),        // Prepare a call with arg_count (doesn't execute)
    Invoke,            // Execute a Thunk value from the stack
    Ret,
    
    // Control flow
    JmpIfFalse(usize),  // Pop value from stack, if false jump to offset
    Jmp(usize),         // Unconditional jump to offset
}

impl OpCode {
    /// Get the opcode discriminant as a u8 for dispatch table indexing
    pub fn discriminant(&self) -> u8 {
        use std::mem::discriminant;
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
            OpCode::Pow => 9,
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
            OpCode::Print => 21,
            OpCode::CallStack(_) => 22,
            OpCode::Thunk(_) => 23,
            OpCode::Invoke => 24,
            OpCode::Ret => 25,
            OpCode::JmpIfFalse(_) => 26,
            OpCode::Jmp(_) => 27,
        }
    }
}

pub const OPCODE_COUNT: usize = 28;