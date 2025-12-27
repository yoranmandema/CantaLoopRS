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

    CallStack(u32),
    Thunk(u32),        // Prepare a call with arg_count (doesn't execute)
    Invoke,            // Execute a Thunk value from the stack
    Ret,
    
    // Control flow
    JmpIfFalse(usize),  // Pop value from stack, if false jump to offset
    Jmp(usize),         // Unconditional jump to offset
}