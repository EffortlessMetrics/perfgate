//! Optional server-ledger readiness checks.

use anyhow::Context;
use std::time::Duration;

use crate::doctor::ensure_artifact_dir_writable;
use crate::storage::with_tokio_runtime;
use crate::{BASELINE_SERVER_NOT_CONFIGURED, LedgerAction, LedgerDoctorArgs, ServerFlags};
use perfgate_client::{
    BaselineClient, ClientConfig, ListDecisionsQuery, PruneDecisionsRequest, ResolvedServerConfig,
    RetryConfig,
};
use perfgate_types::ConfigFile;
use perfgate_types::config::load_config_file;

pub(crate) fn execute_ledger_action(
    action: LedgerAction,
    server_flags: &ServerFlags,
) -> anyhow::Result<()> {
    match action {
        LedgerAction::Doctor(args) => execute_ledger_doctor(args, server_flags),
    }
}

fn execute_ledger_doctor(args: LedgerDoctorArgs, server_flags: &ServerFlags) -> anyhow::Result<()> {
    let config = if args.config.exists() {
        load_config_file(&args.config)
            .with_context(|| format!("load ledger doctor config {}", args.config.display()))?
    } else {
        ConfigFile::default()
    };
    let server_config = server_flags.resolve(&config.baseline_server);

    println!("perfgate ledger doctor");
    print_ledger_readiness_line(
        "Local receipts",
        match ensure_artifact_dir_writable(&args.out_dir) {
            Ok(()) => format!("ready ({} writable)", args.out_dir.display()),
            Err(error) => format!(
                "not ready ({} not writable: {error})",
                args.out_dir.display()
            ),
        },
    );

    if !server_config.is_configured() {
        print_ledger_readiness_line("Server URL", "missing");
        print_ledger_readiness_line("API key", "not configured");
        print_ledger_readiness_line("Project", "not configured");
        print_ledger_readiness_line(
            "Upload mode",
            "local receipts only; server ledger is optional team history",
        );
        print_ledger_readiness_line("History", "not checked; no server URL configured");
        print_ledger_readiness_line("Export", "not checked; no server URL configured");
        print_ledger_readiness_line("Prune dry-run", "not checked; no server URL configured");
        print_ledger_next(&[
            "You do not need server mode yet.",
            "Use `perfgate decision bundle` when decision evidence needs to travel.",
        ]);
        print_ledger_do_not();
        return Ok(());
    }

    let url = server_config
        .url
        .as_deref()
        .expect("checked by is_configured");
    print_ledger_readiness_line("Server URL", format!("configured ({url})"));
    print_ledger_readiness_line(
        "API key",
        if server_config.api_key.is_some() {
            "present"
        } else {
            "missing; okay only for local unauthenticated server mode"
        },
    );
    let project = server_config.project.as_deref();
    print_ledger_readiness_line("Project", project.unwrap_or("missing"));
    print_ledger_readiness_line(
        "Upload mode",
        if server_config.fallback_to_local {
            "advisory (fallback_to_local = true); local receipts remain primary"
        } else {
            "server operations are explicit; local receipts remain primary"
        },
    );

    if args.offline {
        print_ledger_readiness_line("Health", "not checked (--offline)");
        print_ledger_readiness_line("History", "not checked (--offline)");
        print_ledger_readiness_line("Export", "not checked (--offline)");
        print_ledger_readiness_line("Prune dry-run", "not checked (--offline)");
        print_ledger_next(&["Run without `--offline` when you want reachability checks."]);
        print_ledger_do_not();
        return Ok(());
    }

    let client = match ledger_doctor_client(&server_config) {
        Ok(client) => client,
        Err(error) => {
            print_ledger_readiness_line("Health", format!("not checkable: {error:#}"));
            print_ledger_readiness_line("History", "not checked; client setup failed");
            print_ledger_readiness_line("Export", "not checked; client setup failed");
            print_ledger_readiness_line("Prune dry-run", "not checked; client setup failed");
            print_ledger_next(&["Fix the server URL or use local receipts only."]);
            print_ledger_do_not();
            return Ok(());
        }
    };

    match with_tokio_runtime(async { client.health_check().await.map_err(anyhow::Error::from) }) {
        Ok(health) if health.status == "healthy" => {
            print_ledger_readiness_line("Health", "reachable");
        }
        Ok(health) => {
            print_ledger_readiness_line(
                "Health",
                format!("unhealthy response ({})", health.status),
            );
        }
        Err(error) => {
            print_ledger_readiness_line("Health", format!("unreachable: {error:#}"));
            print_ledger_readiness_line("History", "not checked; health failed");
            print_ledger_readiness_line("Export", "not checked; health failed");
            print_ledger_readiness_line("Prune dry-run", "not checked; health failed");
            print_ledger_next(&[
                "Keep local receipts and retry ledger checks after the server is healthy.",
            ]);
            print_ledger_do_not();
            return Ok(());
        }
    }

    let Some(project) = project else {
        print_ledger_readiness_line("History", "not checked; project missing");
        print_ledger_readiness_line("Export", "not checked; project missing");
        print_ledger_readiness_line("Prune dry-run", "not checked; project missing");
        print_ledger_next(&[
            "Set `--project`, `PERFGATE_PROJECT`, or `[baseline_server].project`.",
        ]);
        print_ledger_do_not();
        return Ok(());
    };

    let history_result = with_tokio_runtime(async {
        client
            .list_decisions(project, &ListDecisionsQuery::new().with_limit(1))
            .await
            .map_err(anyhow::Error::from)
    });
    match history_result {
        Ok(response) => {
            print_ledger_readiness_line(
                "History",
                format!(
                    "reachable ({} decision record(s) visible)",
                    response.pagination.total
                ),
            );
            print_ledger_readiness_line("Export", "available through decision export");
        }
        Err(error) => {
            print_ledger_readiness_line("History", format!("not reachable: {error:#}"));
            print_ledger_readiness_line("Export", "not available until history is reachable");
        }
    }

    let prune_result = with_tokio_runtime(async {
        client
            .prune_decisions(
                project,
                &PruneDecisionsRequest {
                    older_than: chrono::Utc::now(),
                    dry_run: true,
                },
            )
            .await
            .map_err(anyhow::Error::from)
    });
    match prune_result {
        Ok(response) => {
            print_ledger_readiness_line(
                "Prune dry-run",
                format!(
                    "available ({} record(s) matched; dry-run deleted {})",
                    response.matched, response.deleted
                ),
            );
        }
        Err(error) => {
            print_ledger_readiness_line("Prune dry-run", format!("not available: {error:#}"));
        }
    }

    print_ledger_next(&[
        "Use `perfgate decision upload` only after local decision receipts are reviewed.",
        "Use `perfgate decision history`, `export`, and `prune --dry-run` for team operations.",
    ]);
    print_ledger_do_not();
    Ok(())
}

fn ledger_doctor_client(server_config: &ResolvedServerConfig) -> anyhow::Result<BaselineClient> {
    let url = server_config
        .url
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!(BASELINE_SERVER_NOT_CONFIGURED))?;
    let mut client_config = ClientConfig::new(url)
        .with_timeout(Duration::from_secs(2))
        .with_retry(RetryConfig::new().with_max_retries(0));
    if let Some(api_key) = &server_config.api_key {
        client_config = client_config.with_api_key(api_key);
    }
    BaselineClient::new(client_config)
        .with_context(|| format!("create ledger doctor client for {url}"))
}

fn print_ledger_readiness_line(label: &str, value: impl AsRef<str>) {
    println!("{label}: {}", value.as_ref());
}

fn print_ledger_next(lines: &[&str]) {
    println!();
    println!("Next:");
    for line in lines {
        println!("  {line}");
    }
}

fn print_ledger_do_not() {
    println!();
    println!("Do not:");
    println!("  make the server ledger part of local correctness.");
    println!("  rerun benchmarks only to repair optional ledger uploads.");
}
