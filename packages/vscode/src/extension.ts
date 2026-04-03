import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

export async function activate(
  context: vscode.ExtensionContext,
): Promise<void> {
  const config = vscode.workspace.getConfiguration("rustledger");
  const command = config.get<string>("server.path", "rledger-lsp");
  const extraArgs = config.get<string[]>("server.extraArgs", []);
  const journalFile = config.get<string>("journalFile", "");

  const serverOptions: ServerOptions = {
    command,
    args: extraArgs,
  };

  const initializationOptions: Record<string, string> = {};
  if (journalFile) {
    initializationOptions.journalFile = journalFile;
  }

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "beancount" }],
    synchronize: {
      fileEvents: vscode.workspace.createFileSystemWatcher("**/*.beancount"),
    },
    initializationOptions,
  };

  client = new LanguageClient(
    "rustledger",
    "rustledger",
    serverOptions,
    clientOptions,
  );

  await client.start();
  context.subscriptions.push({
    dispose: () => {
      client?.stop();
    },
  });
}

export async function deactivate(): Promise<void> {
  await client?.stop();
  client = undefined;
}
