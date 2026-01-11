//! Symbol occurrence tracking for LSP semantic indexing.
//!
//! This module tracks symbol occurrences (definitions and references) during HIR lowering
//! to enable accurate span-to-symbol mapping for the LSP.

use crate::core::hir_lowering::SymbolId;
use crate::core::hir_lowering::Span as HirSpan;

/// Role of a symbol occurrence in the source code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolRole {
    /// Symbol definition (e.g., `let x = ...`, `fn f(...)`)
    Definition,
    /// Symbol read/reference (e.g., `x` in an expression)
    Read,
    /// Function call (e.g., `f()`)
    Call,
    /// Type reference (e.g., `State` in `State { ... }`)
    Type,
    /// Field access (e.g., `.field` in `obj.field`)
    FieldAccess,
}

/// A single occurrence of a symbol in the source code.
#[derive(Debug, Clone)]
pub struct SymbolOccurrence {
    /// The symbol ID this occurrence refers to
    pub symbol_id: SymbolId,
    /// Source code span of this occurrence
    pub span: HirSpan,
    /// Role of this occurrence
    pub role: SymbolRole,
}

/// Collection of symbol occurrences for semantic indexing.
#[derive(Debug, Clone, Default)]
pub struct SymbolOccurrences {
    occurrences: Vec<SymbolOccurrence>,
}

impl SymbolOccurrences {
    pub fn new() -> Self {
        Self {
            occurrences: Vec::new(),
        }
    }

    /// Record a symbol occurrence.
    pub fn record(&mut self, symbol_id: SymbolId, span: HirSpan, role: SymbolRole) {
        self.occurrences.push(SymbolOccurrence {
            symbol_id,
            span,
            role,
        });
    }

    /// Get all occurrences as a slice.
    pub fn as_slice(&self) -> &[SymbolOccurrence] {
        &self.occurrences
    }

    /// Build a span-to-symbol mapping from occurrences.
    pub fn build_span_to_symbol_map(&self) -> std::collections::HashMap<HirSpan, SymbolId> {
        let mut map = std::collections::HashMap::new();
        for occurrence in &self.occurrences {
            map.insert(occurrence.span, occurrence.symbol_id);
        }
        map
    }
}
