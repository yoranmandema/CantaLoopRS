# VS Code Extension Checklist

## ✅ Setup Complete

All extension files have been created:

- ✅ `package.json` - Extension manifest with language registration
- ✅ `language-configuration.json` - Language configuration (comments, brackets)
- ✅ `syntaxes/cantaloop.tmLanguage.json` - TextMate grammar (fallback)
- ✅ `src/extension.ts` - Extension entry point with LSP client
- ✅ `tsconfig.json` - TypeScript configuration
- ✅ `.vscode/launch.json` - Debug configuration
- ✅ `.vscode/tasks.json` - Build tasks
- ✅ `README.md` - Extension documentation
- ✅ `.gitignore` - Git ignore rules

## 🔍 Verification Steps

### 1. Build LSP Binary

```bash
cargo build --bin cantaloop-lsp
```

Verify binary exists:
- `target/debug/cantaloop-lsp` (Linux/macOS)
- `target/debug/cantaloop-lsp.exe` (Windows)

### 2. Install Extension Dependencies

```bash
cd vscode-extension
npm install
```

This installs:
- `vscode-languageclient@^9.0.0`
- `typescript@^5.0.0`
- `@types/vscode@^1.74.0`

### 3. Compile TypeScript

```bash
npm run compile
```

Or watch mode:
```bash
npm run watch
```

### 4. Test Extension

1. Open `vscode-extension` folder in VS Code
2. Press **F5** to launch Extension Development Host
3. In new window, create/open a `.cl` file:
   ```cantaloop
   fn main() -> num {
       let x = 42;
       x
   }
   ```
4. Verify:
   - Language shows as "CantaLoop" in status bar
   - Basic highlighting appears (TextMate grammar)
   - LSP starts (check Output panel → "CantaLoop Language Server")
   - Semantic tokens override grammar (better highlighting)

## 🎯 Critical Verification Points

### Semantic Token Legend Match

**LSP Legend** (from `src/lsp/handlers/initialize.rs`):
```rust
token_types: vec![
    SemanticTokenType::FUNCTION,      // Index 0
    SemanticTokenType::VARIABLE,      // Index 1
    SemanticTokenType::PARAMETER,     // Index 2
    SemanticTokenType::KEYWORD,       // Index 3
    SemanticTokenType::OPERATOR,      // Index 4
    SemanticTokenType::STRING,        // Index 5
    SemanticTokenType::NUMBER,        // Index 6
    SemanticTokenType::COMMENT,       // Index 7
    SemanticTokenType::TYPE,          // Index 8
]
```

**Extension Mapping** (from `package.json`):
```json
"semanticTokenScopes": {
  "function": ["entity.name.function.cantaloop"],
  "variable": ["variable.other.cantaloop"],
  "parameter": ["variable.parameter.cantaloop"],
  "type": ["entity.name.type.cantaloop"]
}
```

✅ **Status**: These match. The LSP emits tokens with types 0-8, and the extension maps them correctly.

### Token Emission Verification

Check `src/lsp/handlers/tokens.rs` to verify tokens are emitted with correct types:

```rust
// Function: type 0
// Variable: type 1  
// Parameter: type 2
// Type (Module): type 8
```

✅ **Status**: Token types in `generate_semantic_tokens()` match the legend.

## 🐛 Common Issues & Solutions

### Issue: Extension doesn't activate

**Symptoms**: No highlighting, status bar doesn't show "CantaLoop"

**Solutions**:
1. Check `package.json` has correct language ID: `"id": "cantaloop"`
2. Verify file extension: `.cl` or `.mln`
3. Check activation events: `"onLanguage:cantaloop"`

### Issue: LSP doesn't start

**Symptoms**: No diagnostics, no semantic tokens, Output panel shows errors

**Solutions**:
1. Verify binary exists: `target/debug/cantaloop-lsp`
2. Check binary is executable (Linux/macOS): `chmod +x target/debug/cantaloop-lsp`
3. Check extension.ts path resolution (development vs production)
4. Look at Output panel → "CantaLoop Language Server" for errors

### Issue: Semantic tokens not showing

**Symptoms**: Only TextMate grammar highlighting, no semantic tokens

**Solutions**:
1. Verify semantic highlighting enabled: `"editor.semanticHighlighting.enabled": true`
2. Use token inspector: Command Palette → "Developer: Inspect Editor Tokens and Scopes"
3. Check if LSP is emitting tokens (check Output panel)
4. Verify token legend matches (see above)
5. Check for token type mismatches (LSP emits type 0, but legend says something else)

### Issue: Binary path wrong

**Symptoms**: LSP can't find binary

**Solutions**:
1. Development mode: Binary should be at `../target/debug/cantaloop-lsp`
2. Production mode: Binary should be at `bin/cantaloop-lsp`
3. Update `extension.ts` if your structure differs

## 📋 Next Steps

Once extension works:

1. **Test All LSP Features**:
   - [ ] Diagnostics (errors/warnings)
   - [ ] Go-to definition (Ctrl+Click)
   - [ ] Find references (Shift+F12)
   - [ ] Hover (mouse over symbol)
   - [ ] Semantic tokens (verify override grammar)

2. **Verify Semantic Token Quality**:
   - [ ] Functions highlighted correctly
   - [ ] Variables highlighted correctly
   - [ ] Parameters highlighted correctly
   - [ ] Types (modules) highlighted correctly
   - [ ] Literals (strings, numbers) highlighted correctly

3. **Future Enhancements**:
   - [ ] Effect-aware highlighting (pure vs effectful functions)
   - [ ] Execution marker highlighting (`!`)
   - [ ] Pipeline operator highlighting (`->` vs `~>`)

## 🔗 Resources

- VS Code Extension API: https://code.visualstudio.com/api
- Language Server Protocol: https://microsoft.github.io/language-server-protocol/
- TextMate Grammar Guide: https://macromates.com/manual/en/language_grammars

---

**Last Updated**: After creating extension structure.
