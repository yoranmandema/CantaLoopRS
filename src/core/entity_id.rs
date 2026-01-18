//! Unified entity identification system for the CantaLoop compiler.
//!
//! All semantic entities (functions, variables, types, modules) share a single
//! ID space with reserved ranges. This eliminates the need for name-based lookups
//! and makes IDs stable across compilation phases (CST→AST→HIR→Bytecode).

use serde::Serialize;
use std::fmt;

/// Unified identifier for all compiler entities.
///
/// EntityId is the single source of identity throughout the compilation pipeline.
/// Once an entity (function, variable, type, etc.) is assigned an ID, that ID
/// remains stable across all phases.
///
/// # ID Ranges
/// - `0-9999`: User-defined entities (functions, variables, constants)
/// - `10000-19999`: Native functions (std.*, functional.*, etc.)
/// - `20000-29999`: Native types and modules
/// - `30000-99999`: Reserved for future use
/// - `100000+`: Temporary/synthetic IDs (anonymous closures, compiler-generated)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, PartialOrd, Ord)]
pub struct EntityId(pub u32);

impl EntityId {
    /// Create a new EntityId.
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// Get the underlying ID value.
    pub const fn as_u32(self) -> u32 {
        self.0
    }

    /// Check if this is a user-defined entity.
    pub const fn is_user_defined(self) -> bool {
        self.0 < 10000
    }

    /// Check if this is a native function.
    pub const fn is_native_function(self) -> bool {
        self.0 >= 10000 && self.0 < 20000
    }

    /// Check if this is a native type or module.
    pub const fn is_native_type_or_module(self) -> bool {
        self.0 >= 20000 && self.0 < 30000
    }

    /// Check if this is a synthetic/temporary ID.
    pub const fn is_synthetic(self) -> bool {
        self.0 >= 100000
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "EntityId({})", self.0)
    }
}

/// ID range constants for different entity types.
pub mod id_ranges {
    use super::EntityId;

    // User-defined entities: 0-9999
    pub const USER_START: EntityId = EntityId::new(0);
    pub const USER_END: EntityId = EntityId::new(9999);

    // Native functions: 10000-19999
    pub const NATIVE_FUNC_START: EntityId = EntityId::new(10000);
    pub const NATIVE_FUNC_END: EntityId = EntityId::new(19999);

    // Native types and modules: 20000-29999
    pub const NATIVE_TYPE_START: EntityId = EntityId::new(20000);
    pub const NATIVE_TYPE_END: EntityId = EntityId::new(29999);

    // Synthetic/temporary: 100000+
    pub const SYNTHETIC_START: EntityId = EntityId::new(100000);
}

/// Generator for EntityIds.
///
/// Maintains separate counters for different ID ranges to ensure
/// IDs are allocated from the correct range.
#[derive(Debug, Clone)]
pub struct EntityIdGenerator {
    /// Next ID for user-defined entities (0-9999)
    next_user_id: u32,
    /// Next ID for native functions (10000-19999)
    next_native_func_id: u32,
    /// Next ID for native types/modules (20000-29999)
    next_native_type_id: u32,
    /// Next ID for synthetic entities (100000+)
    next_synthetic_id: u32,
}

impl EntityIdGenerator {
    /// Create a new ID generator.
    pub fn new() -> Self {
        Self {
            next_user_id: 0,
            next_native_func_id: 10000,
            next_native_type_id: 20000,
            next_synthetic_id: 100000,
        }
    }

    /// Generate the next ID for a user-defined entity.
    pub fn next_user(&mut self) -> EntityId {
        let id = self.next_user_id;
        assert!(id <= 9999, "User ID range exhausted");
        self.next_user_id += 1;
        EntityId::new(id)
    }

    /// Generate the next ID for a native function.
    pub fn next_native_func(&mut self) -> EntityId {
        let id = self.next_native_func_id;
        assert!(id < 20000, "Native function ID range exhausted");
        self.next_native_func_id += 1;
        EntityId::new(id)
    }

    /// Generate the next ID for a native type or module.
    pub fn next_native_type(&mut self) -> EntityId {
        let id = self.next_native_type_id;
        assert!(id < 30000, "Native type ID range exhausted");
        self.next_native_type_id += 1;
        EntityId::new(id)
    }

    /// Generate the next ID for a synthetic/temporary entity.
    pub fn next_synthetic(&mut self) -> EntityId {
        let id = self.next_synthetic_id;
        self.next_synthetic_id += 1;
        EntityId::new(id)
    }

    /// Allocate a specific ID (for testing or pre-assigned IDs).
    /// Panics if the ID is already allocated.
    pub fn allocate_specific(&mut self, id: EntityId) -> EntityId {
        let id_val = id.as_u32();
        if id_val < 10000 {
            assert!(id_val >= self.next_user_id, "ID already allocated");
            self.next_user_id = id_val + 1;
        } else if id_val < 20000 {
            assert!(id_val >= self.next_native_func_id, "ID already allocated");
            self.next_native_func_id = id_val + 1;
        } else if id_val < 30000 {
            assert!(id_val >= self.next_native_type_id, "ID already allocated");
            self.next_native_type_id = id_val + 1;
        } else if id_val >= 100000 {
            assert!(id_val >= self.next_synthetic_id, "ID already allocated");
            self.next_synthetic_id = id_val + 1;
        } else {
            panic!("Cannot allocate ID in reserved range: {}", id_val);
        }
        id
    }

    /// Reset all counters (for testing).
    pub fn reset(&mut self) {
        self.next_user_id = 0;
        self.next_native_func_id = 10000;
        self.next_native_type_id = 20000;
        self.next_synthetic_id = 100000;
    }
}

impl Default for EntityIdGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_id_ranges() {
        let user_id = EntityId::new(42);
        assert!(user_id.is_user_defined());
        assert!(!user_id.is_native_function());

        let native_func = EntityId::new(10500);
        assert!(native_func.is_native_function());
        assert!(!native_func.is_user_defined());

        let native_type = EntityId::new(20001);
        assert!(native_type.is_native_type_or_module());

        let synthetic = EntityId::new(100042);
        assert!(synthetic.is_synthetic());
    }

    #[test]
    fn test_generator() {
        let mut gen = EntityIdGenerator::new();

        let id1 = gen.next_user();
        let id2 = gen.next_user();
        assert_eq!(id1.as_u32(), 0);
        assert_eq!(id2.as_u32(), 1);

        let native1 = gen.next_native_func();
        assert_eq!(native1.as_u32(), 10000);

        let type1 = gen.next_native_type();
        assert_eq!(type1.as_u32(), 20000);

        let synth1 = gen.next_synthetic();
        assert_eq!(synth1.as_u32(), 100000);
    }
}
