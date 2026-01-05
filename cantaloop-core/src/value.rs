use crate::storage::VmStorage;

// Type tags for NaN values (using quiet NaN with specific payload)
// QNAN has bits 0x7FF8 in the exponent, we use bits 48-51 for the tag
// Note: Bit 51 must be set for quiet NaN, so tags use bits 48-50, with bit 51 always set
const PAYLOAD_MASK: u64 = 0x0000_FFFF_FFFF_FFFF; // Bits 0-47 for payload (exclude tag bits 48-51)
const QNAN_BASE: u64 = 0x7FF8_0000_0000_0000; // Quiet NaN base (exponent=0x7FF, bit 51 set)
const TAG_MASK: u64 = 0xF << 48; // Bits 48-51 for tag
const TAG_CLEAR_MASK: u64 = !TAG_MASK; // Mask to clear tag bits
const QNAN_BIT_51: u64 = 1 << 51; // Bit 51 must be set for quiet NaN

const TAG_STRING: u64 = 0x1;
const TAG_BOOLEAN: u64 = 0x2;
const TAG_FUNCTION: u64 = 0x3;
const TAG_THUNK: u64 = 0x4;
const TAG_NONE: u64 = 0x5;
const TAG_ARRAY: u64 = 0x6;
const TAG_ARRAY_ITER: u64 = 0x7;
const TAG_STRUCT: u64 = 0x8;

/// Tagged union Value using NaN boxing
/// Numbers: valid f64 (not NaN)
/// Other types: NaN with tag bits in payload
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Value {
    raw: u64,
}

/// Array iterator state
pub struct ArrayIterator {
    pub array_idx: usize,
    pub current_idx: usize,
}

/// Struct instance data stored in the heap
#[cfg(feature = "std")]
pub struct StructData {
    pub type_id: u32, // Struct type ID (index into struct definitions)
    pub fields: std::vec::Vec<Value>, // Field values in order
}

/// Fixed-size struct data for no_std (max 16 fields)
#[cfg(not(feature = "std"))]
pub struct StructData {
    pub type_id: u32,
    pub fields: FixedFieldArray,
}

/// Fixed-size field storage (max 16 fields per struct)
#[cfg(not(feature = "std"))]
pub struct FixedFieldArray {
    data: [Option<Value>; 16],
    len: usize,
}

#[cfg(not(feature = "std"))]
impl FixedFieldArray {
    pub fn new() -> Self {
        Self {
            data: [None; 16],
            len: 0,
        }
    }
    
    pub fn from_slice(fields: &[Value]) -> Result<Self, crate::error::VmError> {
        if fields.len() > 16 {
            return Err(crate::error::VmError::HeapFull);
        }
        let mut arr = Self::new();
        for (i, &field) in fields.iter().enumerate() {
            arr.data[i] = Some(field);
        }
        arr.len = fields.len();
        Ok(arr)
    }
    
    pub fn len(&self) -> usize {
        self.len
    }
    
    pub fn get(&self, idx: usize) -> Option<Value> {
        if idx < self.len {
            self.data[idx]
        } else {
            None
        }
    }
    
    pub fn iter(&self) -> impl Iterator<Item = Value> + '_ {
        self.data[..self.len].iter().filter_map(|&v| v)
    }
}

/// Thunk data stored in the heap
#[cfg(feature = "std")]
pub enum ThunkData {
    Regular {
        func_id: u32,
        bound: std::vec::Vec<Option<Value>>, // None = hole, Some(value) = bound argument
    },
    Composed {
        first: Value,  // First thunk (f)
        second: Value, // Second thunk (g) - composition is g(f(x))
    },
}

/// Fixed-size thunk data for no_std (max 8 bound arguments)
#[cfg(not(feature = "std"))]
pub enum ThunkData {
    Regular {
        func_id: u32,
        bound: FixedBoundArray, // None = hole, Some(value) = bound argument
    },
    Composed {
        first: Value,  // First thunk (f)
        second: Value, // Second thunk (g) - composition is g(f(x))
    },
}

