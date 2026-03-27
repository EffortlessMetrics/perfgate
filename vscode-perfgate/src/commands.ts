import * as vscode from "vscode";
import * as path from "path";
import * as fs from "fs";
import { PerfgateDiagnostics } from "./diagnostics";
import { PerfgateStatusBar } from "./statusBar";

export function registerCommands(
  context: vscode.ExtensionContext,
  outputChannel: vscode.OutputChannel,
  diagnostics: PerfgateDiagnostics,
  statusBar: PerfgateStatusBar
): void {
  context.subscriptions.push(
    vscode.commands.registerCommand("perfgate.check", () =>
      runCheck(outputChannel, diagnostics, statusBar)
    ),
    vscode.commands.registerCommand("perfgate.run", () =>
      runBenchmark(outputChannel)
    ),
    vscode.commands.registerCommand("perfgate.compare", () =>
      runCompare(outputChannel)
    ),
    vscode.commands.registerCommand("perfgate.viewReport", () =>
      viewReport(outputChannel)
    ),
    vscode.commands.registerCommand("perfgate.openDashboard", () =>
      openDashboard()
    ),
    vscode.commands.registerCommand("perfgate.pickBench", () => pickBench())
  );
}

function getConfig(): vscode.WorkspaceConfiguration {
  return vscode.workspace.getConfiguration("perfgate");
}

function getBinaryPath(): string {
  return getConfig().get<string>("binaryPath") || "perfgate";
}

function getConfigPath(): string {
  return getConfig().get<string>("configPath") || "perfgate.toml";
}

function getWorkspaceRoot(): string | undefined {
  return vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
}

/**
 * Parse perfgate.toml to extract bench names for quick-pick.
 */
async function getBenchNames(): Promise<string[]> {
  const root = getWorkspaceRoot();
  if (!root) {
    return [];
  }

  const configPath = path.join(root, getConfigPath());
  try {
    const content = await fs.promises.readFile(configPath, "utf-8");
    const names: string[] = [];
    const regex = /^\s*name\s*=\s*"([^"]+)"/gm;
    let match;
    while ((match = regex.exec(content)) !== null) {
      names.push(match[1]);
    }
    return names;
  } catch {
    return [];
  }
}

async function pickBench(): Promise<string | undefined> {
  const names = await getBenchNames();
  if (names.length === 0) {
    vscode.window.showInformationMessage(
      "No benchmarks found in perfgate.toml."
    );
    return undefined;
  }

  return vscode.window.showQuickPick(names, {
    placeHolder: "Select a benchmark",
  });
}

async function runCheck(
  outputChannel: vscode.OutputChannel,
  diagnostics: PerfgateDiagnostics,
  statusBar: PerfgateStatusBar
): Promise<void> {
  const root = getWorkspaceRoot();
  if (!root) {
    vscode.window.showErrorMessage("No workspace folder open.");
    return;
  }

  const bench = await pickBench();
  if (!bench) {
    return;
  }

  statusBar.setRunning();

  const binary = getBinaryPath();
  const config = getConfigPath();

  const task = new vscode.Task(
    { type: "perfgate", task: "check", bench },
    vscode.TaskScope.Workspace,
    `check: ${bench}`,
    "perfgate",
    new vscode.ShellExecution(binary, ["check", "--config", config, "--bench", bench], {
      cwd: root,
    }),
    "$perfgate"
  );

  task.presentationOptions = {
    reveal: vscode.TaskRevealKind.Always,
    panel: vscode.TaskPanelKind.Shared,
  };

  const execution = await vscode.tasks.executeTask(task);

  // Listen for task end to update status
  const disposable = vscode.tasks.onDidEndTaskProcess((e) => {
    if (e.execution === execution) {
      const exitCode = e.exitCode ?? 1;
      if (exitCode === 0) {
        statusBar.setVerdict("pass");
      } else if (exitCode === 2) {
        statusBar.setVerdict("fail");
      } else if (exitCode === 3) {
        statusBar.setVerdict("warn");
      } else {
        statusBar.setVerdict("error");
      }

      // Refresh diagnostics after check completes
      diagnostics.refresh();
      disposable.dispose();
    }
  });
}

