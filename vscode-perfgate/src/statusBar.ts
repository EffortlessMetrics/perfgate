import * as vscode from "vscode";

export type VerdictState = "pass" | "warn" | "fail" | "error" | "running" | "none";

/**
 * Status bar item that shows the last perfgate verdict at the bottom of the
 * VS Code window.
 *
 * States:
 * - `none`    - no verdict yet (hidden)
 * - `running` - a perfgate task is executing
 * - `pass`    - last check passed (green)
 * - `warn`    - last check warned (yellow)
 * - `fail`    - last check failed (red)
 * - `error`   - runtime error (red)
 */
export class PerfgateStatusBar {
  private readonly item: vscode.StatusBarItem;
  private state: VerdictState = "none";

  constructor(context: vscode.ExtensionContext) {
    this.item = vscode.window.createStatusBarItem(
      vscode.StatusBarAlignment.Left,
      50
    );
    this.item.command = "perfgate.check";
    this.item.tooltip = "Click to run perfgate check";
    context.subscriptions.push(this.item);

    // Show item if there is a perfgate.toml in the workspace
    this.detectAndShow();
  }

  private async detectAndShow(): Promise<void> {
    const files = await vscode.workspace.findFiles(
      "**/perfgate.toml",
      "**/node_modules/**",
      1
    );
    if (files.length > 0) {
      this.updateDisplay();
      this.item.show();
    }
  }

  setVerdict(verdict: "pass" | "warn" | "fail" | "error"): void {
    this.state = verdict;
    this.updateDisplay();
    this.item.show();
  }

  setRunning(): void {
    this.state = "running";
    this.updateDisplay();
    this.item.show();
  }

  reset(): void {
    this.state = "none";
    this.updateDisplay();
  }

  getState(): VerdictState {
    return this.state;
  }

  private updateDisplay(): void {
    switch (this.state) {
      case "pass":
        this.item.text = "$(check) perfgate: pass";
        this.item.backgroundColor = undefined;
        this.item.color = new vscode.ThemeColor(
          "statusBarItem.foreground"
        );
        this.item.tooltip = "Last perfgate check passed. Click to re-run.";
        break;
      case "warn":
        this.item.text = "$(warning) perfgate: warn";
        this.item.backgroundColor = new vscode.ThemeColor(
          "statusBarItem.warningBackground"
        );
        this.item.color = undefined;
        this.item.tooltip =
          "Last perfgate check had warnings. Click to re-run.";
        break;
      case "fail":
        this.item.text = "$(error) perfgate: fail";
        this.item.backgroundColor = new vscode.ThemeColor(
          "statusBarItem.errorBackground"
        );
        this.item.color = undefined;
        this.item.tooltip =
          "Last perfgate check failed (budget violated). Click to re-run.";
        break;
      case "error":
        this.item.text = "$(error) perfgate: error";
        this.item.backgroundColor = new vscode.ThemeColor(
          "statusBarItem.errorBackground"
        );
        this.item.color = undefined;
        this.item.tooltip =
          "Last perfgate check encountered a runtime error. Click to re-run.";
        break;
      case "running":
        this.item.text = "$(sync~spin) perfgate: running...";
        this.item.backgroundColor = undefined;
        this.item.color = undefined;
        this.item.tooltip = "perfgate check is running...";
        break;
      case "none":
      default:
        this.item.text = "$(beaker) perfgate";
        this.item.backgroundColor = undefined;
        this.item.color = undefined;
        this.item.tooltip = "Click to run perfgate check.";
        break;
    }
  }

  dispose(): void {
    this.item.dispose();
  }
}
