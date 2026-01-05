//! Virtual Machine implementation - THE canonical VM.
//! 
//! This is the single source of truth for VM execution logic.
//! It is generic over:
//! - Storage backend (FixedStorage for embedded, DynamicStorage for desktop)
//! - Host (DesktopHost for desktop, Esp32Host for embedded)
//! 
//! All execution logic lives here. Platform-specific code lives in Host implementations.

use crate::opcode::OpCode;
use crate::value::Value;
use crate::storage::VmStorage;
use crate::host::Host;
use crate::error::VmError;

/// Result of executing a step - indicates control flow behavior
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepResult {
    /// Normal execution, IP was incremented
    Normal,
    /// Special case (e.g., Ret), needs to restart loop
    Continue,
    /// VM has halted (end of program)
    Halted,
}

/// Call frame for function execution
#[cfg(feature = "std")]
pub struct CallFrame {
    pub code: &'static [OpCode],
    pub ip: usize,
    pub locals: std::vec::Vec<Value>, // Local variable slots
    pub stack_depth: usize,
}

/// Virtual Machine - THE canonical VM implementation.
/// 
/// This is the single source of truth for all VM execution logic.
/// It is generic over:
/// - `S: VmStorage` - Storage backend (FixedStorage or DynamicStorage)
/// - `H: Host` - Host implementation (DesktopHost or Esp32Host)
/// 
/// All execution logic lives here. Platform differences exist only in Storage and Host.
pub struct VM<S: VmStorage, H: Host> {
    /// Storage backend (stack + heap)
    pub storage: S,
    
    /// Host for platform-specific operations
    pub host: H,
    
    /// Current bytecode being executed
    pub code: &'static [OpCode],
    
    /// Instruction pointer
    pub ip: usize,
    
    /// Call stack
    #[cfg(feature = "std")]
    pub call_stack: std::vec::Vec<CallFrame>,
    
    // Note: For no_std, call_stack would need to be a fixed-size array
    // This is a limitation that needs to be addressed for full no_std support
}

impl<S: VmStorage, H: Host> VM<S, H> {
    /// Create a new VM with the given storage backend, host, and bytecode
    pub fn new(storage: S, host: H, code: &'static [OpCode]) -> Self {
        Self {
            storage,
            host,
            code,
            ip: 0,
            #[cfg(feature = "std")]
            call_stack: std::vec::Vec::new(),
        }
    }
    
    /// Push a value onto the stack
    pub fn push(&mut self, v: Value) -> Result<(), VmError> {
        self.storage.push(v)
    }
    
    /// Pop a value from the stack
    pub fn pop(&mut self) -> Option<Value> {
        self.storage.pop()
    }
    
    /// Peek at the top of the stack
    pub fn peek(&self) -> Option<Value> {
        self.storage.peek()
    }
    
    /// Get current stack depth
    pub fn stack_depth(&self) -> usize {
        self.storage.stack_depth()
    }
    
