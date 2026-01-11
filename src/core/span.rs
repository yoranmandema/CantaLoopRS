//! Canonical span type and conversion adapters.
//!
//! This module defines the canonical internal span representation and provides
//! explicit adapters for converting between different span representations used
//! throughout the compiler.
//!
//! ## Span Semantics
//!
//! **Canonical Span**: Byte-based offsets (usize) representing source code ranges.
//! This is the single source of truth for all span-based operations in the LSP
//! and semantic analysis layers.
//!
//! ## Conversion Paths
//!
//! ```
//! CST Span (u32) → Canonical Span → HIR Span (usize) → LSP Range (line/col)
//!      ↓              ↓                    ↓
//!   (pest)     (canonical)          (semantic index)
//! ```
//!
//! All conversions are explicit and documented to prevent subtle bugs when:
//! - Multi-file symbols enter the picture
//! - Generated spans are introduced
//! - Desugared constructs need span tracking

use crate::core::cst::Span as CstSpan;
use crate::core::hir_lowering::Span as HirSpan;

/// Canonical span representation.
///
/// This is the byte-based span type used throughout the LSP and semantic layers.
/// All other span types should be converted to this for cross-layer communication.
///
/// **Invariant**: `start <= end` and both are byte offsets in the source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CanonicalSpan {
    /// Start byte offset (inclusive).
    pub start: usize,
    /// End byte offset (exclusive).
    pub end: usize,
}

impl CanonicalSpan {
    /// Create a new canonical span.
    ///
    /// # Panics
    /// Panics if `start > end` (invalid span).
    pub fn new(start: usize, end: usize) -> Self {
        assert!(start <= end, "Invalid span: start ({}) > end ({})", start, end);
        Self { start, end }
    }

    /// Get the length of the span in bytes.
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    /// Check if the span is empty (zero length).
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// Check if a byte offset is contained within this span.
    pub fn contains(&self, offset: usize) -> bool {
        self.start <= offset && offset < self.end
    }

    /// Check if a byte offset is at or before the start of this span.
    pub fn contains_inclusive(&self, offset: usize) -> bool {
        self.start <= offset && offset <= self.end
    }
}

// ============================================================================
// Explicit Adapters
// ============================================================================

/// Convert CST span (u32 from pest parser) to canonical span.
///
/// **Note**: Pest parser returns u32 offsets. This conversion is safe because:
/// - Source files are limited to reasonable sizes
/// - u32 is sufficient for all practical source files
impl From<CstSpan> for CanonicalSpan {
    fn from(span: CstSpan) -> Self {
        Self::new(span.start as usize, span.end as usize)
    }
}

/// Convert canonical span to CST span.
///
/// **Warning**: May panic if span exceeds u32::MAX (unlikely in practice).
impl From<CanonicalSpan> for CstSpan {
    fn from(span: CanonicalSpan) -> Self {
        Self::new(
            span.start.try_into().expect("Span exceeds u32::MAX"),
            span.end.try_into().expect("Span exceeds u32::MAX"),
        )
    }
}

/// HIR spans are already usize-based, so they're equivalent to canonical spans.
/// This conversion is trivial but explicit for clarity.
impl From<HirSpan> for CanonicalSpan {
    fn from(span: HirSpan) -> Self {
        Self::new(span.start, span.end)
    }
}

/// Convert canonical span to HIR span.
impl From<CanonicalSpan> for HirSpan {
    fn from(span: CanonicalSpan) -> Self {
        Self::new(span.start, span.end)
    }
}

/// Convert between CST and HIR spans (via canonical span for explicitness).
impl From<CstSpan> for HirSpan {
    fn from(span: CstSpan) -> Self {
        CanonicalSpan::from(span).into()
    }
}

/// Convert between HIR and CST spans (via canonical span for explicitness).
///
/// **Warning**: May panic if span exceeds u32::MAX (unlikely in practice).
impl From<HirSpan> for CstSpan {
    fn from(span: HirSpan) -> Self {
        CanonicalSpan::from(span).into()
    }
}

// ============================================================================
// LSP Range Conversion
// ============================================================================
// Conversion to LSP ranges is handled by LineIndex in lsp/mapping/spans.rs
// to avoid circular dependencies. That module uses CanonicalSpan (via HirSpan)
// as the source of truth.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canonical_span_basic() {
        let span = CanonicalSpan::new(10, 20);
        assert_eq!(span.start, 10);
        assert_eq!(span.end, 20);
        assert_eq!(span.len(), 10);
        assert!(!span.is_empty());
    }

    #[test]
    fn test_canonical_span_empty() {
        let span = CanonicalSpan::new(10, 10);
        assert!(span.is_empty());
        assert_eq!(span.len(), 0);
    }

    #[test]
    fn test_canonical_span_contains() {
        let span = CanonicalSpan::new(10, 20);
        assert!(!span.contains(9));
        assert!(span.contains(10));
        assert!(span.contains(15));
        assert!(!span.contains(20)); // end is exclusive
        assert!(span.contains_inclusive(20)); // but inclusive check includes end
    }

    #[test]
    fn test_cst_to_canonical() {
        let cst_span = CstSpan::new(5, 10);
        let canonical: CanonicalSpan = cst_span.into();
        assert_eq!(canonical.start, 5);
        assert_eq!(canonical.end, 10);
    }

    #[test]
    fn test_hir_to_canonical() {
        let hir_span = HirSpan::new(15, 25);
        let canonical: CanonicalSpan = hir_span.into();
        assert_eq!(canonical.start, 15);
        assert_eq!(canonical.end, 25);
    }

    #[test]
    fn test_cst_to_hir() {
        let cst_span = CstSpan::new(7, 12);
        let hir_span: HirSpan = cst_span.into();
        assert_eq!(hir_span.start, 7);
        assert_eq!(hir_span.end, 12);
    }
}
