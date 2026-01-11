# Semantic Tokens Fix

## Issue

VS Code was requesting `textDocument/semanticTokens/range` but we only implemented `textDocument/semanticTokens/full`, causing errors.

## Fixes Applied

1. **Disabled range requests**: Changed `range: Some(true)` to `range: None` in initialization
2. **Fixed delta calculation**: First token now uses absolute position (required by LSP spec)
3. **Added logging**: Added debug log to see how many tokens are generated

## What Changed

### src/lsp/handlers/initialize.rs
- Set `range: None` instead of `range: Some(true)`
- This tells VS Code we don't support range requests, only full document requests

### src/lsp/handlers/tokens.rs
- Fixed delta calculation: first token uses absolute line/column
- Added logging to help debug token generation

## Testing

After rebuilding:

1. **Rebuild LSP**:
   ```bash
   cargo build --bin cantaloop-lsp
   ```

2. **Restart Extension Development Host** (F5 again)

3. **Check Output panel**:
   - Should see: "Generated X semantic tokens for file"
   - Should NOT see range request errors

4. **Verify highlighting**:
   - Open a `.cl` file with code
   - Should see semantic highlighting (not just TextMate)
   - Use token inspector: Command Palette → "Developer: Inspect Editor Tokens and Scopes"
   - Click on tokens - should say "semantic" not "textmate"

## Expected Behavior

- No more "Method not found" errors for range requests
- Semantic tokens should work for full document requests
- Highlighting should use semantic tokens (from LSP) instead of just TextMate grammar
