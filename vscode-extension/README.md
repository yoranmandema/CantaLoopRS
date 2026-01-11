# CantaLoop VS Code Extension

Language support for CantaLoop in VS Code and Cursor.

## Features

- **Syntax Highlighting**: TextMate grammar for basic lexical highlighting (fallback)
- **Semantic Highlighting**: Rich semantic tokens from the LSP (primary)
- **Language Server Protocol**: Full LSP support via `cantaloop-lsp` binary
  - Diagnostics (errors and warnings)
  - Go-to definition
  - Find references
  - Hover information
  - Semantic tokens

## Installation

### Development

1. Build the LSP binary:
   ```bash
   cd ..
   cargo build --bin cantaloop-lsp
   ```

2. Install extension dependencies:
   ```bash
   cd vscode-extension
   npm install
   ```

3. Compile TypeScript:
   ```bash
   npm run compile
   ```

4. Press F5 in VS Code to launch the extension in a new window.

### Production

1. Build the LSP binary for your platform:
   ```bash
   cargo build --release --bin cantaloop-lsp
   ```

2. Copy the binary to `vscode-extension/bin/`:
   ```bash
   # Linux/macOS
   cp target/release/cantaloop-lsp vscode-extension/bin/cantaloop-lsp
   
   # Windows
   copy target\release\cantaloop-lsp.exe vscode-extension\bin\cantaloop-lsp.exe
   ```

3. Package the extension:
   ```bash
   cd vscode-extension
   npm install
   npm run compile
   vsce package
   ```

## Configuration

The extension automatically detects `.cl` files as CantaLoop source code.

### Semantic Highlighting

Semantic highlighting is enabled by default. To verify it's working:

1. Open Developer Tools (Help → Toggle Developer Tools)
2. Use Command Palette → "Developer: Inspect Editor Tokens and Scopes"
3. Click on tokens to see if they're semantic or TextMate-based

### Language Server Location

In development mode, the extension looks for the LSP binary at:
- `../target/debug/cantaloop-lsp` (or `.exe` on Windows)

In production, it uses:
- `bin/cantaloop-lsp` (or `.exe` on Windows)

## Troubleshooting

### Semantic tokens not showing

1. Check that semantic tokens are enabled:
   - Settings → Editor: Semantic Highlighting → Enabled

2. Verify LSP is running:
   - Open Output panel → Select "CantaLoop Language Server"
   - Look for initialization messages

3. Check token legend matches:
   - The LSP must emit tokens matching the legend declared in `initialize`
   - Mismatches cause VS Code to silently drop tokens

### LSP not starting

1. Check binary exists and is executable:
   ```bash
   # Linux/macOS
   chmod +x bin/cantaloop-lsp
   
   # Verify it runs
   ./bin/cantaloop-lsp --version
   ```

2. Check extension logs:
   - View → Output → Select "CantaLoop Language Server"

3. Restart language server:
   - Command Palette → "CantaLoop: Restart Language Server"

## Architecture

This extension follows the standard VS Code language extension pattern:

- **TextMate Grammar**: Basic fallback highlighting (`syntaxes/cantaloop.tmLanguage.json`)
- **Semantic Tokens**: Rich highlighting from LSP (primary source of truth)
- **Language Server**: Separate binary (`cantaloop-lsp`) handles all language intelligence

The LSP is the single source of truth for all semantic information. The TextMate grammar is only used when the LSP is unavailable.

## Development

### Project Structure

```
vscode-extension/
├── src/
│   └── extension.ts          # Extension entry point
├── syntaxes/
│   └── cantaloop.tmLanguage.json  # TextMate grammar
├── language-configuration.json    # Language config
├── package.json                   # Extension manifest
└── tsconfig.json                  # TypeScript config
```

### Building

```bash
npm run compile      # Compile TypeScript
npm run watch        # Watch mode for development
```

### Testing

1. Open this folder in VS Code
2. Press F5 to launch Extension Development Host
3. Open a `.cl` file to test

## License

Same as CantaLoop project.
