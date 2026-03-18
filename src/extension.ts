import * as path from "path";
import * as vscode from "vscode";
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
  const serverPath = context.asAbsolutePath(
    path.join("server", "target", "debug", serverBinaryName())
  );

  const run: Executable = {
    command: serverPath,
  };

  const serverOptions: ServerOptions = {
    run,
    debug: run,
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "rkw" }],
  };

  client = new LanguageClient(
    "rustKeywordLsp",
    "Rust Keyword LSP",
    serverOptions,
    clientOptions
  );

  await client.start();
}

export async function deactivate(): Promise<void> {
  if (!client) {
    return;
  }

  await client.stop();
}