/// Fixed-size bound storage (max 8 bound arguments per thunk)
#[cfg(not(feature = "std"))]
pub struct FixedBoundArray {
    data: [Option<Value>; 8],
    len: usize,
}

#[cfg(not(feature = "std"))]
impl FixedBoundArray {
    pub fn new() -> Self {
        Self {
            data: [None; 8],
            len: 0,
        }
    }
    
    pub fn from_slice(bound: &[Option<Value>]) -> Result<Self, crate::error::VmError> {
        if bound.len() > 8 {
            return Err(crate::error::VmError::HeapFull);
        }
        let mut arr = Self::new();
        for (i, &val) in bound.iter().enumerate() {
            arr.data[i] = val;
        }
        arr.len = bound.len();
        Ok(arr)
    }
    
    pub fn len(&self) -> usize {
        self.len
    }
    
    pub fn get(&self, idx: usize) -> Option<Option<Value>> {
        if idx < self.len {
            Some(self.data[idx])
        } else {
            None
        }
    }
    
    pub fn get_mut(&mut self, idx: usize) -> Option<&mut Option<Value>> {
        if idx < self.len {
            Some(&mut self.data[idx])
        } else {
            None
        }
    }
    
    pub fn iter(&self) -> impl Iterator<Item = Option<Value>> + '_ {
        self.data[..self.len].iter().copied()
    }
}

// Helper methods for creating ThunkData
impl ThunkData {
    /// Create a regular thunk with bound arguments
    #[cfg(feature = "std")]
    pub fn regular(func_id: u32, bound: std::vec::Vec<Option<Value>>) -> Self {
        Self::Regular { func_id, bound }
    }
    
    #[cfg(not(feature = "std"))]
    pub fn regular(func_id: u32, bound: &[Option<Value>]) -> Result<Self, crate::error::VmError> {
        Ok(Self::Regular {
            func_id,
            bound: FixedBoundArray::from_slice(bound)?,
        })
    }
    
    /// Create a composed thunk
    pub fn composed(first: Value, second: Value) -> Self {
        Self::Composed { first, second }
    }
    
    /// Get the function ID if this is a regular thunk
    pub fn func_id(&self) -> Option<u32> {
        match self {
            #[cfg(feature = "std")]
            Self::Regular { func_id, .. } => Some(*func_id),
            #[cfg(not(feature = "std"))]
            Self::Regular { func_id, .. } => Some(*func_id),
            Self::Composed { .. } => None,
        }
    }
    
    /// Get bound arguments as a slice (for std) or iterator (for no_std)
    #[cfg(feature = "std")]
    pub fn bound(&self) -> Option<&[Option<Value>]> {
        match self {
            Self::Regular { bound, .. } => Some(bound.as_slice()),
            Self::Composed { .. } => None,
        }
    }
    
    #[cfg(not(feature = "std"))]
    pub fn bound_iter(&self) -> Option<impl Iterator<Item = Option<Value>> + '_> {
        match self {
            Self::Regular { bound, .. } => Some(bound.iter()),
            Self::Composed { .. } => None,
        }
    }
}

// Helper methods for accessing StructData fields
impl StructData {
    /// Get the number of fields
    pub fn field_count(&self) -> usize {
        #[cfg(feature = "std")]
        {
            self.fields.len()
        }
        #[cfg(not(feature = "std"))]
        {
            self.fields.len()
        }
    }
    
    /// Get a field by index
    pub fn get_field(&self, idx: usize) -> Option<Value> {
        #[cfg(feature = "std")]
        {
            self.fields.get(idx).copied()
        }
        #[cfg(not(feature = "std"))]
        {
            self.fields.get(idx)
        }
    }
    
    /// Iterate over fields
    #[cfg(feature = "std")]
    pub fn fields(&self) -> impl Iterator<Item = Value> + '_ {
        self.fields.iter().copied()
    }
    
    #[cfg(not(feature = "std"))]
    pub fn fields(&self) -> impl Iterator<Item = Value> + '_ {
        self.fields.iter()
    }
}

