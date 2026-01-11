# Quick Start: Running the CantaLoop Extension

## Prerequisites

1. **Build the LSP binary** (must be done first):
   ```bash
   cd ..
   cargo build --bin cantaloop-lsp
   ```

   On Windows, the binary will be at:
   - `target\debug\cantaloop-lsp.exe`

   On Linux/macOS:
   - `target/debug/cantaloop-lsp`

2. **Install extension dependencies**:
   ```bash
   cd vscode-extension
   npm install
   ```

3. **Compile TypeScript**:
   ```bash
   npm run compile
   ```

## Running the Extension (Development)

### Method 1: Launch from VS Code (Recommended)

1. **Open the extension folder in VS Code**:
   - Open VS Code
   - File → Open Folder
   - Select `CantaLoopRS/vscode-extension`

2. **Launch Extension Development Host**:
   - Press **F5** (or go to Run → Start Debugging)
   - A new VS Code window opens labeled "[Extension Development Host]"

3. **Test the extension**:
   - In the new window, create a new file (File → New File)
   - Save it as `test.cl` (or open an existing `.cl` file)
   - The extension should activate automatically
   - You should see:
     - Syntax highlighting
     - Language shows as "CantaLoop" in status bar
     - Diagnostics (if there are errors)
     - Semantic tokens

### Method 2: Using Command Line

```bash
cd vscode-extension

# Compile TypeScript
npm run compile

# Launch VS Code with extension loaded
code --extensionDevelopmentPath=. test.cl
```

## Verifying It's Working

### Check 1: Language Detection
- Open a `.cl` file
- Check the status bar (bottom right)
- Should show "CantaLoop" as the language

### Check 2: LSP is Running
- View → Output (or Ctrl+Shift+U)
- Select "CantaLoop Language Server" from the dropdown
- Should see initialization messages:
  ```
  CantaLoop Language Server is starting...
  CantaLoop Language Server is ready and running
  ```

### Check 3: Syntax Highlighting
- Open a `.cl` file with code:
  ```cantaloop
  fn test() -> num {
      let x = 42;
      x
  }
  ```
- You should see:
  - Keywords (`fn`, `let`, `->`) highlighted
  - Numbers highlighted
  - Functions highlighted (if semantic tokens work)

### Check 4: Semantic Tokens (Advanced)
1. Command Palette (Ctrl+Shift+P)
2. Type: "Developer: Inspect Editor Tokens and Scopes"
3. Click on a token in your `.cl` file
4. Check if it says "semantic" (vs "textmate")

## Troubleshooting

### Extension Doesn't Activate

**Problem**: No highlighting, language doesn't show as "CantaLoop"

**Solutions**:
1. Check file extension is `.cl`
2. Make sure you're in the Extension Development Host window (not the original VS Code window)
3. Reload the window: Ctrl+R (or Command Palette → "Developer: Reload Window")

### LSP Not Starting

**Problem**: No diagnostics, Output panel shows errors

**Solutions**:
1. **Verify binary exists**:
   ```bash
   # Windows
   ls target\debug\cantaloop-lsp.exe
   
   # Linux/macOS
   ls target/debug/cantaloop-lsp
   ```

2. **Check binary path in extension**:
   - The extension looks for: `../target/debug/cantaloop-lsp` (or `.exe` on Windows)
   - Make sure you're running from the `vscode-extension` folder

3. **Check Output panel**:
   - View → Output
   - Select "CantaLoop Language Server"
   - Look for error messages

4. **Rebuild LSP**:
   ```bash
   cd ..
   cargo clean
   cargo build --bin cantaloop-lsp
   ```

### No Semantic Tokens

**Problem**: Only basic highlighting (TextMate), no semantic highlighting

**Solutions**:
1. **Check semantic highlighting is enabled**:
   - File → Preferences → Settings
   - Search: "semantic highlighting"
   - Ensure "Editor: Semantic Highlighting" is enabled

2. **Check token inspector** (see Check 4 above)
   - If tokens are "textmate", semantic tokens aren't working
   - If tokens are "semantic", it's working! ✅

3. **Check LSP is running** (see Check 2 above)

4. **Restart language server**:
   - Command Palette → "CantaLoop: Restart Language Server"

## Development Workflow

### Watch Mode (Auto-recompile)

1. Open terminal in VS Code
2. Run:
   ```bash
   npm run watch
   ```
3. This will recompile TypeScript automatically when you change `extension.ts`

### Reloading Extension Changes

After modifying `extension.ts`:
1. Stop the Extension Development Host
2. Run `npm run compile` (or use watch mode)
3. Press F5 again to relaunch

### Testing Different Files

Create test files in the Extension Development Host:
- `test.cl` - Simple test
- `error.cl` - Test error diagnostics
- `complex.cl` - Test advanced features

## Next Steps

Once it's working:
- Try go-to definition (Ctrl+Click or F12)
- Try find references (Shift+F12)
- Hover over symbols to see type information
- Test with actual CantaLoop projects

## Common Commands

```bash
# Build LSP binary
cd ..
cargo build --bin cantaloop-lsp

# Install extension deps
cd vscode-extension
npm install

# Compile TypeScript
npm run compile

# Watch mode (auto-compile)
npm run watch

# Launch extension (F5 in VS Code)
# Or from command line:
code --extensionDevelopmentPath=. test.cl
```

---

**Need help?** Check the Output panel for error messages from the Language Server.
