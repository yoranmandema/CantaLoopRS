/// VM execution errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmError {
    /// Stack overflow - tried to push when stack is full
    StackOverflow,
    /// Stack underflow - tried to pop from empty stack
    StackUnderflow,
    /// Global variable index out of bounds
    GlobalOutOfBounds,
    /// Heap allocation failed (for fixed-size storage)
    HeapFull,
    /// Invalid operation
    InvalidOperation,
}

#[cfg(feature = "std")]
impl std::fmt::Display for VmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VmError::StackOverflow => write!(f, "Stack overflow"),
            VmError::StackUnderflow => write!(f, "Stack underflow"),
            VmError::GlobalOutOfBounds => write!(f, "Global variable index out of bounds"),
            VmError::HeapFull => write!(f, "Heap is full"),
            VmError::InvalidOperation => write!(f, "Invalid operation"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for VmError {}

