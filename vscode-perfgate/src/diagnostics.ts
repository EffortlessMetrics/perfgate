import * as vscode from "vscode";
import * as path from "path";
import * as fs from "fs";

/**
 * Verdict status from perfgate compare/report JSON receipts.
 */
interface PerfgateVerdict {
  status: "pass" | "warn" | "fail" | "skip";
  counts: {
    pass: number;
    warn: number;
    fail: number;
    skip: number;
  };
  reasons: string[];
}

/**
 * Delta for a single metric from a compare receipt.
 */
interface PerfgateDelta {
  baseline: number;
  current: number;
  pct: number;
  regression: number;
  status: "pass" | "warn" | "fail" | "skip";
  statistic?: string;
  cv?: number;
  noise_threshold?: number;
}

/**
 * Simplified shape of a perfgate compare receipt (perfgate.compare.v1).
 */
interface CompareReceipt {
  schema: string;
  bench: {
    name: string;
  };
  deltas: Record<string, PerfgateDelta>;
  verdict: PerfgateVerdict;
}

/**
 * Simplified shape of a sensor report (sensor.report.v1).
 */
interface SensorReport {
  schema: string;
  verdict?: {
    status: string;
  };
  checks?: Array<{
    check_id: string;
    status: string;
    findings?: Array<{
      code: string;
      severity: string;
      message: string;
    }>;
  }>;
}

/**
 * Manages VS Code diagnostics derived from perfgate output files.
 *
 * Watches for compare.json and report.json files in the workspace and
 * translates budget verdicts into VS Code Problems panel entries. Diagnostic
 * entries are mapped to the corresponding benchmark definition line in
 * perfgate.toml whenever possible.
 */
export class PerfgateDiagnostics {
  private readonly collection: vscode.DiagnosticCollection;
  private readonly outputChannel: vscode.OutputChannel;

  constructor(
    context: vscode.ExtensionContext,
    outputChannel: vscode.OutputChannel
  ) {
    this.collection = vscode.languages.createDiagnosticCollection("perfgate");
    this.outputChannel = outputChannel;
    context.subscriptions.push(this.collection);

    // Perform initial scan
    const autoRefresh =
      vscode.workspace.getConfiguration("perfgate").get<boolean>("autoRefreshDiagnostics") ?? true;
    if (autoRefresh) {
      this.refresh();
    }
  }

  /**
   * Refresh all diagnostics by scanning for perfgate output files.
   */
  async refresh(): Promise<void> {
    this.collection.clear();

    const root = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
    if (!root) {
      return;
    }

    const artifactDir =
      vscode.workspace.getConfiguration("perfgate").get<string>("artifactDir") ||
      "artifacts/perfgate";

    // Attempt to load compare receipts from multiple locations
    const compareFiles = [
      path.join(root, artifactDir, "compare.json"),
      path.join(root, artifactDir, "extras", "perfgate.compare.v1.json"),
    ];

    // Also look for cockpit multi-bench extras
    const extrasDir = path.join(root, artifactDir, "extras");
    try {
      const entries = await fs.promises.readdir(extrasDir, {
        withFileTypes: true,
      });
      for (const entry of entries) {
        if (entry.isDirectory()) {
          compareFiles.push(
            path.join(extrasDir, entry.name, "perfgate.compare.v1.json")
          );
        }
      }
    } catch {
      // extras dir may not exist
    }

    // Also search workspace root for any compare files
    try {
      const files = await vscode.workspace.findFiles(
        "**/perfgate.compare.v1.json",
        "**/node_modules/**",
        20
      );
      for (const file of files) {
        if (!compareFiles.includes(file.fsPath)) {
          compareFiles.push(file.fsPath);
        }
      }
    } catch {
      // ignore search errors
    }

    const configPath = path.join(
      root,
      vscode.workspace.getConfiguration("perfgate").get<string>("configPath") ||
        "perfgate.toml"
    );

    // Parse config file to find bench line numbers
    const benchLines = await this.parseBenchLines(configPath);

    for (const compareFile of compareFiles) {
      await this.processCompareFile(compareFile, configPath, benchLines);
    }

    // Also check for sensor reports (cockpit mode)
    const reportFiles = [
      path.join(root, artifactDir, "report.json"),
    ];

    for (const reportFile of reportFiles) {
      await this.processSensorReport(reportFile, configPath, benchLines);
    }
  }

  /**
   * Parse perfgate.toml to find the line number for each [[bench]] name entry.
   */
  private async parseBenchLines(
    configPath: string
  ): Promise<Map<string, number>> {
    const result = new Map<string, number>();

    try {
      const content = await fs.promises.readFile(configPath, "utf-8");
      const lines = content.split("\n");

      for (let i = 0; i < lines.length; i++) {
        const match = lines[i].match(/^\s*name\s*=\s*"([^"]+)"/);
        if (match) {
          result.set(match[1], i); // 0-indexed line number
        }
      }
    } catch {
      // Config file may not exist
    }