async function runBenchmark(outputChannel: vscode.OutputChannel): Promise<void> {
  const root = getWorkspaceRoot();
  if (!root) {
    vscode.window.showErrorMessage("No workspace folder open.");
    return;
  }

  const bench = await pickBench();
  if (!bench) {
    return;
  }

  const binary = getBinaryPath();
  const outFile = path.join("artifacts", "perfgate", "run.json");

  const commandInput = await vscode.window.showInputBox({
    prompt: "Enter the benchmark command to execute",
    placeHolder: "./scripts/bench.sh",
  });

  if (!commandInput) {
    return;
  }

  const task = new vscode.Task(
    { type: "perfgate", task: "run", bench },
    vscode.TaskScope.Workspace,
    `run: ${bench}`,
    "perfgate",
    new vscode.ShellExecution(
      binary,
      ["run", "--name", bench, "--out", outFile, "--", ...commandInput.split(" ")],
      { cwd: root }
    )
  );

  await vscode.tasks.executeTask(task);
}

async function runCompare(outputChannel: vscode.OutputChannel): Promise<void> {
  const root = getWorkspaceRoot();
  if (!root) {
    vscode.window.showErrorMessage("No workspace folder open.");
    return;
  }

  const binary = getBinaryPath();

  const baselineUri = await vscode.window.showOpenDialog({
    canSelectFiles: true,
    canSelectFolders: false,
    canSelectMany: false,
    filters: { JSON: ["json"] },
    openLabel: "Select baseline",
    defaultUri: vscode.Uri.file(root),
  });

  if (!baselineUri || baselineUri.length === 0) {
    return;
  }

  const currentUri = await vscode.window.showOpenDialog({
    canSelectFiles: true,
    canSelectFolders: false,
    canSelectMany: false,
    filters: { JSON: ["json"] },
    openLabel: "Select current",
    defaultUri: vscode.Uri.file(root),
  });

  if (!currentUri || currentUri.length === 0) {
    return;
  }

  const outFile = path.join("artifacts", "perfgate", "compare.json");

  const task = new vscode.Task(
    { type: "perfgate", task: "compare" },
    vscode.TaskScope.Workspace,
    "compare",
    "perfgate",
    new vscode.ShellExecution(
      binary,
      [
        "compare",
        "--baseline",
        baselineUri[0].fsPath,
        "--current",
        currentUri[0].fsPath,
        "--out",
        outFile,
      ],
      { cwd: root }
    ),
    "$perfgate"
  );

  await vscode.tasks.executeTask(task);
}

async function viewReport(outputChannel: vscode.OutputChannel): Promise<void> {
  const root = getWorkspaceRoot();
  if (!root) {
    vscode.window.showErrorMessage("No workspace folder open.");
    return;
  }

  const artifactDir =
    getConfig().get<string>("artifactDir") || "artifacts/perfgate";

  // Look for report files in priority order
  const candidates = [
    path.join(root, artifactDir, "report.json"),
    path.join(root, artifactDir, "compare.json"),
    path.join(root, artifactDir, "extras", "perfgate.report.v1.json"),
    path.join(root, artifactDir, "extras", "perfgate.compare.v1.json"),
  ];

  for (const candidate of candidates) {
    try {
      await fs.promises.access(candidate);
      const doc = await vscode.workspace.openTextDocument(candidate);
      await vscode.window.showTextDocument(doc, { preview: true });
      return;
    } catch {
      // Try next candidate
    }
  }

  // Fallback: let user pick a file
  const uri = await vscode.window.showOpenDialog({
    canSelectFiles: true,
    canSelectFolders: false,
    canSelectMany: false,
    filters: { JSON: ["json"] },
    openLabel: "Select perfgate report",
    defaultUri: vscode.Uri.file(root),
  });

  if (uri && uri.length > 0) {
    const doc = await vscode.workspace.openTextDocument(uri[0]);
    await vscode.window.showTextDocument(doc, { preview: true });
  }
}

async function openDashboard(): Promise<void> {
  const serverUrl = getConfig().get<string>("serverUrl");
  if (!serverUrl) {
    const action = await vscode.window.showWarningMessage(
      "No perfgate server URL configured. Set perfgate.serverUrl in settings.",
      "Open Settings"
    );
    if (action === "Open Settings") {
      await vscode.commands.executeCommand(
        "workbench.action.openSettings",
        "perfgate.serverUrl"
      );
    }
    return;
  }

  // Trim /api/v1 suffix if present to get dashboard URL
  const dashboardUrl = serverUrl.replace(/\/api\/v\d+\/?$/, "");
  vscode.env.openExternal(vscode.Uri.parse(dashboardUrl));
}