impl Value {
    #[inline(always)]
    pub fn number(n: f64) -> Self {
        // Ensure it's not NaN
        #[cfg(feature = "std")]
        debug_assert!(!n.is_nan(), "Cannot create Value::Number from NaN");
        Self { raw: n.to_bits() }
    }

    #[inline(always)]
    pub fn boolean(b: bool) -> Self {
        Self {
            raw: (QNAN_BASE & TAG_CLEAR_MASK)
                | (TAG_BOOLEAN << 48)
                | QNAN_BIT_51
                | (if b { 1 } else { 0 }),
        }
    }

    #[inline(always)]
    pub fn function(id: u32) -> Self {
        Self {
            raw: (QNAN_BASE & TAG_CLEAR_MASK) | (TAG_FUNCTION << 48) | QNAN_BIT_51 | (id as u64),
        }
    }

    #[inline(always)]
    pub fn none() -> Self {
        Self {
            raw: (QNAN_BASE & TAG_CLEAR_MASK) | (TAG_NONE << 48) | QNAN_BIT_51,
        }
    }

    /// Create a string value using storage
    #[cfg(feature = "std")]
    pub fn string_with_storage<S: VmStorage>(s: std::string::String, storage: &mut S) -> Result<Self, crate::error::VmError> {
        let idx = storage.alloc_string(s)?;
        Ok(Self {
            raw: (QNAN_BASE & TAG_CLEAR_MASK) | (TAG_STRING << 48) | QNAN_BIT_51 | (idx as u64),
        })
    }

    /// Create an array value using storage
    pub fn array_with_storage<S: VmStorage>(elements: &[Value], storage: &mut S) -> Result<Self, crate::error::VmError> {
        let idx = storage.alloc_array(elements)?;
        Ok(Self {
            raw: (QNAN_BASE & TAG_CLEAR_MASK) | (TAG_ARRAY << 48) | QNAN_BIT_51 | (idx as u64),
        })
    }

    /// Create a struct value using storage
    pub fn struct_with_storage<S: VmStorage>(type_id: u32, fields: &[Value], storage: &mut S) -> Result<Self, crate::error::VmError> {
        let idx = storage.alloc_struct(type_id, fields)?;
        Ok(Self {
            raw: (QNAN_BASE & TAG_CLEAR_MASK) | (TAG_STRUCT << 48) | QNAN_BIT_51 | (idx as u64),
        })
    }

    /// Create a thunk value using storage
    pub fn thunk_with_storage<S: VmStorage>(thunk: ThunkData, storage: &mut S) -> Result<Self, crate::error::VmError> {
        let idx = storage.alloc_thunk(thunk)?;
        Ok(Self {
            raw: (QNAN_BASE & TAG_CLEAR_MASK) | (TAG_THUNK << 48) | QNAN_BIT_51 | (idx as u64),
        })
    }

    /// Create an array iterator value using storage
    pub fn array_iter_with_storage<S: VmStorage>(array_idx: usize, storage: &mut S) -> Result<Self, crate::error::VmError> {
        let idx = storage.alloc_array_iter(array_idx)?;
        Ok(Self {
            raw: (QNAN_BASE & TAG_CLEAR_MASK) | (TAG_ARRAY_ITER << 48) | QNAN_BIT_51 | (idx as u64),
        })
    }

    #[inline(always)]
    fn tag(&self) -> u64 {
        // Check if it's a NaN (exponent bits 0x7FF)
        if (self.raw & 0x7FF0_0000_0000_0000) == 0x7FF0_0000_0000_0000 {
            // Extract tag from bits 48-51
            let tag_bits = (self.raw >> 48) & 0xF; // Extract bits 48-51 (4 bits)
            if tag_bits == 0x8 {
                8 // TAG_STRUCT: bit 51 set, bits 48-50 = 0
            } else {
                tag_bits & 0x7 // Tags 0-7: extract bits 48-50 (bit 51 is set but not part of tag value)
            }
        } else {
            0 // Number
        }
    }

