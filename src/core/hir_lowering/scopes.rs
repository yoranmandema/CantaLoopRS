//! Scope management for HIR lowering.
//! 
//! Scopes are first-class entities with stable IDs, enabling proper scope resolution,
//! shadowing detection, and closure analysis.

use super::{Variable, SymbolId};
use serde::Serialize;

/// Unique identifier for a scope.
/// Scopes form a tree structure via parent relationships.
/// Each scope contains a set of symbols.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct ScopeId(pub usize);

impl ScopeId {
    pub fn as_usize(self) -> usize {
        self.0
    }
}

impl From<usize> for ScopeId {
    fn from(value: usize) -> Self {
        ScopeId(value)
    }
}

impl From<ScopeId> for usize {
    fn from(value: ScopeId) -> Self {
        value.0
    }
}

// For backward compatibility during migration
pub type ScopeIdOld = usize;

/// Scope arena containing all scopes in the program.
#[derive(Debug, Clone, Serialize)]
pub struct ScopeArena {
    pub scopes: Vec<HirBlockContext>,
}

impl Default for ScopeArena {
    fn default() -> Self {
        Self { scopes: Vec::new() }
    }
}

/// Context for a HIR block, containing variables and parent scope.
#[derive(Debug, Clone, Serialize)]
pub struct HirBlockContext {
    pub vars: Vec<Variable>,
    pub parent: Option<ScopeId>,
}

/// Formal scope entity.
/// 
/// Scopes are now first-class entities with stable IDs.
/// This enables proper scope resolution, shadowing detection, and closure analysis.
#[derive(Debug, Clone, Serialize)]
pub struct Scope {
    pub id: ScopeId,
    pub parent: Option<ScopeId>,
    pub symbols: Vec<SymbolId>,
}

