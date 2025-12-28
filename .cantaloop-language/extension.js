const path = require("path");
const vscode = require("vscode");

let client;

function activate(context) {
  // Get the path to the LSP server binary
  // For Windows, use .exe, for Linux/Mac use the binary without extension
  const isWindows = process.platform === "win32";
  const serverName = isWindows ? "cantaloop-lsp.exe" : "cantaloop-lsp";
  const serverPath = path.join(context.extensionPath, "server", serverName);

  const serverOptions = {
    command: serverPath,
    args: []
  };

  const clientOptions = {
    documentSelector: [{ scheme: "file", language: "cantaloop" }]
  };

  const { LanguageClient } = require("vscode-languageclient/node");
  client = new LanguageClient(
    "cantaloop",
    "CantaLoop Language Server",
    serverOptions,
    clientOptions
  );

  context.subscriptions.push(client.start());
}

function deactivate() {
  if (!client) return;
  return client.stop();
}

module.exports = { activate, deactivate };

