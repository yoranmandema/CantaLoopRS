//! Formatting handler.

use tower_lsp::lsp_types::*;

use crate::lsp::server::CantaLoopServer;
use crate::lsp::mapping::spans::LineIndex;

/// Handle textDocument/formatting.
pub async fn handle_formatting(
    server: &CantaLoopServer,
    params: DocumentFormattingParams,
) -> tower_lsp::jsonrpc::Result<Option<Vec<TextEdit>>> {
    let uri = &params.text_document.uri;

    // Get source text and file ID
    let (_file_id, source_text) = {
        let source_manager = server.source_manager.read().await;
        let file_id = match source_manager.get_file_id(uri) {
            Some(id) => id,
            None => return Ok(None),
        };
        let text = source_manager.get_file_text(file_id)
            .unwrap_or("")
            .to_string();
        (file_id, text)
    };

    // Basic formatting: normalize whitespace and indentation
    // This is a simple implementation - can be enhanced later
    let formatted = format_code(&source_text, &params.options);

    if formatted == source_text {
        return Ok(Some(vec![]));
    }

    let line_index = LineIndex::new(&source_text);
    let (end_line, end_col) = line_index.byte_to_line_col(source_text.len());
    let range = Range {
        start: Position { line: 0, character: 0 },
        end: Position { line: end_line, character: end_col },
    };

    Ok(Some(vec![TextEdit {
        range,
        new_text: formatted,
    }]))
}

/// Handle textDocument/rangeFormatting.
pub async fn handle_range_formatting(
    server: &CantaLoopServer,
    params: DocumentRangeFormattingParams,
) -> tower_lsp::jsonrpc::Result<Option<Vec<TextEdit>>> {
    let uri = &params.text_document.uri;

    // Get source text and file ID
    let (_file_id, source_text) = {
        let source_manager = server.source_manager.read().await;
        let file_id = match source_manager.get_file_id(uri) {
            Some(id) => id,
            None => return Ok(None),
        };
        let text = source_manager.get_file_text(file_id)
            .unwrap_or("")
            .to_string();
        (file_id, text)
    };

    let line_index = LineIndex::new(&source_text);
    let range = params.range;

    // Extract the range to format
    let start_offset = line_index.line_col_to_byte(range.start.line, range.start.character);
    let end_offset = line_index.line_col_to_byte(range.end.line, range.end.character);
    
    if start_offset >= end_offset || end_offset > source_text.len() {
        return Ok(Some(vec![]));
    }

    let range_text = &source_text[start_offset..end_offset];
    let formatted = format_code(range_text, &params.options);

    if formatted == range_text {
        return Ok(Some(vec![]));
    }

    Ok(Some(vec![TextEdit {
        range,
        new_text: formatted,
    }]))
}

/// Basic code formatter.
/// 
/// This is a simple implementation that:
/// - Normalizes line endings
/// - Ensures consistent indentation (4 spaces)
/// - Adds/removes trailing whitespace
/// 
/// A more sophisticated formatter could be added later.
fn format_code(source: &str, _options: &FormattingOptions) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut result = Vec::new();
    let mut indent_level = 0;
    let indent_size = 4;

    for line in lines {
        let trimmed = line.trim_end();
        
        // Decrease indent for closing braces/brackets
        if trimmed.ends_with('}') || trimmed.ends_with(']') {
            if indent_level > 0 {
                indent_level -= 1;
            }
        }

        // Add indented line
        if !trimmed.is_empty() {
            let indent = " ".repeat(indent_level * indent_size);
            result.push(format!("{}{}", indent, trimmed));
        } else {
            result.push(String::new());
        }

        // Increase indent for opening braces/brackets
        if trimmed.ends_with('{') || trimmed.ends_with('[') {
            indent_level += 1;
        }
    }

    result.join("\n")
}
