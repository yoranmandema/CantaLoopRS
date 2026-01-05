//! Host trait for platform-specific operations.
//! 
//! The Host trait abstracts over platform-specific functionality that the VM needs:
//! - Native function calls
//! - Constant lookups
//! - Type registry access
//! 
//! This allows the same VM to work on desktop, ESP32, or any other platform
//! by providing different Host implementations.

use crate::value::Value;
use crate::error::VmError;

/// Trait for platform-specific host operations.
/// 
/// Implementations provide:
/// - Native function execution
/// - Constant value lookups
/// - Type information
/// 
/// Desktop and embedded platforms implement this differently:
/// - Desktop: Uses Engine with stdlib functions
/// - ESP32: Uses embedded-specific native functions
pub trait Host {
    /// Call a native function by ID with the given arguments.
    /// 
    /// Returns the result value, or an error if the function doesn't exist
    /// or execution fails.
    fn call_native_function(&mut self, func_id: u32, args: &[Value]) -> Result<Value, VmError>;
    
    /// Get a constant value by ID.
    /// 
    /// Constants are compile-time values that don't change at runtime.
    fn get_constant(&self, const_id: u32) -> Option<Value>;
    
    /// Get type registry information for a struct type ID.
    /// 
    /// Returns (struct_name, field_names) if the type is known.
    #[cfg(feature = "std")]
    fn get_type_info(&self, type_id: u32) -> Option<(&str, &[std::string::String])>;
    
    /// Get a bytecode function by ID.
    /// 
    /// Returns the bytecode function if it exists.
    fn get_bytecode_function(&self, func_id: u32) -> Option<BytecodeFunction>;
}

/// A bytecode function that can be executed by the VM.
#[derive(Clone)]
pub struct BytecodeFunction {
    /// The compiled bytecode instructions
    pub code: &'static [crate::opcode::OpCode],
    /// Variable IDs for function parameters, in order
    pub param_var_ids: &'static [u32],
}

/// Null host implementation for testing or when no host is needed.
/// 
/// All operations return errors or None.
pub struct NullHost;

impl Host for NullHost {
    fn call_native_function(&mut self, _func_id: u32, _args: &[Value]) -> Result<Value, VmError> {
        Err(VmError::InvalidOperation)
    }
    
    fn get_constant(&self, _const_id: u32) -> Option<Value> {
        None
    }
    
    #[cfg(feature = "std")]
    fn get_type_info(&self, _type_id: u32) -> Option<(&str, &[std::string::String])> {
        None
    }
    
    fn get_bytecode_function(&self, _func_id: u32) -> Option<BytecodeFunction> {
        None
    }
}

