# LSP Development Workflow

This guide explains how to develop the CantaLoop LSP server efficiently with hot-reloading.

## Option 1: Extension Development Mode (Recommended - Fastest)

VS Code/Cursor supports an "Extension Development Host" mode that allows you to test your extension without packaging and reinstalling it.

### Setup

1. **First, build the LSP server** (debug mode is faster for development):
   ```powershell
   # Windows
   .\rebuild-lsp-dev.ps1
   
   # macOS/Linux
   ./rebuild-lsp-dev.sh
   ```
   
   Or use the VS Code task: `Ctrl+Shift+P` → "Tasks: Run Task" → "Quick Rebuild LSP (Dev)"

2. **Launch Extension Development Host**:
   - Press `F5` (or go to Run → Start Debugging)
   - Select "Launch Extension (Development)" if prompted
   - This opens a new VS Code/Cursor window with your extension loaded
   - The extension points directly to your local `.cantaloop-language` folder

**Note**: If you get an error about `cppvsdbg` not being supported, that's normal - we removed that configuration. Just make sure "Launch Extension (Development)" is selected.

### Development Workflow

1. Make changes to LSP code in `src/lsp_server.rs` or other files
2. Rebuild the LSP binary:
   - Run the task: `Ctrl+Shift+P` → "Tasks: Run Task" → "Quick Rebuild LSP (Dev)"
   - Or run `.\rebuild-lsp-dev.ps1` manually
3. **Restart the LSP server** (no need to reload the whole window):
   - `Ctrl+Shift+P` → "Developer: Restart Extension Host"
   - This restarts just the extension host, not the entire window
   - Your LSP server will restart with the new binary

**Time saved**: ~5-10 seconds per iteration (no packaging, no reinstalling, no full window reload)

### Tips

- Use debug builds (`cargo build --bin cantaloop-lsp`) for faster compilation during development
- Use release builds only when you want to test performance or create the final package
- Keep the Extension Development Host window open - you can have both windows open side-by-side
- Changes to the extension.js file require a full extension host restart (but not a window reload)

## Option 2: Using cargo watch (Auto-rebuild) - Recommended for Active Development

For even faster iteration, you can use `cargo watch` to automatically rebuild when files change:

1. **Install cargo-watch** (if not already installed):
   ```bash
   cargo install cargo-watch
   ```

2. **Run watch mode** using the provided scripts:
   ```powershell
   # Windows PowerShell - LSP only
   .\watch-lsp.ps1
   
   # Windows PowerShell - melon only
   .\watch-melon.ps1
   
   # Windows PowerShell - both (runs in parallel)
   .\watch-all.ps1
   ```
   
   ```bash
   # macOS/Linux - LSP only
   ./watch-lsp.sh
   
   # macOS/Linux - melon only
   ./watch-melon.sh
   
   # macOS/Linux - both (requires GNU parallel, or run in separate terminals)
   ./watch-all.sh
   ```

3. **Restart the extension host** when you see the rebuild complete:
   - In your Extension Development Host window: `Ctrl+Shift+P` → "Developer: Restart Extension Host"
   - The watch scripts will show a reminder message after each rebuild
   - **Note**: VS Code/Cursor doesn't support auto-restarting the extension host, so this step is still manual

**Benefits**: 
- No need to manually run rebuild commands
- See build errors immediately as you save files
- Faster feedback loop during active development

## Option 3: Full Rebuild (For Release)

When you want to create a release build or test the final package:

1. Run the full rebuild script:
   ```powershell
   .\rebuild-extension.ps1
   ```
   
   Or use the task: `Ctrl+Shift+B` → "Rebuild Extension"

2. Reinstall the extension (if using the installed version):
   - `Ctrl+Shift+P` → "Tasks: Run Task" → "Reinstall Extension"
   - Or manually: `cursor --install-extension .cantaloop-language/cantaloop-language-0.0.1.vsix`

## Comparison

| Method | Build Time | Restart Time | Total Iteration Time |
|--------|-----------|--------------|---------------------|
| Extension Dev Mode (F5) | ~5-15s (debug) | ~2s (extension host) | ~7-17s |
| Full Rebuild + Reinstall | ~30-60s (release) | ~5-10s (full reload) | ~35-70s |

**Recommendation**: Use Extension Development Mode (F5) for day-to-day development. Only do full rebuilds when preparing a release or testing the final package.

