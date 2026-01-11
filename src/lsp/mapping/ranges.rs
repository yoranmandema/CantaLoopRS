//! Range conversion utilities.
//!
//! This module provides additional utilities for working with LSP ranges.

use tower_lsp::lsp_types::{Position, Range};

/// Check if a position is within a range.
pub fn position_in_range(position: &Position, range: &Range) -> bool {
    position >= &range.start && position <= &range.end
}
