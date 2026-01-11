//! CST node identity for tracking nodes through lowering.
//!
//! Each CST node gets a unique ID that persists through AST→HIR lowering,
//! enabling accurate span→symbol binding for the LSP.

use serde::Serialize;

/// Unique identifier for a CST node.
///
/// This ID is assigned during CST building and persists through the entire
/// compilation pipeline, allowing us to track which HIR symbols correspond
/// to which CST nodes (and thus which source spans).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct CstId(pub u32);

impl CstId {
    /// Create a new CST ID.
    pub fn new(id: u32) -> Self {
        Self(id)
    }

    /// Get the underlying ID value.
    pub fn as_u32(self) -> u32 {
        self.0
    }
}

/// Generator for unique CST node IDs.
#[derive(Debug, Clone)]
pub struct CstIdGenerator {
    next_id: u32,
}

impl CstIdGenerator {
    /// Create a new ID generator starting at 0.
    pub fn new() -> Self {
        Self { next_id: 0 }
    }

    /// Generate the next unique CST ID.
    pub fn next(&mut self) -> CstId {
        let id = self.next_id;
        self.next_id += 1;
        CstId::new(id)
    }

    /// Reset the generator (for testing or reuse).
    pub fn reset(&mut self) {
        self.next_id = 0;
    }
}

impl Default for CstIdGenerator {
    fn default() -> Self {
        Self::new()
    }
}
