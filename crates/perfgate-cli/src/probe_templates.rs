//! Probe starter template generation.

use anyhow::Context;
use std::fs;
use std::path::Path;

use crate::{ProbeInitArgs, ProbeTemplate};

pub(crate) fn execute_probe_init(args: ProbeInitArgs) -> anyhow::Result<()> {
    let template = probe_template(args.template);
    fs::create_dir_all(&args.out_dir)
        .with_context(|| format!("create probe template directory {}", args.out_dir.display()))?;

    write_probe_template_file(
        &args.out_dir.join("README.md"),
        &render_probe_template_readme(&template, &args.out_dir),
        args.force,
    )?;
    write_probe_template_file(
        &args.out_dir.join("probes-baseline.jsonl"),
        &format!("{}\n", template.baseline_jsonl.join("\n")),
        args.force,
    )?;
    write_probe_template_file(
        &args.out_dir.join("probes-current.jsonl"),
        &format!("{}\n", template.current_jsonl.join("\n")),
        args.force,
    )?;
    write_probe_template_file(
        &args.out_dir.join("scenario.toml"),
        &render_probe_template_scenario(&template),
        args.force,
    )?;
    write_probe_template_file(
        &args.out_dir.join("tradeoff.toml"),
        &render_probe_template_tradeoff(&template),
        args.force,
    )?;

    eprintln!(
        "Probe starter template '{}' written to {}",
        template.slug,
        args.out_dir.display()
    );
    eprintln!("Review the generated snippets before adding them to perfgate.toml.");
    eprintln!(
        "Next: perfgate ingest probes --file {}/probes-current.jsonl --out artifacts/perfgate/probes-current.json",
        args.out_dir.display()
    );
    Ok(())
}

fn write_probe_template_file(path: &Path, content: &str, force: bool) -> anyhow::Result<()> {
    if path.exists() && !force {
        anyhow::bail!(
            "{} already exists; pass --force to overwrite generated probe starter files",
            path.display()
        );
    }

    fs::write(path, content).with_context(|| format!("write probe template {}", path.display()))
}

#[derive(Debug, Clone, Copy)]
struct ProbeStarterTemplate {
    slug: &'static str,
    title: &'static str,
    bench: &'static str,
    scenario: &'static str,
    workload: &'static str,
    dominant_probe: &'static str,
    local_probe: &'static str,
    support_probe: &'static str,
    baseline_jsonl: &'static [&'static str],
    current_jsonl: &'static [&'static str],
}

fn probe_template(template: ProbeTemplate) -> ProbeStarterTemplate {
    match template {
        ProbeTemplate::Parser => ProbeStarterTemplate {
            slug: "parser",
            title: "Parser pipeline probes",
            bench: "parser",
            scenario: "large_file_parse",
            workload: "large-file parser throughput",
            dominant_probe: "parser.batch_loop",
            local_probe: "parser.tokenize",
            support_probe: "parser.ast_build",
            baseline_jsonl: &[
                r#"{"probe":"parser.tokenize","scope":"local","wall_ms":12.4,"items":10000}"#,
                r#"{"probe":"parser.ast_build","scope":"local","wall_ms":28.2,"items":10000}"#,
                r#"{"probe":"parser.batch_loop","scope":"dominant","wall_ms":44.8,"items":10000}"#,
            ],
            current_jsonl: &[
                r#"{"probe":"parser.tokenize","scope":"local","wall_ms":12.8,"items":10000}"#,
                r#"{"probe":"parser.ast_build","scope":"local","wall_ms":27.5,"items":10000}"#,
                r#"{"probe":"parser.batch_loop","scope":"dominant","wall_ms":39.6,"items":10000}"#,
            ],
        },
        ProbeTemplate::Batch => ProbeStarterTemplate {
            slug: "batch",
            title: "Batch processing probes",
            bench: "batch",
            scenario: "batch_transform",
            workload: "batch transform throughput",
            dominant_probe: "batch.transform",
            local_probe: "batch.read_inputs",
            support_probe: "batch.write_outputs",
            baseline_jsonl: &[
                r#"{"probe":"batch.read_inputs","scope":"local","wall_ms":18.0,"items":5000}"#,
                r#"{"probe":"batch.transform","scope":"dominant","wall_ms":92.0,"items":5000}"#,
                r#"{"probe":"batch.write_outputs","scope":"local","wall_ms":26.0,"items":5000}"#,
            ],
            current_jsonl: &[
                r#"{"probe":"batch.read_inputs","scope":"local","wall_ms":18.4,"items":5000}"#,
                r#"{"probe":"batch.transform","scope":"dominant","wall_ms":82.0,"items":5000}"#,
                r#"{"probe":"batch.write_outputs","scope":"local","wall_ms":25.2,"items":5000}"#,
            ],
        },
        ProbeTemplate::Cli => ProbeStarterTemplate {
            slug: "cli",
            title: "CLI workflow probes",
            bench: "cli-command",
            scenario: "cli_user_path",
            workload: "CLI command response path",
            dominant_probe: "cli.execute",
            local_probe: "cli.parse_args",
            support_probe: "cli.render_output",
            baseline_jsonl: &[
                r#"{"probe":"cli.parse_args","scope":"local","wall_ms":3.2,"items":1}"#,
                r#"{"probe":"cli.execute","scope":"dominant","wall_ms":42.0,"items":1}"#,
                r#"{"probe":"cli.render_output","scope":"local","wall_ms":5.8,"items":1}"#,
            ],
            current_jsonl: &[
                r#"{"probe":"cli.parse_args","scope":"local","wall_ms":3.3,"items":1}"#,
                r#"{"probe":"cli.execute","scope":"dominant","wall_ms":37.8,"items":1}"#,
                r#"{"probe":"cli.render_output","scope":"local","wall_ms":5.9,"items":1}"#,
            ],
        },
        ProbeTemplate::Server => ProbeStarterTemplate {
            slug: "server",
            title: "Server request probes",
            bench: "server-request",
            scenario: "server_local_request",
            workload: "controlled local server request path",
            dominant_probe: "server.handle_request",
            local_probe: "server.decode_request",
            support_probe: "server.encode_response",
            baseline_jsonl: &[
                r#"{"probe":"server.decode_request","scope":"local","wall_ms":4.4,"items":100}"#,
                r#"{"probe":"server.handle_request","scope":"dominant","wall_ms":31.0,"items":100}"#,
                r#"{"probe":"server.encode_response","scope":"local","wall_ms":6.5,"items":100}"#,
            ],
            current_jsonl: &[
                r#"{"probe":"server.decode_request","scope":"local","wall_ms":4.5,"items":100}"#,
                r#"{"probe":"server.handle_request","scope":"dominant","wall_ms":27.0,"items":100}"#,
                r#"{"probe":"server.encode_response","scope":"local","wall_ms":6.7,"items":100}"#,
            ],
        },
    }
}

