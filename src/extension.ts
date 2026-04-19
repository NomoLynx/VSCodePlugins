import * as path from "path";
import * as vscode from "vscode";
import * as fs from "fs";
import {
  Executable,
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

function serverBinaryName(): string {
  return process.platform === "win32"
    ? "rust_keyword_lsp_server.exe"
    : "rust_keyword_lsp_server";
}

export async function activate(context: vscode.ExtensionContext): Promise<void> {

  const serverPath = path.join(
    context.extensionPath,
    "server",
    "target",
    "release",
    serverBinaryName()
  );

  // ✅ STEP 1: print paths
  console.log("RiscV LSP EXTENSION PATH:", context.extensionPath);
  console.log("RiscV LSP SERVER PATH:", serverPath);

  // ✅ STEP 2: check if file exists
  if (!fs.existsSync(serverPath)) {
    vscode.window.showErrorMessage("❌ RiscV LSP binary NOT FOUND: " + serverPath);
    return;
  } else {
    vscode.window.showInformationMessage("✅ RiscV LSP binary FOUND");
  }

  const run: Executable = {
    command: serverPath,
  };

  const serverOptions: ServerOptions = {
    run,
    debug: run,
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "riscvasm" }],
    outputChannelName: "RISC-V LSP",
  };

  client = new LanguageClient(
    "rustRiscVLsp",
    "Rust RiscV LSP",
    serverOptions,
    clientOptions
  );

  console.log("RISC-V extension starting...");

  try {
    await client.start();
    vscode.window.showInformationMessage("✅ RiscV LSP started");
  } catch (err) {
    vscode.window.showErrorMessage("❌ Failed to start RiscV LSP: " + err);
    console.error(err);
  }

  // ✅ IMPORTANT
  context.subscriptions.push(client);
}

export async function deactivate(): Promise<void> {
  if (!client) {
    return;
  }

  await client.stop();
}
