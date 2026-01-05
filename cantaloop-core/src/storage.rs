use crate::value::{Value, ArrayIterator, StructData, ThunkData};
use crate::error::VmError;

/// Trait for VM storage backends.
/// 
/// This abstraction allows the VM to work with both:
/// - Fixed-size storage (for embedded systems, no heap allocation)
/// - Dynamic storage (for desktop, uses Vec/HashMap)
pub trait VmStorage {
    /// Push a value onto the stack.
    /// Returns an error if the stack is full.
    fn push(&mut self, v: Value) -> Result<(), VmError>;
    
    /// Pop a value from the stack.
    /// Returns None if the stack is empty.
    fn pop(&mut self) -> Option<Value>;
    
    /// Peek at the top of the stack without removing it.
    fn peek(&self) -> Option<Value>;
    
    /// Get the current stack depth.
    fn stack_depth(&self) -> usize;
    
    /// Allocate a string in the heap and return its index.
    #[cfg(feature = "std")]
    fn alloc_string(&mut self, s: std::string::String) -> Result<usize, VmError>;
    
    /// Get a string from the heap by index.
    #[cfg(feature = "std")]
    fn get_string(&self, idx: usize) -> Option<&str>;
    
    /// Allocate an array in the heap and return its index.
    fn alloc_array(&mut self, elements: &[Value]) -> Result<usize, VmError>;
    
    /// Get an array from the heap by index.
    fn get_array(&self, idx: usize) -> Option<&[Value]>;
    
    /// Get a mutable array from the heap by index.
    fn get_array_mut(&mut self, idx: usize) -> Option<&mut [Value]>;
    
    /// Allocate a struct in the heap and return its index.
    fn alloc_struct(&mut self, type_id: u32, fields: &[Value]) -> Result<usize, VmError>;
    
    /// Get a struct from the heap by index.
    fn get_struct(&self, idx: usize) -> Option<&StructData>;
    
    /// Allocate a thunk in the heap and return its index.
    fn alloc_thunk(&mut self, thunk: ThunkData) -> Result<usize, VmError>;
    
    /// Get a thunk from the heap by index.
    fn get_thunk(&self, idx: usize) -> Option<&ThunkData>;
    
    /// Get a mutable thunk from the heap by index.
    fn get_thunk_mut(&mut self, idx: usize) -> Option<&mut ThunkData>;
    
    /// Allocate an array iterator in the heap and return its index.
    fn alloc_array_iter(&mut self, array_idx: usize) -> Result<usize, VmError>;
    
    /// Get an array iterator from the heap by index.
    fn get_array_iter_mut(&mut self, idx: usize) -> Option<&mut ArrayIterator>;
}

/// Fixed-size storage for embedded systems (no heap allocation).
/// 
/// Uses fixed-size arrays for all storage. All memory is allocated up front.
/// 
/// NOTE: This is a simplified version. For full no_std support, you'll need to:
/// 1. Add heapless crate for Vec/String replacements
/// 2. Implement fixed-size field storage for StructData
/// 3. Implement fixed-size bound storage for ThunkData
pub struct FixedStorage<const STACK_SIZE: usize, const HEAP_SIZE: usize> {
    stack: [Option<Value>; STACK_SIZE],
    stack_ptr: usize,
    
    // Heap storage - using Option to track allocation
    // arrays: Using fixed-size arrays with a simple free-list approach
    arrays: [Option<FixedArray>; HEAP_SIZE],
    array_count: usize,
    
    structs: [Option<StructData>; HEAP_SIZE],
    struct_count: usize,
    
    thunks: [Option<ThunkData>; HEAP_SIZE],
    thunk_count: usize,
    
    array_iters: [Option<ArrayIterator>; HEAP_SIZE],
    array_iter_count: usize,
}

// Simple fixed-size array storage (max 64 elements per array)
struct FixedArray {
    data: [Option<Value>; 64],
    len: usize,
}

impl FixedArray {
    fn new() -> Self {
        Self {
            data: [None; 64],
            len: 0,
        }
    }
    
    fn from_slice(elements: &[Value]) -> Result<Self, VmError> {
        if elements.len() > 64 {
            return Err(VmError::HeapFull);
        }
        let mut arr = Self::new();
        for (i, &elem) in elements.iter().enumerate() {
            arr.data[i] = Some(elem);
        }
        arr.len = elements.len();
        Ok(arr)
    }
    
    fn as_slice(&self) -> &[Value] {
        // This is unsafe but necessary for the API
        // In practice, you'd use a proper fixed-size vec from heapless
        unsafe {
            core::slice::from_raw_parts(
                self.data.as_ptr() as *const Value,
                self.len
            )
        }
    }
    
