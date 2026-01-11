# VS Code / Cursor Extension Setup Guide

## Overview

This guide explains how to set up and use the CantaLoop VS Code/Cursor extension.

## Architecture

The extension follows the standard VS Code language extension pattern:

```
VS Code/Cursor Extension
    ↓ (Language Client)
CantaLoop LSP Binary (cantaloop-lsp)
    ↓ (LSP Protocol)
Compiler State (CompilerState)
```

### Two-Layer Highlighting

VS Code supports two independent highlighting systems:

| Layer | Purpose | Provider | When Used |
|-------|---------|----------|-----------|
| TextMate Grammar | Basic lexical highlighting | Extension | Fallback when LSP unavailable |
| Semantic Tokens | Meaning-aware highlighting | LSP | **Primary source of truth** |

**Key Principle**: Semantic tokens from the LSP override TextMate grammar automatically.

## Setup Instructions

### 1. Build the LSP Binary

First, ensure the LSP binary is built:

```bash
# Debug build (for development)
cargo build --bin cantaloop-lsp

# Release build (for production)
cargo build --release --bin cantaloop-lsp
```

The binary will be at:
- Debug: `target/debug/cantaloop-lsp` (`.exe` on Windows)
- Release: `target/release/cantaloop-lsp` (`.exe` on Windows)

### 2. Install Extension Dependencies

```bash
cd vscode-extension
npm install
```

This installs:
- `vscode-languageclient` - VS Code LSP client library
- `typescript` - TypeScript compiler
- `@types/vscode` - VS Code API types

### 3. Compile TypeScript

```bash
npm run compile
```

Or use watch mode for development:
```bash
npm run watch
```

### 4. Test the Extension

**Option A: Launch from VS Code**

1. Open the `vscode-extension` folder in VS Code
2. Press F5 to launch Extension Development Host
3. In the new window, open a `.cl` file
4. The extension should activate automatically

**Option B: Install Locally**

```bash
# From vscode-extension directory
vsce package
code --install-extension cantaloop-0.1.0.vsix
```

## Extension Files

### package.json

Defines the extension manifest:
- Language registration (`.cl`, `.mln` files)
- TextMate grammar registration
- Semantic token scope mappings
- Activation events

### language-configuration.json

Defines language features:
- Comments (`//`, `/* */`)
- Brackets and auto-closing pairs
- Indentation rules

### syntaxes/cantaloop.tmLanguage.json

TextMate grammar for fallback highlighting:
- Keywords (`fn`, `let`, `if`, etc.)
- Literals (strings, numbers, booleans)
- Operators (`->`, `~>`, `!`, etc.)
- Comments

**Note**: This is a fallback. Semantic tokens from the LSP provide the real highlighting.

### src/extension.ts

Extension entry point:
- Launches the LSP binary
- Configures language client
- Handles activation/deactivation

**Key Configuration**:
```typescript
// Development: uses target/debug/cantaloop-lsp
// Production: uses bin/cantaloop-lsp
const serverExe = isDevelopment
  ? path.join(rustProjectRoot, "target", "debug", "cantaloop-lsp")
  : context.asAbsolutePath(path.join("bin", "cantaloop-lsp"));
```

## Semantic Tokens Configuration

### LSP Legend (Must Match)

The LSP declares this legend during `initialize`:

```rust
token_types: vec![
    SemanticTokenType::FUNCTION,    // 0
    SemanticTokenType::VARIABLE,    // 1
    SemanticTokenType::PARAMETER,   // 2
    SemanticTokenType::KEYWORD,     // 3
    SemanticTokenType::OPERATOR,    // 4
    SemanticTokenType::STRING,      // 5
    SemanticTokenType::NUMBER,      // 6
    SemanticTokenType::COMMENT,     // 7
    SemanticTokenType::TYPE,        // 8
]
```

### Extension Scope Mapping

The extension maps semantic token types to TextMate scopes:

```json
"semanticTokenScopes": [
  {
    "language": "cantaloop",
    "scopes": {
      "function": ["entity.name.function.cantaloop"],
      "variable": ["variable.other.cantaloop"],
      "parameter": ["variable.parameter.cantaloop"],
      "type": ["entity.name.type.cantaloop"]
    }
  }
]
```

**Important**: If the LSP emits token type 0, VS Code expects it to be a function. Mismatches cause tokens to be silently dropped.

## Verification Checklist

