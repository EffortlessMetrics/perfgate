/// Redacts sensitive values in command arguments for diagnostic artifacts.
///
/// Redaction rules are intentionally simple and deterministic:
/// - `--token <value>` / `--password <value>` style pairs redact the following value.
/// - `--token=<value>` / `--password=<value>` inline assignments redact the value.
/// - `KEY=VALUE` entries where `KEY` looks secret are redacted.
pub fn redact_command_for_diagnostics(command: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(command.len());
    let mut redact_next = false;

    for arg in command {
        if redact_next {
            out.push("[REDACTED]".to_string());
            redact_next = false;
            continue;
        }

        if let Some((k, _)) = arg.split_once('=')
            && is_secret_key(k)
        {
            out.push(format!("{k}=[REDACTED]"));
            continue;
        }

        if let Some((flag, _value)) = arg.split_once('=')
            && flag.starts_with("--")
            && is_secret_key(flag.trim_start_matches('-'))
        {
            out.push(format!("{flag}=[REDACTED]"));
            continue;
        }

        if arg.starts_with("--") && is_secret_key(arg.trim_start_matches('-')) {
            out.push(arg.clone());
            redact_next = true;
            continue;
        }

        out.push(arg.clone());
    }

    out
}

fn is_secret_key(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    k.contains("token")
        || k.contains("password")
        || k.contains("secret")
        || (k.contains("api") && k.contains("key"))
}

#[cfg(test)]
mod tests {
    use super::redact_command_for_diagnostics;

    #[test]
    fn redacts_sensitive_command_values() {
        let redacted = redact_command_for_diagnostics(&[
            "bench".to_string(),
            "--token".to_string(),
            "<value>".to_string(),
            "--api-key=<value>".to_string(),
            "PASSWORD=<value>".to_string(),
        ]);

        assert_eq!(
            redacted,
            vec![
                "bench".to_string(),
                "--token".to_string(),
                "[REDACTED]".to_string(),
                "--api-key=[REDACTED]".to_string(),
                "PASSWORD=[REDACTED]".to_string(),
            ]
        );
    }
}