    fn as_slice_mut(&mut self) -> &mut [Value] {
        unsafe {
            core::slice::from_raw_parts_mut(
                self.data.as_mut_ptr() as *mut Value,
                self.len
            )
        }
    }
}

impl<const STACK_SIZE: usize, const HEAP_SIZE: usize> FixedStorage<STACK_SIZE, HEAP_SIZE> {
    pub fn new() -> Self {
        Self {
            stack: [None; STACK_SIZE],
            stack_ptr: 0,
            arrays: [(); HEAP_SIZE].map(|_| None),
            array_count: 0,
            structs: [(); HEAP_SIZE].map(|_| None),
            struct_count: 0,
            thunks: [(); HEAP_SIZE].map(|_| None),
            thunk_count: 0,
            array_iters: [(); HEAP_SIZE].map(|_| None),
            array_iter_count: 0,
        }
    }
}

impl<const STACK_SIZE: usize, const HEAP_SIZE: usize> VmStorage for FixedStorage<STACK_SIZE, HEAP_SIZE> {
    fn push(&mut self, v: Value) -> Result<(), VmError> {
        if self.stack_ptr >= STACK_SIZE {
            return Err(VmError::StackOverflow);
        }
        self.stack[self.stack_ptr] = Some(v);
        self.stack_ptr += 1;
        Ok(())
    }
    
    fn pop(&mut self) -> Option<Value> {
        if self.stack_ptr == 0 {
            return None;
        }
        self.stack_ptr -= 1;
        self.stack[self.stack_ptr].take()
    }
    
    fn peek(&self) -> Option<Value> {
        if self.stack_ptr == 0 {
            None
        } else {
            self.stack[self.stack_ptr - 1]
        }
    }
    
    fn stack_depth(&self) -> usize {
        self.stack_ptr
    }
    
    #[cfg(feature = "std")]
    fn alloc_string(&mut self, _s: std::string::String) -> Result<usize, VmError> {
        // Fixed storage doesn't support std strings in no_std mode
        Err(VmError::InvalidOperation)
    }
    
    #[cfg(feature = "std")]
    fn get_string(&self, _idx: usize) -> Option<&str> {
        None
    }
    
    fn alloc_array(&mut self, elements: &[Value]) -> Result<usize, VmError> {
        if self.array_count >= HEAP_SIZE {
            return Err(VmError::HeapFull);
        }
        // Find first free slot
        for i in 0..HEAP_SIZE {
            if self.arrays[i].is_none() {
                let fixed_arr = FixedArray::from_slice(elements)?;
                self.arrays[i] = Some(fixed_arr);
                self.array_count += 1;
                return Ok(i);
            }
        }
        Err(VmError::HeapFull)
    }
    
    fn get_array(&self, idx: usize) -> Option<&[Value]> {
        self.arrays.get(idx)?.as_ref().map(|v| v.as_slice())
    }
    
    fn get_array_mut(&mut self, idx: usize) -> Option<&mut [Value]> {
        self.arrays.get_mut(idx)?.as_mut().map(|v| v.as_slice_mut())
    }
    
    fn alloc_struct(&mut self, type_id: u32, fields: &[Value]) -> Result<usize, VmError> {
        if self.struct_count >= HEAP_SIZE {
            return Err(VmError::HeapFull);
        }
        // Find first free slot
        for i in 0..HEAP_SIZE {
            if self.structs[i].is_none() {
                #[cfg(feature = "std")]
                {
                    use crate::value::StructData;
                    self.structs[i] = Some(StructData {
                        type_id,
                        fields: fields.to_vec(),
                    });
                }
                #[cfg(not(feature = "std"))]
                {
                    use crate::value::{StructData, FixedFieldArray};
                    let field_array = FixedFieldArray::from_slice(fields)?;
                    self.structs[i] = Some(StructData {
                        type_id,
                        fields: field_array,
                    });
                }
                self.struct_count += 1;
                return Ok(i);
            }
        }
        Err(VmError::HeapFull)
    }
    
    fn get_struct(&self, idx: usize) -> Option<&StructData> {
        self.structs.get(idx)?.as_ref()
    }
    
    fn alloc_thunk(&mut self, thunk: ThunkData) -> Result<usize, VmError> {
        if self.thunk_count >= HEAP_SIZE {
            return Err(VmError::HeapFull);
        }
        // Find first free slot
        for i in 0..HEAP_SIZE {
            if self.thunks[i].is_none() {
                self.thunks[i] = Some(thunk);
                self.thunk_count += 1;
                return Ok(i);
            }
        }
        Err(VmError::HeapFull)
    }
    
