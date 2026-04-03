import { execFileSync } from "child_process";
import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";

const INSTALL_URL = "https://rustledger.github.io/getting-started/installation.html";

let client: LanguageClient | undefined;

function findBinary(command: string): boolean {
  try {
    execFileSync(command, ["--version"], { stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
}

export async function activate(
  context: vscode.ExtensionContext,
): Promise<void> {
  const config = vscode.workspace.getConfiguration("rustledger");
  const command = config.get<string>("server.path", "rledger-lsp");
  const extraArgs = config.get<string[]>("server.extraArgs", []);
  const journalFile = config.get<string>("journalFile", "");

  if (!findBinary(command)) {
    const install = "Install";
    const result = await vscode.window.showWarningMessage(
      `Could not find "${command}". Install rustledger to enable language features.`,
      install,
    );
    if (result === install) {
      vscode.env.openExternal(vscode.Uri.parse(INSTALL_URL));
    }
    return;
  }

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
      fileEvents:
        vscode.workspace.createFileSystemWatcher("**/*.{beancount,bean}"),
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
