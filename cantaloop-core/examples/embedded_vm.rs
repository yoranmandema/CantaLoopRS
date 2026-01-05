//! Example of using the embedded VM with FixedStorage
//! 
//! This example demonstrates how to create and use a VM with fixed-size storage
//! for embedded systems.

use cantaloop_core::{
    FixedStorage, VM, OpCode, VmError, NullHost,
};

// Define the embedded VM type with fixed storage sizes
// 256 stack slots, 64 heap slots
// Using NullHost for the example (no native functions)
type EmbeddedVM = VM<FixedStorage<256, 64>, NullHost>;

fn main() -> Result<(), VmError> {
    // Create fixed-size storage
    let storage = FixedStorage::<256, 64>::new();
    let host = NullHost;
    
    // Create a simple bytecode program: push 2, push 3, add, result should be 5
    let bytecode: &'static [OpCode] = Box::leak(Box::new([
        OpCode::LdNum(2.0),
        OpCode::LdNum(3.0),
        OpCode::Add,
    ]));
    
    // Create VM with the bytecode
    let mut vm = EmbeddedVM::new(storage, host, bytecode);
    
    // Run the VM
    vm.run()?;
    
    // Get the result from the stack
    if let Some(result) = vm.pop() {
        if let Some(n) = result.as_number() {
            println!("Result: {}", n);
            assert_eq!(n, 5.0);
        } else {
            eprintln!("Expected number result");
        }
    } else {
        eprintln!("Stack is empty");
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_simple_arithmetic() -> Result<(), VmError> {
        let storage = FixedStorage::<256, 64>::new();
        let host = NullHost;
        let bytecode: &'static [OpCode] = Box::leak(Box::new([
            OpCode::LdNum(10.0),
            OpCode::LdNum(5.0),
            OpCode::Sub,
        ]));
        
        let mut vm = EmbeddedVM::new(storage, host, bytecode);
        vm.run()?;
        
        let result = vm.pop().ok_or(VmError::StackUnderflow)?;
        assert_eq!(result.as_number(), Some(5.0));
        
        Ok(())
    }
    
    #[test]
    fn test_array_creation() -> Result<(), VmError> {
        let storage = FixedStorage::<256, 64>::new();
        let host = NullHost;
        let bytecode: &'static [OpCode] = Box::leak(Box::new([
            OpCode::LdNum(1.0),
            OpCode::LdNum(2.0),
            OpCode::LdNum(3.0),
            OpCode::MakeArray(3),
        ]));
        
        let mut vm = EmbeddedVM::new(storage, host, bytecode);
        vm.run()?;
        
        let array_val = vm.pop().ok_or(VmError::StackUnderflow)?;
        let array = array_val.as_array(&vm.storage).ok_or(VmError::InvalidOperation)?;
        
        assert_eq!(array.len(), 3);
        assert_eq!(array[0].as_number(), Some(1.0));
        assert_eq!(array[1].as_number(), Some(2.0));
        assert_eq!(array[2].as_number(), Some(3.0));
        
        Ok(())
    }
}

