#![no_std]
#![cfg_attr(feature = "std", allow(unused_imports))]

// When std is available, re-export it for convenience
#[cfg(feature = "std")]
extern crate std;

pub mod error;
pub mod host;
pub mod opcode;
pub mod storage;
pub mod value;
pub mod vm;

pub use error::VmError;
pub use host::{Host, BytecodeFunction, NullHost};
pub use opcode::OpCode;
pub use storage::{VmStorage, FixedStorage, DynamicStorage};
pub use value::Value;
pub use vm::{VM, CallFrame, StepResult};