    /// Execute a single instruction
    pub fn step(&mut self) -> Result<StepResult, VmError> {
        if self.ip >= self.code.len() {
            return Ok(StepResult::Halted);
        }
        
        let opcode = &self.code[self.ip];
        self.ip += 1;
        
        match opcode {
            OpCode::LdNum(n) => {
                self.push(Value::number(*n))?;
                Ok(StepResult::Continue)
            }
            #[cfg(feature = "std")]
            OpCode::LdStr(s) => {
                let val = Value::string_with_storage(s.clone(), &mut self.storage)?;
                self.push(val)?;
                Ok(StepResult::Continue)
            }
            OpCode::LdBool(b) => {
                self.push(Value::boolean(*b))?;
                Ok(StepResult::Continue)
            }
            OpCode::LdVar(_var_id) => {
                // TODO: Implement variable loading (needs locals/frame support)
                Err(VmError::InvalidOperation)
            }
            OpCode::LdConst(_const_id) => {
                // TODO: Implement constant loading
                Err(VmError::InvalidOperation)
            }
            OpCode::LdFunc(id) => {
                self.push(Value::function(*id))?;
                Ok(StepResult::Continue)
            }
            OpCode::Add => {
                let b = self.pop().ok_or(VmError::StackUnderflow)?;
                let a = self.pop().ok_or(VmError::StackUnderflow)?;
                if let (Some(na), Some(nb)) = (a.as_number(), b.as_number()) {
                    self.push(Value::number(na + nb))?;
                    Ok(StepResult::Continue)
                } else {
                    Err(VmError::InvalidOperation)
                }
            }
            OpCode::Sub => {
                let b = self.pop().ok_or(VmError::StackUnderflow)?;
                let a = self.pop().ok_or(VmError::StackUnderflow)?;
                if let (Some(na), Some(nb)) = (a.as_number(), b.as_number()) {
                    self.push(Value::number(na - nb))?;
                    Ok(StepResult::Continue)
                } else {
                    Err(VmError::InvalidOperation)
                }
            }
            OpCode::Mul => {
                let b = self.pop().ok_or(VmError::StackUnderflow)?;
                let a = self.pop().ok_or(VmError::StackUnderflow)?;
                if let (Some(na), Some(nb)) = (a.as_number(), b.as_number()) {
                    self.push(Value::number(na * nb))?;
                    Ok(StepResult::Continue)
                } else {
                    Err(VmError::InvalidOperation)
                }
            }
            OpCode::Div => {
                let b = self.pop().ok_or(VmError::StackUnderflow)?;
                let a = self.pop().ok_or(VmError::StackUnderflow)?;
                if let (Some(na), Some(nb)) = (a.as_number(), b.as_number()) {
                    if nb == 0.0 {
                        return Err(VmError::InvalidOperation);
                    }
                    self.push(Value::number(na / nb))?;
                    Ok(StepResult::Continue)
                } else {
                    Err(VmError::InvalidOperation)
                }
            }
            OpCode::Pop => {
                self.pop();
                Ok(StepResult::Continue)
            }
            OpCode::Eq => {
                let b = self.pop().ok_or(VmError::StackUnderflow)?;
                let a = self.pop().ok_or(VmError::StackUnderflow)?;
                let eq = match (a.as_number(), b.as_number()) {
                    (Some(na), Some(nb)) => na == nb,
                    (None, None) => {
                        // Compare booleans
                        match (a.as_boolean(), b.as_boolean()) {
                            (Some(ba), Some(bb)) => ba == bb,
                            _ => {
                                // For other types, compare function IDs or use a more sophisticated comparison
                                // For now, just compare if they're the same type
                                a.as_function() == b.as_function()
                            }
                        }
                    }
                    _ => false,
                };
                self.push(Value::boolean(eq))?;
                Ok(StepResult::Continue)
            }
            OpCode::MakeArray(count) => {
                // Pop count values from stack
                // For no_std compatibility, we need to collect into a fixed-size array
                // Since we can't use Vec, we'll use a temporary array and pass a slice
                const MAX_ARRAY_ELEMENTS: usize = 64;
                if *count as usize > MAX_ARRAY_ELEMENTS {
                    return Err(VmError::HeapFull);
                }
                
                // Collect values from stack (LIFO order)
                let mut temp = [Value::none(); MAX_ARRAY_ELEMENTS];
                for i in 0..*count as usize {
                    temp[i] = self.pop().ok_or(VmError::StackUnderflow)?;
                }
                
                // Reverse to get correct order
                let mut elements = [Value::none(); MAX_ARRAY_ELEMENTS];
                for i in 0..*count as usize {
                    elements[i] = temp[*count as usize - 1 - i];
                }
                
                // Create array from slice
                let array = Value::array_with_storage(&elements[..*count as usize], &mut self.storage)?;
                self.push(array)?;
                Ok(StepResult::Continue)
            }
            _ => {
                // Other opcodes not yet implemented
                Err(VmError::InvalidOperation)
            }
        }
    }
    
    /// Run the VM until it halts or encounters an error
    pub fn run(&mut self) -> Result<(), VmError> {
        loop {
            match self.step()? {
                StepResult::Normal => continue,
                StepResult::Continue => continue,
                StepResult::Halted => break,
            }
        }
        Ok(())
    }
}


// Type aliases for convenience
// Note: These require Host implementations which will be in platform-specific crates
// For now, we just export the generic VM type

// For embedded, you would use:
// pub type EmbeddedVM<const STACK: usize, const HEAP: usize> = VM<FixedStorage<STACK, HEAP>>;
// Example: pub type EmbeddedVM = VM<FixedStorage<256, 64>>;

