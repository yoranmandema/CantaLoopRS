use tower_lsp::lsp_types::{Position, Range};

/// Convert a byte position in text to (line, column) coordinates.
pub fn byte_position_to_line_col(text: &str, pos: usize) -> (usize, usize) {
    let text_before = &text[..pos];
    let line = text_before.matches('\n').count();
    let col = text_before
        .rfind('\n')
        .map(|last_nl| pos - last_nl - 1)
        .unwrap_or(pos);
    (line, col)
}

/// Extract identifier at a given position in text.
/// Returns (identifier, start_col, end_col) if found.
pub fn extract_identifier_at_position(text: &str, line: usize, col: usize) -> Option<(String, usize, usize)> {
    let lines: Vec<&str> = text.lines().collect();
    if line >= lines.len() {
        return None;
    }

    let line_text = lines[line];
    if col >= line_text.len() {
        return None;
    }

    // Find start of identifier
    let mut start = col;
    while start > 0 && (line_text.chars().nth(start - 1).map_or(false, |c| c.is_alphanumeric() || c == '_')) {
        start -= 1;
    }

    // Find end of identifier
    let mut end = col;
    while end < line_text.len() && (line_text.chars().nth(end).map_or(false, |c| c.is_alphanumeric() || c == '_')) {
        end += 1;
    }

    if start >= end {
        return None;
    }

    Some((line_text[start..end].to_string(), start, end))
}

/// Create an LSP Range from line, column, and length.
pub fn create_range(line: usize, col: usize, length: usize) -> Range {
    Range {
        start: Position {
            line: line as u32,
            character: col as u32,
        },
        end: Position {
            line: line as u32,
            character: (col + length) as u32,
        },
    }
}

/// Extract variable name from an error message.
/// Messages are typically: "Variable 'b' is not declared..." or "b is not a variable..."
pub fn extract_variable_name_from_message(msg: &str) -> String {
    if let Some(start) = msg.find('\'') {
        // Extract from "Variable 'b' is not declared..."
        let after_quote = &msg[start + 1..];
        if let Some(end) = after_quote.find('\'') {
            after_quote[..end].to_string()
        } else if let Some(end) = after_quote.find(' ') {
            // Handle case where there's no closing quote
            after_quote[..end].to_string()
        } else {
            after_quote.to_string()
        }
    } else {
        // Extract from "b is not a variable..."
        if let Some(end) = msg.find(' ') {
            msg[..end].to_string()
        } else {
            msg.to_string()
        }
    }
}

/// Find a variable name in code lines (as a word boundary to avoid partial matches).
pub fn find_variable_in_code(lines: &[&str], var_name: &str) -> Option<(usize, usize)> {
    for (line_num, line) in lines.iter().enumerate() {
        // Find all occurrences and use the first one that's a valid identifier
        let mut search_pos = 0;
        while let Some(pos) = line[search_pos..].find(var_name) {
            let abs_pos = search_pos + pos;
            let after_pos = abs_pos + var_name.len();
            
            // Check if it's a valid identifier boundary (not part of a larger identifier)
            // Check character before (if exists)
            let before_ok = if abs_pos > 0 {
                line[..abs_pos].chars().last().map_or(true, |c| !c.is_alphanumeric() && c != '_')
            } else {
                true
            };
            
            // Check character after (if exists)
            let after_ok = if after_pos < line.len() {
                line[after_pos..].chars().next().map_or(true, |c| !c.is_alphanumeric() && c != '_')
            } else {
                true // At end of line, which is a valid boundary
            };
            
            if before_ok && after_ok {
                return Some((line_num, abs_pos));
            }
            search_pos = after_pos;
        }
    }
    None
}


