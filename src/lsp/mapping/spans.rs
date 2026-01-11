//! Span conversion utilities.

use crate::core::cst::Span as CstSpan;
use crate::core::hir_lowering::Span as HirSpan;
use tower_lsp::lsp_types::{Position, Range};

/// Convert CST span (u32) to HIR span (usize).
pub fn cst_to_hir(cst_span: CstSpan) -> HirSpan {
    HirSpan::new(cst_span.start as usize, cst_span.end as usize)
}

/// Convert HIR span (usize) to CST span (u32).
pub fn hir_to_cst(hir_span: HirSpan) -> CstSpan {
    CstSpan::new(hir_span.start as u32, hir_span.end as u32)
}

/// Build a line index from source text for efficient byte-to-line conversion.
pub struct LineIndex {
    line_starts: Vec<usize>,
}

impl LineIndex {
    pub fn new(source: &str) -> Self {
        let mut line_starts = vec![0];
        for (byte_pos, ch) in source.char_indices() {
            if ch == '\n' {
                line_starts.push(byte_pos + 1);
            }
        }
        Self { line_starts }
    }

    /// Convert byte offset to (line, column) tuple.
    pub fn byte_to_line_col(&self, byte: usize) -> (u32, u32) {
        let line = self.line_starts.partition_point(|&x| x <= byte) - 1;
        let col = byte - self.line_starts.get(line).copied().unwrap_or(0);
        (line as u32, col as u32)
    }

    /// Convert (line, column) to byte offset.
    pub fn line_col_to_byte(&self, line: u32, col: u32) -> usize {
        let line_start = self.line_starts.get(line as usize).copied().unwrap_or(0);
        line_start + col as usize
    }

    /// Convert HIR span to LSP range.
    pub fn hir_span_to_range(&self, span: HirSpan) -> Range {
        let (start_line, start_col) = self.byte_to_line_col(span.start);
        let (end_line, end_col) = self.byte_to_line_col(span.end);

        Range {
            start: Position {
                line: start_line,
                character: start_col,
            },
            end: Position {
                line: end_line,
                character: end_col,
            },
        }
    }

    /// Convert LSP range to HIR span.
    pub fn range_to_hir_span(&self, range: Range) -> HirSpan {
        let start = self.line_col_to_byte(range.start.line, range.start.character);
        let end = self.line_col_to_byte(range.end.line, range.end.character);
        HirSpan::new(start, end)
    }
}

/// Convert HIR span to LSP range using source text.
pub fn hir_span_to_range(span: HirSpan, source: &str) -> Option<Range> {
    let index = LineIndex::new(source);
    Some(index.hir_span_to_range(span))
}

/// Convert LSP position to byte offset.
pub fn position_to_byte_offset(position: Position, source: &str) -> usize {
    let index = LineIndex::new(source);
    index.line_col_to_byte(position.line, position.character)
}