    return result;
  }

  /**
   * Process a single compare receipt file and emit diagnostics.
   */
  private async processCompareFile(
    filePath: string,
    configPath: string,
    benchLines: Map<string, number>
  ): Promise<void> {
    try {
      const content = await fs.promises.readFile(filePath, "utf-8");
      const receipt: CompareReceipt = JSON.parse(content);

      if (!receipt.schema?.startsWith("perfgate.compare")) {
        return;
      }

      const diagnosticsMap = new Map<string, vscode.Diagnostic[]>();
      const configUri = configPath;

      const benchName = receipt.bench?.name;
      if (!benchName) {
        return;
      }

      const line = benchLines.get(benchName) ?? 0;
      const range = new vscode.Range(line, 0, line, 999);

      // Process each metric delta
      if (receipt.deltas) {
        for (const [metric, delta] of Object.entries(receipt.deltas)) {
          if (delta.status === "pass" || delta.status === "skip") {
            continue;
          }

          const severity =
            delta.status === "fail"
              ? vscode.DiagnosticSeverity.Error
              : vscode.DiagnosticSeverity.Warning;

          const pctStr =
            delta.pct !== undefined
              ? `${delta.pct > 0 ? "+" : ""}${(delta.pct * 100).toFixed(1)}%`
              : "N/A";

          const message = `perfgate: ${benchName} ${metric} regression ${pctStr} (baseline: ${delta.baseline}, current: ${delta.current})`;

          const diagnostic = new vscode.Diagnostic(range, message, severity);
          diagnostic.source = "perfgate";
          diagnostic.code = `${metric}_${delta.status}`;

          if (!diagnosticsMap.has(configUri)) {
            diagnosticsMap.set(configUri, []);
          }
          diagnosticsMap.get(configUri)!.push(diagnostic);
        }
      }

      // Process overall verdict
      if (
        receipt.verdict?.status === "fail" ||
        receipt.verdict?.status === "warn"
      ) {
        const severity =
          receipt.verdict.status === "fail"
            ? vscode.DiagnosticSeverity.Error
            : vscode.DiagnosticSeverity.Warning;

        for (const reason of receipt.verdict.reasons || []) {
          const diagnostic = new vscode.Diagnostic(
            range,
            `perfgate: ${benchName}: ${reason}`,
            severity
          );
          diagnostic.source = "perfgate";
          diagnostic.code = reason;

          if (!diagnosticsMap.has(configUri)) {
            diagnosticsMap.set(configUri, []);
          }
          diagnosticsMap.get(configUri)!.push(diagnostic);
        }
      }

      // Apply diagnostics to config file
      for (const [filePath, diags] of diagnosticsMap) {
        const uri = vscode.Uri.file(filePath);
        const existing = this.collection.get(uri) ?? [];
        this.collection.set(uri, [...existing, ...diags]);
      }
    } catch (e) {
      // File may not exist or may not be valid JSON
      this.outputChannel.appendLine(
        `perfgate: could not process ${filePath}: ${e}`
      );
    }
  }

  /**
   * Process a sensor report file (cockpit mode) and emit diagnostics.
   */
  private async processSensorReport(
    filePath: string,
    configPath: string,
    benchLines: Map<string, number>
  ): Promise<void> {
    try {
      const content = await fs.promises.readFile(filePath, "utf-8");
      const report: SensorReport = JSON.parse(content);

      if (!report.schema?.startsWith("sensor.report")) {
        return;
      }

      if (!report.checks) {
        return;
      }

      const configUri = configPath;
      const diagnostics: vscode.Diagnostic[] = [];

      for (const check of report.checks) {
        if (check.status === "pass" || check.status === "skip") {
          continue;
        }

        for (const finding of check.findings ?? []) {
          if (finding.severity === "info") {
            continue;
          }

          const severity =
            finding.severity === "error"
              ? vscode.DiagnosticSeverity.Error
              : vscode.DiagnosticSeverity.Warning;

          // Try to find bench name from finding message
          const benchMatch = finding.message.match(
            /(?:bench\s+)?['"]?([^'":\s]+)['"]?/
          );
          const benchName = benchMatch?.[1];
          const line = benchName ? benchLines.get(benchName) ?? 0 : 0;
          const range = new vscode.Range(line, 0, line, 999);

          const diagnostic = new vscode.Diagnostic(
            range,
            `perfgate: ${finding.message}`,
            severity
          );
          diagnostic.source = "perfgate";
          diagnostic.code = finding.code;

          diagnostics.push(diagnostic);
        }
      }

      if (diagnostics.length > 0) {
        const uri = vscode.Uri.file(configUri);
        const existing = this.collection.get(uri) ?? [];
        this.collection.set(uri, [...existing, ...diagnostics]);
      }
    } catch {
      // File may not exist or may not be valid JSON
    }
  }

  /**
   * Clear all diagnostics.
   */
  clear(): void {
    this.collection.clear();
  }

  dispose(): void {
    this.collection.dispose();
  }
}
