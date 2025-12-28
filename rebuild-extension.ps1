# Rebuild and package the CantaLoop language extension
# This script:
# 1. Builds the LSP server in release mode
# 2. Copies it to the extension directory
# 3. Packages the extension as a .vsix file

Write-Host "Building LSP server..." -ForegroundColor Cyan
cargo build --release --bin cantaloop-lsp

if ($LASTEXITCODE -ne 0) {
    Write-Host "Build failed!" -ForegroundColor Red
    exit 1
}

Write-Host "Copying LSP server to extension directory..." -ForegroundColor Cyan
$serverPath = "target\release\cantaloop-lsp.exe"
$extensionServerPath = ".cantaloop-language\server\cantaloop-lsp.exe"

if (Test-Path $serverPath) {
    Copy-Item $serverPath $extensionServerPath -Force
    Write-Host "LSP server copied successfully" -ForegroundColor Green
} else {
    Write-Host "LSP server binary not found at $serverPath" -ForegroundColor Red
    exit 1
}

Write-Host "Packaging extension..." -ForegroundColor Cyan
Push-Location .cantaloop-language
try {
    vsce package --out cantaloop-language-0.0.1.vsix
    if ($LASTEXITCODE -eq 0) {
        Write-Host "Extension packaged successfully: .cantaloop-language\cantaloop-language-0.0.1.vsix" -ForegroundColor Green
    } else {
        Write-Host "Packaging failed!" -ForegroundColor Red
        exit 1
    }
} finally {
    Pop-Location
}

Write-Host "`nDone! Extension ready for installation." -ForegroundColor Green

