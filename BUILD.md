# Building the CantaLoop Extension

This project supports building on Windows, macOS, and Linux.

## Quick Start

### Windows
```powershell
.\rebuild-extension.ps1
```

### macOS/Linux
```bash
./rebuild-extension.sh
```

Or use the VS Code task: Press `Ctrl+Shift+B` (or `Cmd+Shift+B` on macOS) to run "Rebuild Extension".

## What the Scripts Do

1. **Build the LSP server** in release mode using `cargo build --release --bin cantaloop-lsp`
2. **Copy the binary** to `.cantaloop-language/server/` with the correct name:
   - Windows: `cantaloop-lsp.exe`
   - macOS/Linux: `cantaloop-lsp`
3. **Package the extension** as a `.vsix` file using `vsce package`

## Platform-Specific Notes

- **Windows**: The binary is named `cantaloop-lsp.exe`
- **macOS/Linux**: The binary is named `cantaloop-lsp` (no extension)
- The extension automatically detects the platform and uses the correct binary name

## Requirements

- Rust and Cargo installed
- Node.js and npm installed
- `vsce` installed globally: `npm install -g @vscode/vsce`

## Installing the Extension

### Quick Install (Recommended)
Use the VS Code task: Press `Ctrl+Shift+P` / `Cmd+Shift+P` → "Tasks: Run Task" → "Reinstall Extension"

This will automatically:
1. Uninstall the old version
2. Install the new `.vsix` file

### Manual Install
1. Open the Command Palette (`Ctrl+Shift+P` / `Cmd+Shift+P`)
2. Run "Extensions: Install from VSIX..."
3. Select `.cantaloop-language/cantaloop-language-0.0.1.vsix`

### Command Line Install
**Windows:**
```powershell
cd .cantaloop-language
cursor --uninstall-extension yoran.cantaloop-language
cursor --install-extension cantaloop-language-0.0.1.vsix
```

**macOS/Linux:**
```bash
cd .cantaloop-language
cursor --uninstall-extension yoran.cantaloop-language
cursor --install-extension cantaloop-language-0.0.1.vsix
```

