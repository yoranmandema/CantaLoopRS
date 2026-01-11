import * as vscode from "vscode";
import * as path from "path";
import { LanguageClient, LanguageClientOptions, ServerOptions, ErrorAction, CloseAction } from "vscode-languageclient/node";

// Store clients per workspace folder - do NOT recreate on activation
const clients = new Map<string, LanguageClient>();

export function activate(context: vscode.ExtensionContext) {
  // Must start one language client per workspace folder
  if (!vscode.workspace.workspaceFolders) {
    console.warn("No workspace folders found - CantaLoop LSP will not start");
    return;
  }

  const isDevelopment = context.extensionMode === vscode.ExtensionMode.Development;

  for (const folder of vscode.workspace.workspaceFolders) {
    // Do NOT recreate client if it already exists
    const folderUri = folder.uri.toString();
    if (clients.has(folderUri)) {
      console.log(`Client already exists for workspace: ${folder.name}, skipping`);
      continue;
    }
    // Determine server executable path
    let serverExe: string;
    if (isDevelopment) {
      // Development: use the binary from the Rust project
      const rustProjectRoot = path.resolve(context.extensionPath, "..");
      serverExe = path.join(rustProjectRoot, "target", "debug", "cantaloop-lsp");
      
      // On Windows, add .exe extension
      if (process.platform === "win32") {
        serverExe += ".exe";
      }
    } else {
      // Production: use bundled binary
      serverExe = context.asAbsolutePath(
        path.join("bin", process.platform === "win32" ? "cantaloop-lsp.exe" : "cantaloop-lsp")
      );
    }

    // Configure server options with workspace folder as working directory
    const serverOptions: ServerOptions = {
      command: serverExe,
      args: [],
      options: {
        cwd: folder.uri.fsPath,
        env: {
          ...process.env,
          // Ensure RUST_BACKTRACE is set for better error messages
          RUST_BACKTRACE: "1",
        },
      },
    };

    // Configure client options - scope to this workspace folder
    const folderPattern = folder.uri.fsPath.replace(/\\/g, "/");
    const clientOptions: LanguageClientOptions = {
      documentSelector: [
        {
          scheme: "file",
          language: "cantaloop",
          pattern: `${folderPattern}/**/*`
        },
        {
          scheme: "untitled",
          language: "cantaloop",
        },
      ],
      initializationOptions: {},
      synchronize: {},
      // CRITICAL: Disable auto-restart to prevent VS Code from killing the server
      errorHandler: {
        error: () => ({ action: ErrorAction.Continue }),
        closed: () => ({ action: CloseAction.DoNotRestart })
      }
    };

    // Create and start the language client for this workspace
    const client = new LanguageClient(
      `cantaloop-${folder.name}`,
      `CantaLoop (${folder.name})`,
      serverOptions,
      clientOptions
    );

    // Handle client state changes
    client.onDidChangeState((e) => {
      if (e.newState === 1) { // Starting
        console.log(`CantaLoop Language Server (${folder.name}) is starting...`);
      } else if (e.newState === 2) { // Running
        console.log(`CantaLoop Language Server (${folder.name}) is ready and running`);
      } else if (e.newState === 3) { // Stopping
        console.log(`CantaLoop Language Server (${folder.name}) is stopping...`);
      }
    });

    // Start the client and track it
    client.start();
    clients.set(folderUri, client);
    
    // Add to context subscriptions for cleanup
    context.subscriptions.push({
      dispose: () => {
        client.stop();
        clients.delete(folderUri);
      }
    });
  }

  // Handle workspace folder removals
  context.subscriptions.push(
    vscode.workspace.onDidChangeWorkspaceFolders((e) => {
      for (const removed of e.removed) {
        const uri = removed.uri.toString();
        const client = clients.get(uri);
        if (client) {
          client.stop();
          clients.delete(uri);
        }
      }
      // Add new folders (if any) - but don't restart existing ones
      for (const added of e.added) {
        // This will be handled by the extension activation if needed
        // For now, we only handle removals here
      }
    })
  );

  // Log when documents are opened
  context.subscriptions.push(
    vscode.workspace.onDidOpenTextDocument((doc) => {
      if (doc.languageId === "cantaloop") {
        console.log(`File opened: ${doc.fileName} (language: ${doc.languageId})`);
      }
    })
  );

  // Register commands
  context.subscriptions.push(
    vscode.commands.registerCommand("cantaloop.restartLanguageServer", async () => {
      // Restart all clients
      let restarted = 0;
      for (const [uri, c] of clients.entries()) {
        await c.stop();
        c.start();
        restarted++;
      }
      vscode.window.showInformationMessage(`CantaLoop Language Server restarted (${restarted} workspace(s))`);
    })
  );
}

export function deactivate(): Thenable<void> | undefined {
  // Stop all clients
  const promises: Thenable<void>[] = [];
  for (const client of clients.values()) {
    promises.push(client.stop());
  }
  clients.clear();
  
  if (promises.length === 0) {
    return undefined;
  }
  
  // Return a promise that resolves when all clients are stopped
  return Promise.all(promises).then(() => {});
}