    #[inline(always)]
    pub fn as_number(&self) -> Option<f64> {
        if self.tag() == 0 {
            Some(f64::from_bits(self.raw))
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn as_boolean(&self) -> Option<bool> {
        if self.tag() == TAG_BOOLEAN {
            Some((self.raw & 1) != 0)
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn as_function(&self) -> Option<u32> {
        if self.tag() == TAG_FUNCTION {
            Some((self.raw & PAYLOAD_MASK) as u32)
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn is_none(&self) -> bool {
        self.tag() == TAG_NONE
    }

    #[inline(always)]
    pub fn is_thunk(&self) -> bool {
        self.tag() == TAG_THUNK
    }

    #[inline(always)]
    pub fn is_struct(&self) -> bool {
        self.tag() == TAG_STRUCT
    }

    /// Get a string from storage
    #[cfg(feature = "std")]
    pub fn as_string<'a, S: VmStorage>(&self, storage: &'a S) -> Option<&'a str> {
        if self.tag() == TAG_STRING {
            let idx = (self.raw & PAYLOAD_MASK) as usize;
            storage.get_string(idx)
        } else {
            None
        }
    }

    /// Get an array from storage
    pub fn as_array<'a, S: VmStorage>(&self, storage: &'a S) -> Option<&'a [Value]> {
        if self.tag() == TAG_ARRAY {
            let idx = (self.raw & PAYLOAD_MASK) as usize;
            storage.get_array(idx)
        } else {
            None
        }
    }

    /// Get a mutable array from storage
    pub fn as_array_mut<'a, S: VmStorage>(&self, storage: &'a mut S) -> Option<&'a mut [Value]> {
        if self.tag() == TAG_ARRAY {
            let idx = (self.raw & PAYLOAD_MASK) as usize;
            storage.get_array_mut(idx)
        } else {
            None
        }
    }

    /// Get a struct from storage
    #[cfg(feature = "std")]
    pub fn as_struct<'a, S: VmStorage>(&self, storage: &'a S) -> Option<&'a StructData> {
        if self.tag() == TAG_STRUCT {
            let idx = (self.raw & PAYLOAD_MASK) as usize;
            storage.get_struct(idx)
        } else {
            None
        }
    }

    /// Get a thunk from storage
    #[cfg(feature = "std")]
    pub fn as_thunk<'a, S: VmStorage>(&self, storage: &'a S) -> Option<&'a ThunkData> {
        if self.is_thunk() {
            let idx = (self.raw & PAYLOAD_MASK) as usize;
            storage.get_thunk(idx)
        } else {
            None
        }
    }

    /// Get a mutable thunk from storage
    #[cfg(feature = "std")]
    pub fn as_thunk_mut<'a, S: VmStorage>(&self, storage: &'a mut S) -> Option<&'a mut ThunkData> {
        if self.is_thunk() {
            let idx = (self.raw & PAYLOAD_MASK) as usize;
            storage.get_thunk_mut(idx)
        } else {
            None
        }
    }

    /// Get an array iterator from storage
    pub fn as_array_iter_mut<'a, S: VmStorage>(&self, storage: &'a mut S) -> Option<&'a mut ArrayIterator> {
        if self.tag() == TAG_ARRAY_ITER {
            let idx = (self.raw & PAYLOAD_MASK) as usize;
            storage.get_array_iter_mut(idx)
        } else {
            None
        }
    }
}

#[cfg(feature = "std")]
impl core::fmt::Debug for Value {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(n) = self.as_number() {
            write!(f, "Value::Number({})", n)
        } else if let Some(b) = self.as_boolean() {
            write!(f, "Value::Boolean({})", b)
        } else if let Some(id) = self.as_function() {
            write!(f, "Value::Function({})", id)
        } else if self.is_none() {
            write!(f, "Value::None")
        } else {
            write!(f, "Value::Tagged({:x})", self.raw)
        }
    }
}

