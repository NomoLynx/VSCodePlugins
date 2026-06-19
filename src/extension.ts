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

let outputChannel: vscode.OutputChannel;

function serverBinaryName(): string {
  return process.platform === "win32"
    ? "rust_keyword_lsp_server.exe"
    : "rust_keyword_lsp_server";
}

function debuggerBinaryName(): string {
  return process.platform === "win32"
    ? "riscv_debug_adapter.exe"
    : "riscv_debug_adapter";
}

class RiscvDebugAdapterFactory
  implements vscode.DebugAdapterDescriptorFactory {

  constructor(
    private readonly context: vscode.ExtensionContext,
    private readonly output: vscode.OutputChannel
  ) {}

  createDebugAdapterDescriptor(
    session: vscode.DebugSession
  ): vscode.ProviderResult<vscode.DebugAdapterDescriptor> {

    const debuggerPath = path.join(
      this.context.extensionPath,
      "debugger",
      "target",
      "release",
      "riscv_debug_adapter.exe"
    );

    this.output.appendLine("Launching debugger:");
    this.output.appendLine(debuggerPath);

    if (!fs.existsSync(debuggerPath)) {
      this.output.appendLine("❌ debugger not found");
      vscode.window.showErrorMessage("Debugger not found");
      return;
    }

    const logDir = path.join(
        this.context.extensionPath,
        "logs"
    );

    fs.mkdirSync(logDir, { recursive: true });

    const logFile = path.join(
        logDir,
        "riscv_debugger.log"
    );

    return new vscode.DebugAdapterExecutable(
        debuggerPath,
        [
            "--log-file",
            logFile
        ]
    );
  }
}

export async function activate(context: vscode.ExtensionContext): Promise<void> {

  outputChannel = vscode.window.createOutputChannel("RISC-V Debugger");
  outputChannel.appendLine("Debugger extension activated");

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

  context.subscriptions.push(
    vscode.debug.registerDebugAdapterDescriptorFactory(
      "riscv",
      new RiscvDebugAdapterFactory(context, outputChannel)
    )
  );

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