    fn get_thunk(&self, idx: usize) -> Option<&ThunkData> {
        self.thunks.get(idx)?.as_ref()
    }
    
    fn get_thunk_mut(&mut self, idx: usize) -> Option<&mut ThunkData> {
        self.thunks.get_mut(idx)?.as_mut()
    }
    
    fn alloc_array_iter(&mut self, array_idx: usize) -> Result<usize, VmError> {
        if self.array_iter_count >= HEAP_SIZE {
            return Err(VmError::HeapFull);
        }
        // Find first free slot
        for i in 0..HEAP_SIZE {
            if self.array_iters[i].is_none() {
                self.array_iters[i] = Some(ArrayIterator {
                    array_idx,
                    current_idx: 0,
                });
                self.array_iter_count += 1;
                return Ok(i);
            }
        }
        Err(VmError::HeapFull)
    }
    
    fn get_array_iter_mut(&mut self, idx: usize) -> Option<&mut ArrayIterator> {
        self.array_iters.get_mut(idx)?.as_mut()
    }
}

/// Dynamic storage for desktop systems (uses heap allocation).
/// 
/// Uses Vec and HashMap for dynamic growth. Only available with std feature.
#[cfg(feature = "std")]
pub struct DynamicStorage {
    stack: std::vec::Vec<Value>,
    strings: std::vec::Vec<std::string::String>,
    arrays: std::vec::Vec<std::vec::Vec<Value>>,
    structs: std::vec::Vec<StructData>,
    thunks: std::vec::Vec<ThunkData>,
    array_iters: std::vec::Vec<ArrayIterator>,
}

#[cfg(feature = "std")]
impl DynamicStorage {
    pub fn new() -> Self {
        Self {
            stack: std::vec::Vec::new(),
            strings: std::vec::Vec::new(),
            arrays: std::vec::Vec::new(),
            structs: std::vec::Vec::new(),
            thunks: std::vec::Vec::new(),
            array_iters: std::vec::Vec::new(),
        }
    }
}

#[cfg(feature = "std")]
impl VmStorage for DynamicStorage {
    fn push(&mut self, v: Value) -> Result<(), VmError> {
        self.stack.push(v);
        Ok(())
    }
    
    fn pop(&mut self) -> Option<Value> {
        self.stack.pop()
    }
    
    fn peek(&self) -> Option<Value> {
        self.stack.last().copied()
    }
    
    fn stack_depth(&self) -> usize {
        self.stack.len()
    }
    
    fn alloc_string(&mut self, s: std::string::String) -> Result<usize, VmError> {
        let idx = self.strings.len();
        self.strings.push(s);
        Ok(idx)
    }
    
    fn get_string(&self, idx: usize) -> Option<&str> {
        self.strings.get(idx).map(|s| s.as_str())
    }
    
    fn alloc_array(&mut self, elements: &[Value]) -> Result<usize, VmError> {
        let idx = self.arrays.len();
        self.arrays.push(elements.to_vec());
        Ok(idx)
    }
    
    fn get_array(&self, idx: usize) -> Option<&[Value]> {
        self.arrays.get(idx).map(|v| v.as_slice())
    }
    
    fn get_array_mut(&mut self, idx: usize) -> Option<&mut [Value]> {
        self.arrays.get_mut(idx).map(|v| v.as_mut_slice())
    }
    
    fn alloc_struct(&mut self, type_id: u32, fields: &[Value]) -> Result<usize, VmError> {
        let idx = self.structs.len();
        self.structs.push(StructData {
            type_id,
            #[cfg(feature = "std")]
            fields: fields.to_vec(),
        });
        Ok(idx)
    }
    
    fn get_struct(&self, idx: usize) -> Option<&StructData> {
        self.structs.get(idx)
    }
    
    fn alloc_thunk(&mut self, thunk: ThunkData) -> Result<usize, VmError> {
        let idx = self.thunks.len();
        self.thunks.push(thunk);
        Ok(idx)
    }
    
    fn get_thunk(&self, idx: usize) -> Option<&ThunkData> {
        self.thunks.get(idx)
    }
    
    fn get_thunk_mut(&mut self, idx: usize) -> Option<&mut ThunkData> {
        self.thunks.get_mut(idx)
    }
    
    fn alloc_array_iter(&mut self, array_idx: usize) -> Result<usize, VmError> {
        let idx = self.array_iters.len();
        self.array_iters.push(ArrayIterator {
            array_idx,
            current_idx: 0,
        });
        Ok(idx)
    }
    
    fn get_array_iter_mut(&mut self, idx: usize) -> Option<&mut ArrayIterator> {
        self.array_iters.get_mut(idx)
    }
}

