import * as vscode from "vscode";
import * as path from "path";
import * as fs from "fs";

interface PerfgateTaskDefinition extends vscode.TaskDefinition {
  task: string;
  bench?: string;
  config?: string;
  mode?: "standard" | "cockpit";
}

/**
 * Task provider that auto-detects perfgate.toml in the workspace and provides
 * tasks for check, run, and compare operations.
 *
 * Detected tasks:
 * - `perfgate: check --all` (cockpit mode, all benchmarks)
 * - `perfgate: check <name>` (one per [[bench]] entry in perfgate.toml)
 * - `perfgate: run <name>` (one per [[bench]] entry)
 */
export class PerfgateTaskProvider implements vscode.TaskProvider {
  static readonly type = "perfgate";

  async provideTasks(): Promise<vscode.Task[]> {
    const tasks: vscode.Task[] = [];
    const workspaceRoot = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
    if (!workspaceRoot) {
      return tasks;
    }

    const configRelPath =
      vscode.workspace.getConfiguration("perfgate").get<string>("configPath") ||
      "perfgate.toml";
    const configPath = path.join(workspaceRoot, configRelPath);

    try {
      await fs.promises.access(configPath);
    } catch {
      return tasks; // No config file found
    }

    const binary =
      vscode.workspace.getConfiguration("perfgate").get<string>("binaryPath") ||
      "perfgate";

    // Parse bench names from config
    const benchNames = await this.parseBenchNames(configPath);

    // Create "check --all" task (cockpit mode)
    if (benchNames.length > 0) {
      tasks.push(
        this.createTask(
          {
            type: "perfgate",
            task: "check",
            config: configRelPath,
            mode: "cockpit",
          },
          "check --all",
          binary,
          ["check", "--config", configRelPath, "--all", "--mode", "cockpit"],
          workspaceRoot
        )
      );
    }

    // Per-bench tasks
    for (const name of benchNames) {
      // check task
      tasks.push(
        this.createTask(
          {
            type: "perfgate",
            task: "check",
            bench: name,
            config: configRelPath,
          },
          `check: ${name}`,
          binary,
          ["check", "--config", configRelPath, "--bench", name],
          workspaceRoot
        )
      );

      // run task
      tasks.push(
        this.createTask(
          {
            type: "perfgate",
            task: "run",
            bench: name,
            config: configRelPath,
          },
          `run: ${name}`,
          binary,
          [
            "run",
            "--name",
            name,
            "--out",
            `artifacts/perfgate/${name}/run.json`,
            "--",
            "echo",
            "placeholder",
          ],
          workspaceRoot
        )
      );
    }

    // Generic compare task
    tasks.push(
      this.createTask(
        { type: "perfgate", task: "compare" },
        "compare",
        binary,
        [
          "compare",
          "--baseline",
          "${input:baseline}",
          "--current",
          "${input:current}",
          "--out",
          "artifacts/perfgate/compare.json",
        ],
        workspaceRoot
      )
    );

    return tasks;
  }

  resolveTask(task: vscode.Task): vscode.Task | undefined {
    const definition = task.definition as PerfgateTaskDefinition;
    if (!definition.task) {
      return undefined;
    }

    const workspaceRoot = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
    if (!workspaceRoot) {
      return undefined;
    }

    const binary =
      vscode.workspace.getConfiguration("perfgate").get<string>("binaryPath") ||
      "perfgate";
    const config = definition.config || "perfgate.toml";

    let args: string[];
    switch (definition.task) {
      case "check":
        args = ["check", "--config", config];
        if (definition.bench) {
          args.push("--bench", definition.bench);
        }
        if (definition.mode === "cockpit") {
          args.push("--mode", "cockpit");
        }
        break;
      case "run":
        args = [
          "run",
          "--name",
          definition.bench || "unnamed",
          "--out",
          "run.json",
        ];
        break;
      case "compare":
        args = ["compare"];
        break;
      default:
        return undefined;
    }

    return this.createTask(
      definition,
      task.name,
      binary,
      args,
      workspaceRoot
    );
  }

  private createTask(
    definition: PerfgateTaskDefinition,
    name: string,
    binary: string,
    args: string[],
    cwd: string
  ): vscode.Task {
    const task = new vscode.Task(
      definition,
      vscode.TaskScope.Workspace,
      name,
      "perfgate",
      new vscode.ShellExecution(binary, args, { cwd }),
      "$perfgate"
    );

    task.group = vscode.TaskGroup.Test;
    task.presentationOptions = {
      reveal: vscode.TaskRevealKind.Always,
      panel: vscode.TaskPanelKind.Shared,
      clear: true,
    };

    return task;
  }

  private async parseBenchNames(configPath: string): Promise<string[]> {
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
}