fn render_probe_template_readme(template: &ProbeStarterTemplate, out_dir: &Path) -> String {
    let out_dir = out_dir.display();
    format!(
        r#"# {title}

This starter shows a small probe set for {workload}. Review and edit the probe
names before using them in durable decision policy.

Generated files:

- `probes-baseline.jsonl`: reviewed baseline probe events
- `probes-current.jsonl`: current-run probe events for local experimentation
- `scenario.toml`: scenario snippet to copy into `perfgate.toml`
- `tradeoff.toml`: tradeoff snippet to copy into `perfgate.toml`

Next:

```bash
perfgate ingest probes --file {out_dir}/probes-baseline.jsonl --out baselines/probes.json
perfgate ingest probes --file {out_dir}/probes-current.jsonl --out artifacts/perfgate/probes-current.json
perfgate probe compare --baseline baselines/probes.json --current artifacts/perfgate/probes-current.json --out artifacts/perfgate/probe-compare.json
perfgate decision suggest --config perfgate.toml
```

Do not:

- keep probes that no reviewer can act on;
- use generated sample values as release proof;
- turn a temporary debugging span into a durable probe id.
"#,
        title = template.title,
        workload = template.workload,
        out_dir = out_dir
    )
}

fn render_probe_template_scenario(template: &ProbeStarterTemplate) -> String {
    format!(
        r#"# Copy into perfgate.toml after reviewing paths and names.
[[scenario]]
name = "{scenario}"
bench = "{bench}"
weight = 1.0
probe_baseline = "baselines/probes.json"
probe_current = "artifacts/perfgate/probes-current.json"
probe_compare = "artifacts/perfgate/probe-compare.json"
"#,
        scenario = template.scenario,
        bench = template.bench
    )
}

fn render_probe_template_tradeoff(template: &ProbeStarterTemplate) -> String {
    format!(
        r#"# Copy into perfgate.toml only when reviewers need a bounded tradeoff.
[[tradeoff]]
name = "{slug}-dominant-probe-improvement"
if_failed = "wall_ms"
downgrade_to = "warn"

[[tradeoff.require]]
metric = "wall_ms"
probe = "{dominant_probe}"
min_improvement_ratio = 1.10

[[tradeoff.allow]]
metric = "wall_ms"
probe = "{local_probe}"
max_regression = 0.03

# Supporting probe to keep visible in reviews: {support_probe}
"#,
        slug = template.slug,
        dominant_probe = template.dominant_probe,
        local_probe = template.local_probe,
        support_probe = template.support_probe
    )
}
