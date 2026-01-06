use tower_lsp::lsp_types::{Position, Range};

/// Convert a byte position in text to (line, column) coordinates.
pub fn byte_position_to_line_col(text: &str, pos: usize) -> (usize, usize) {
    // Safe string slicing - use get() to handle multi-byte characters
    let text_before = text.get(..pos).unwrap_or("");
    let line = text_before.matches('\n').count();
    let col = text_before
        .rfind('\n')
        .map(|last_nl| pos - last_nl - 1)
        .unwrap_or(pos);
    (line, col)
}

/// Extract identifier at a given position in text.
/// Returns (identifier, start_col, end_col) if found.
/// Note: col is a character position, not a byte position.
pub fn extract_identifier_at_position(text: &str, line: usize, col: usize) -> Option<(String, usize, usize)> {
    let lines: Vec<&str> = text.lines().collect();
    if line >= lines.len() {
        return None;
    }

    let line_text = lines[line];
    
    // Convert character position to byte position
    let char_indices: Vec<(usize, char)> = line_text.char_indices().collect();
    if col >= char_indices.len() {
        return None;
    }

    // Find start of identifier (working with character positions)
    let mut start_char_pos = col;
    while start_char_pos > 0 {
        let (_, ch) = char_indices[start_char_pos - 1];
        if ch.is_alphanumeric() || ch == '_' {
            start_char_pos -= 1;
        } else {
            break;
        }
    }

    // Find end of identifier (working with character positions)
    let mut end_char_pos = col;
    while end_char_pos < char_indices.len() {
        let (_, ch) = char_indices[end_char_pos];
        if ch.is_alphanumeric() || ch == '_' {
            end_char_pos += 1;
        } else {
            break;
        }
    }

    if start_char_pos >= end_char_pos {
        return None;
    }

    // Convert character positions to byte positions for slicing
    let start_byte = char_indices[start_char_pos].0;
    let end_byte = if end_char_pos < char_indices.len() {
        char_indices[end_char_pos].0
    } else {
        line_text.len()
    };

    // Safe string slicing - use get() to handle multi-byte characters
    match line_text.get(start_byte..end_byte) {
        Some(s) => Some((s.to_string(), start_byte, end_byte)),
        None => None, // Invalid char boundary
    }
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
        // Safe string slicing - use get() to handle multi-byte characters
        let after_quote = match msg.get(start + 1..) {
            Some(s) => s,
            None => return msg.to_string(), // Fallback if invalid char boundary
        };
        if let Some(end) = after_quote.find('\'') {
            after_quote.get(..end).unwrap_or(after_quote).to_string()
        } else if let Some(end) = after_quote.find(' ') {
            // Handle case where there's no closing quote
            after_quote.get(..end).unwrap_or(after_quote).to_string()
        } else {
            after_quote.to_string()
        }
    } else {
        // Extract from "b is not a variable..."
        if let Some(end) = msg.find(' ') {
            msg.get(..end).unwrap_or(msg).to_string()
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


