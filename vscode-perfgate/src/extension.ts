import * as vscode from "vscode";
import { registerCommands } from "./commands";
import { PerfgateDiagnostics } from "./diagnostics";
import { PerfgateTaskProvider } from "./tasks";
import { PerfgateStatusBar } from "./statusBar";

let diagnostics: PerfgateDiagnostics;
let statusBar: PerfgateStatusBar;

export function activate(context: vscode.ExtensionContext): void {
  const outputChannel = vscode.window.createOutputChannel("perfgate");
  context.subscriptions.push(outputChannel);

  outputChannel.appendLine("perfgate extension activated");

  // Register task provider
  const taskProvider = new PerfgateTaskProvider();
  context.subscriptions.push(
    vscode.tasks.registerTaskProvider("perfgate", taskProvider)
  );

  // Initialize diagnostics
  diagnostics = new PerfgateDiagnostics(context, outputChannel);

  // Initialize status bar
  statusBar = new PerfgateStatusBar(context);

  // Register commands (they reference diagnostics and status bar)
  registerCommands(context, outputChannel, diagnostics, statusBar);

  // Watch for perfgate output files
  setupFileWatchers(context, outputChannel);

  outputChannel.appendLine("perfgate extension ready");
}

function setupFileWatchers(
  context: vscode.ExtensionContext,
  outputChannel: vscode.OutputChannel
): void {
  // Watch for report.json, compare.json, and sensor report files
  const reportPattern = new vscode.RelativePattern(
    vscode.workspace.workspaceFolders?.[0] ?? "",
    "**/{report,compare,perfgate.compare.v1,perfgate.report.v1}.json"
  );

  const watcher = vscode.workspace.createFileSystemWatcher(reportPattern);

  watcher.onDidChange((uri) => {
    outputChannel.appendLine(`perfgate output changed: ${uri.fsPath}`);
    diagnostics.refresh();
  });

  watcher.onDidCreate((uri) => {
    outputChannel.appendLine(`perfgate output created: ${uri.fsPath}`);
    diagnostics.refresh();
  });

  watcher.onDidDelete(() => {
    diagnostics.clear();
    statusBar.reset();
  });

  context.subscriptions.push(watcher);
}

export function deactivate(): void {
  diagnostics?.dispose();
  statusBar?.dispose();
}
