use tower_lsp::lsp_types::SemanticToken;

use crate::core::hir_lowering::{CompilerState, SemanticItemKind};

/// Token type indices (from legend in server.rs)
const TOKEN_FUNCTION: u32 = 0;
const TOKEN_VARIABLE: u32 = 1;
const TOKEN_TYPE: u32 = 2;
const TOKEN_OPERATOR: u32 = 3;
const TOKEN_KEYWORD: u32 = 4;
const TOKEN_NAMESPACE: u32 = 5;

// Token modifiers are now carried in SemanticItem.modifiers and projected directly

/// Convert absolute positions back to delta-encoded tokens.
fn absolute_to_delta_tokens(absolute_tokens: Vec<(u32, u32, u32, u32, u32)>) -> Vec<SemanticToken> {
    let mut sorted_tokens = Vec::new();
    let mut last_line = 0;
    let mut last_col = 0;
    
    for (line, col, length, token_type, modifiers) in absolute_tokens {
        let delta_line = if sorted_tokens.is_empty() {
            line
        } else {
            line - last_line
        };
        let delta_start = if sorted_tokens.is_empty() || delta_line > 0 {
            col
        } else {
            col - last_col
        };
        
        sorted_tokens.push(SemanticToken {
            delta_line,
            delta_start,
            length,
            token_type,
            token_modifiers_bitset: modifiers,
        });
        
        last_line = line;
        last_col = col;
    }
    sorted_tokens
}

/// Generate semantic tokens from compiler state.
/// 
/// This function extracts tokens directly from CompilerState.semantic_items.
/// All semantic items (keywords, operators, identifiers, types) flow from AST analysis.
/// No text scanning. No heuristics. No divergence.
pub fn generate_semantic_tokens(_text: &str, state: &CompilerState) -> Vec<SemanticToken> {
    let mut absolute_tokens: Vec<(u32, u32, u32, u32, u32)> = Vec::new();
    
    // Use precomputed line index for efficient lookups
    let line_index = match &state.line_index {
        Some(index) => index,
        None => {
            // If line_index is not available, return empty tokens
            // (shouldn't happen in normal operation, but handle gracefully)
            return Vec::new();
        }
    };
    
    // All semantic items come from compiler state - single source of truth
    for item in &state.semantic_items {
        let (line, col) = line_index.lookup(item.span.start);
        
        let token_type = match item.kind {
            SemanticItemKind::Function => TOKEN_FUNCTION,
            SemanticItemKind::Variable | SemanticItemKind::Parameter => TOKEN_VARIABLE,
            SemanticItemKind::Keyword => TOKEN_KEYWORD,
            SemanticItemKind::Operator => TOKEN_OPERATOR,
            SemanticItemKind::Type => TOKEN_TYPE,
            SemanticItemKind::Module => TOKEN_NAMESPACE,
        };
        
        // Use modifiers from semantic item - pure projection, no computation
        let modifiers = item.modifiers.bits();
        
        absolute_tokens.push((
            line,
            col,
            item.span.length() as u32,
            token_type,
            modifiers,
        ));
    }
    
    // Sort all tokens by line and column
    absolute_tokens.sort_by(|a, b| {
        a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1))
    });
    
    // Convert to delta encoding
    absolute_to_delta_tokens(absolute_tokens)
}

// All text scanning functions removed - semantic items now come directly from CompilerState.
// Keywords, operators, identifiers, and types are all extracted from AST in collect_semantic_items().
