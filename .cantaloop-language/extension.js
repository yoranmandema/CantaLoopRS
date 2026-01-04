const path = require("path");
const fs = require("fs");
const cp = require("child_process");
const vscode = require("vscode");

let client;

function activate(context) {
  const output = vscode.window.createOutputChannel("CantaLoop LSP");
  output.appendLine("Activating CantaLoop extension...");

  // Get the path to the LSP server binary
  const isWindows = process.platform === "win32";
  const serverName = isWindows ? "cantaloop-lsp.exe" : "cantaloop-lsp";
  const serverPath = path.join(context.extensionPath, "server", serverName);

  if (!fs.existsSync(serverPath)) {
    const errorMsg = `CantaLoop LSP server binary not found at: ${serverPath}`;
    vscode.window.showErrorMessage(errorMsg);
    output.appendLine(errorMsg);
    console.error(errorMsg);
    return;
  }

  output.appendLine(`Starting CantaLoop LSP server from: ${serverPath}`);

  let LanguageClient;
  try {
    LanguageClient = require("vscode-languageclient/node").LanguageClient;
  } catch (e) {
    const errorMsg = `Failed to load vscode-languageclient: ${e.message}`;
    vscode.window.showErrorMessage(errorMsg);
    output.appendLine(errorMsg);
    console.error(errorMsg, e);
    return;
  }

  const serverOptions = {
    command: serverPath,
    args: [],
  };

  const clientOptions = {
    documentSelector: [{ scheme: "file", language: "cantaloop" }],
    outputChannel: output,
  };

  try {
    client = new LanguageClient(
      "cantaloop",
      "CantaLoop Language Server",
      serverOptions,
      clientOptions
    );
  } catch (e) {
    const errorMsg = `Failed to construct LanguageClient: ${e.message}`;
    vscode.window.showErrorMessage(errorMsg);
    output.appendLine(errorMsg);
    console.error(errorMsg, e);
    return;
  }

  // Start the client and handle errors
  client.start().then(
    () => {
      output.appendLine("CantaLoop LSP client started successfully");
      console.log("CantaLoop LSP client started successfully");
    },
    (error) => {
      const errorMsg = `Failed to start CantaLoop LSP: ${error && error.message ? error.message : String(error)}`;
      vscode.window.showErrorMessage(errorMsg + " — see 'CantaLoop LSP' output for details.");
      output.appendLine(errorMsg);
      output.appendLine(String(error));
      console.error(errorMsg, error);
    }
  );

  // Also listen for unhandled errors from the extension host side
  process.on("uncaughtException", (err) => {
    output.appendLine("Uncaught exception in extension host: " + String(err));
  });

  context.subscriptions.push(client, output);
}

function deactivate() {
  if (!client) return;
  return client.stop();
}

module.exports = { activate, deactivate };

