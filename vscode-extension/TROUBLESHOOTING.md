# Troubleshooting: Extension Conflicts

## Issue: Old Extension Conflicts

If you have an old CantaLoop extension installed from the marketplace or a previous `.vsix` file, it can conflict with your development extension.

## Solution: Uninstall Old Extension

### Method 1: Via VS Code UI

1. **Open Extensions View**:
   - Click the Extensions icon in the sidebar (or Ctrl+Shift+X)
   - Search for "CantaLoop" or "cantaloop"

2. **Find Installed Extension**:
   - Look for any installed CantaLoop extension
   - It might be:
     - From VS Code Marketplace
     - From a previously installed `.vsix` file

3. **Uninstall**:
   - Click the gear icon next to the extension
   - Select "Uninstall"
   - Restart VS Code

### Method 2: Via Command Line

```bash
# List installed extensions
code --list-extensions | grep -i cantaloop

# If found, uninstall it (replace ID with actual extension ID)
code --uninstall-extension <extension-id>
```

### Method 3: Manual Removal

1. **Find Extension Folder**:
   - Windows: `%USERPROFILE%\.vscode\extensions\`
   - macOS/Linux: `~/.vscode/extensions/`

2. **Delete CantaLoop Extension**:
   - Look for folders starting with `cantaloop` or containing "cantaloop"
   - Delete the entire folder

3. **Restart VS Code**

## Verify Old Extension is Gone

1. **Check Extensions View**:
   - Search for "CantaLoop"
   - Should show no installed extensions (only your development one if you have it open)

2. **Check Output Panel**:
   - View → Output
   - Should NOT see multiple CantaLoop Language Servers running

## Development Extension vs Installed Extension

### Development Extension (What You Want)

- **Location**: `CantaLoopRS/vscode-extension/`
- **How it runs**: Press F5 in VS Code → Opens Extension Development Host
- **Activation**: Only active in the Extension Development Host window
- **No conflicts**: Doesn't interfere with installed extensions

### Installed Extension (What Causes Conflicts)

- **Location**: `.vscode/extensions/` in your user directory
- **How it runs**: Automatically when you open `.cl` files
- **Activation**: Active in all VS Code windows
- **Causes conflicts**: Can conflict with development extension

## Best Practice

**Always uninstall the old extension before developing the new one.**

The development extension (F5) runs in an isolated Extension Development Host window, so it won't conflict with installed extensions in that window. However, if you have the old extension installed and you open `.cl` files in your main VS Code window, you might get confused about which extension is running.

## Testing the New Extension

After uninstalling the old extension:

1. **Build LSP binary**:
   ```bash
   cargo build --bin cantaloop-lsp
   ```

2. **Open extension folder**:
   ```bash
   cd vscode-extension
   code .
   ```

3. **Press F5** to launch Extension Development Host

4. **In the new window**, open a `.cl` file
   - This will use your development extension
   - Check Output panel → "CantaLoop Language Server" to verify

## If Still Having Issues

### Check for Multiple Language Servers

1. Open Output panel (View → Output)
2. Check if you see multiple "CantaLoop Language Server" entries
3. If yes, uninstall all CantaLoop extensions and restart

### Clear Extension Cache

```bash
# Windows
rmdir /s /q "%USERPROFILE%\.vscode\extensions\*cantaloop*"

# macOS/Linux
rm -rf ~/.vscode/extensions/*cantaloop*
```

### Check Settings

Sometimes extensions leave settings behind:

1. File → Preferences → Settings
2. Search for "cantaloop"
3. Remove any custom settings
4. Restart VS Code

## Extension IDs to Look For

When searching for old extensions, look for:
- `cantaloop.cantaloop` (or similar)
- Any extension with "cantaloop" in the name
- Any extension with "CantaLoop" in the display name

## Summary

1. ✅ Uninstall old extension (Extensions view or command line)
2. ✅ Restart VS Code
3. ✅ Open `vscode-extension` folder
4. ✅ Press F5 to launch development extension
5. ✅ Test in Extension Development Host window

---

**Note**: The development extension (F5) and installed extensions can coexist, but it's cleaner to uninstall the old one to avoid confusion.