### ✅ Extension Works

- [ ] Language is registered (files show "CantaLoop" in status bar)
- [ ] TextMate grammar works (basic highlighting without LSP)
- [ ] LSP binary launches (check Output panel)
- [ ] Semantic tokens work (use token inspector)

### ✅ LSP Integration

- [ ] Diagnostics appear on errors
- [ ] Go-to definition works (Ctrl+Click or F12)
- [ ] Find references works (Shift+F12)
- [ ] Hover shows type information
- [ ] Semantic tokens override grammar

## Debugging

### Semantic Tokens Not Showing

1. **Check LSP is advertising semantic tokens**:
   - Open Developer Tools → Console
   - Look for `textDocument/semanticTokens/full` request
   - If missing: capability not advertised or language ID mismatch

2. **Use Token Inspector**:
   - Command Palette → "Developer: Inspect Editor Tokens and Scopes"
   - Click on a token
   - Check:
     - Is it "semantic" or "TextMate"?
     - What token type is reported?
     - Does it match the legend?

3. **Verify Token Legend Match**:
   - LSP declares token types in `initialize` response
   - Must exactly match types emitted by `semanticTokensFull`
   - Mismatches cause silent token drops

4. **Check Semantic Highlighting Enabled**:
   ```json
   "editor.semanticHighlighting.enabled": true
   ```
   (Enabled by default in Cursor and modern VS Code)

### LSP Not Starting

1. **Check Binary Exists**:
   ```bash
   # Development
   ls target/debug/cantaloop-lsp
   
   # Production
   ls vscode-extension/bin/cantaloop-lsp
   ```

2. **Check Binary is Executable** (Linux/macOS):
   ```bash
   chmod +x bin/cantaloop-lsp
   ```

3. **Check Extension Logs**:
   - View → Output → Select "CantaLoop Language Server"
   - Look for error messages

4. **Restart Language Server**:
   - Command Palette → "CantaLoop: Restart Language Server"

### Test Grammar vs Semantic Tokens

To verify semantic tokens are working (not just grammar):

1. Disable TextMate rules:
   ```json
   "editor.tokenColorCustomizations": {
     "textMateRules": []
   }
   ```

2. If highlighting disappears → grammar was doing the work
3. If highlighting stays → semantic tokens are working ✅

## Production Build

### Prepare Binary

```bash
# Build release binary
cargo build --release --bin cantaloop-lsp

# Copy to extension bin/ directory
# Linux/macOS
cp target/release/cantaloop-lsp vscode-extension/bin/cantaloop-lsp
chmod +x vscode-extension/bin/cantaloop-lsp

# Windows
copy target\release\cantaloop-lsp.exe vscode-extension\bin\cantaloop-lsp.exe
```

### Package Extension

```bash
cd vscode-extension
npm install -g vsce  # VS Code Extension Manager
npm install
npm run compile
vsce package
```

This creates `cantaloop-0.1.0.vsix` which can be:
- Installed manually: `code --install-extension cantaloop-0.1.0.vsix`
- Published to VS Code Marketplace
- Distributed to users

## Next Steps

Once the extension is working:

1. **Test All LSP Features**:
   - Diagnostics
   - Go-to definition
   - References
   - Hover
   - Semantic tokens

2. **Verify Semantic Token Quality**:
   - Functions are highlighted
   - Variables are highlighted
   - Parameters are highlighted
   - Effect-aware tokens (when implemented)

3. **Add Effect-Specific Highlighting**:
   - Pure vs effectful functions
   - Execution markers (`!`)
   - Pipeline operators (`->`, `~>`)

4. **Publish Extension** (optional):
   - Create Azure DevOps account
   - Install `vsce` globally
   - Publish with `vsce publish`

## Important Notes

### Binary Path

- **Development**: Extension looks in `../target/debug/cantaloop-lsp`
- **Production**: Extension looks in `bin/cantaloop-lsp`

Update `extension.ts` if your build structure differs.

### Windows Support

- Binary must have `.exe` extension
- Extension handles this automatically
- Ensure binary is built for Windows if distributing

### Token Legend Matching

**Critical**: The token types emitted by the LSP must exactly match the legend declared during `initialize`. Any mismatch causes VS Code to silently drop tokens.

Check `src/lsp/handlers/initialize.rs` and `src/lsp/handlers/tokens.rs` to ensure consistency.

---

**Last Updated**: After creating initial extension structure.
