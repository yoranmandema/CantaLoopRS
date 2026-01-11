use serde::Serialize;

/// Byte-based source span for LSP integration.
/// 
/// Uses u32 to match LSP's position representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    pub fn length(&self) -> u32 {
        self.end - self.start
    }

    /// Create a span from a pest::Span (converting usize to u32)
    pub fn from_pest_span(pest_span: pest::Span) -> Self {
        Self {
            start: pest_span.start() as u32,
            end: pest_span.end() as u32,
        }
    }

    /// Create a span from usize values (converting to u32)
    pub fn from_usize(start: usize, end: usize) -> Self {
        Self {
            start: start as u32,
            end: end as u32,
        }
    }

    /// Merge two spans (from start of first to end of second)
    pub fn merge(self, other: Span) -> Self {
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

use super::node_id::CstId;

/// Wrapper that attaches a span and unique ID to any node.
/// 
/// Parsing produces Spanned<T>; semantic phases mostly consume T.
/// The ID allows tracking nodes through lowering for LSP symbol binding.
#[derive(Debug, Clone, Serialize)]
pub struct Spanned<T> {
    /// Unique identifier for this CST node (persists through lowering)
    pub id: CstId,
    /// Source code span
    pub span: Span,
    /// The node data
    pub node: T,
}

impl<T> Spanned<T> {
    /// Create a new Spanned node with ID, span, and node data.
    pub fn new(id: CstId, span: Span, node: T) -> Self {
        Self { id, span, node }
    }

    /// Get the CST node ID
    pub fn id(&self) -> CstId {
        self.id
    }

    /// Extract the inner node, discarding the span
    pub fn into_node(self) -> T {
        self.node
    }

    /// Get a reference to the inner node
    pub fn node(&self) -> &T {
        &self.node
    }

    /// Get a mutable reference to the inner node
    pub fn node_mut(&mut self) -> &mut T {
        &mut self.node
    }

    /// Get the span
    pub fn span(&self) -> Span {
        self.span
    }
}

impl<T> std::ops::Deref for Spanned<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.node
    }
}

impl<T> std::ops::DerefMut for Spanned<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.node
    }
}

