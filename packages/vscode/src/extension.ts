import { execFileSync } from "child_process";
import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";

const INSTALL_URL =
  "https://rustledger.github.io/getting-started/installation.html";

let client: LanguageClient | undefined;
let outputChannel: vscode.OutputChannel | undefined;

function findBinary(command: string): boolean {
  try {
    execFileSync(command, ["--version"], { stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
}

async function startClient(context: vscode.ExtensionContext): Promise<boolean> {
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
    return false;
  }

  const serverOptions: ServerOptions = {
    command,
    args: extraArgs,
  };

  const initializationOptions: Record<string, string> = {};
  if (journalFile) {
    initializationOptions.journalFile = journalFile;
  }

  if (!outputChannel) {
    outputChannel = vscode.window.createOutputChannel("rustledger", {
      log: true,
    });
    context.subscriptions.push(outputChannel);
  }

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "beancount" }],
    synchronize: {
      fileEvents:
        vscode.workspace.createFileSystemWatcher("**/*.{beancount,bean}"),
    },
    initializationOptions,
    outputChannel,
  };

  client = new LanguageClient(
    "rustledger",
    "rustledger",
    serverOptions,
    clientOptions,
  );

  await client.start();
  outputChannel.appendLine(`Started rledger-lsp: ${command}`);
  return true;
}

export async function activate(
  context: vscode.ExtensionContext,
): Promise<void> {
  // Register restart command
  const restartCommand = vscode.commands.registerCommand(
    "rustledger.restartServer",
    async () => {
      outputChannel?.appendLine("Restarting rledger-lsp...");
      if (client) {
        await client.stop();
        client = undefined;
      }
      await startClient(context);
    },
  );
  context.subscriptions.push(restartCommand);

  // Start the client
  const started = await startClient(context);
  if (started) {
    context.subscriptions.push({
      dispose: () => {
        client?.stop();
      },
    });
  }
}

export async function deactivate(): Promise<void> {
  await client?.stop();
  client = undefined;
}
