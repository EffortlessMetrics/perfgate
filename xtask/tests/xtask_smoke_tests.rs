use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn repo_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    dir.push(format!("{}_{}_{}", prefix, std::process::id(), nanos));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn xtask_schema_command_runs() {
    let out_dir = unique_temp_dir("perfgate_xtask_schema");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_xtask"));
    if let Ok(profile) = env::var("LLVM_PROFILE_FILE") {
        cmd.env("LLVM_PROFILE_FILE", profile);
    }
    cmd.current_dir(repo_root());
    let status = cmd
        .arg("schema")
        .arg("--out-dir")
        .arg(&out_dir)
        .status()
        .expect("run xtask schema");
    assert!(status.success());
    assert!(out_dir.join("perfgate.run.v1.schema.json").exists());

    let _ = fs::remove_dir_all(&out_dir);
}

#[test]
fn xtask_mutants_propagates_exit_code() {
    let fake_dir = unique_temp_dir("perfgate_fake_cargo");

    #[cfg(windows)]
    let fake_cargo = {
        let script = fake_dir.join("cargo.cmd");
        fs::write(&script, "@echo off\r\nexit /b 2\r\n").expect("write fake cargo");
        script
    };

    #[cfg(unix)]
    let fake_cargo = {
        use std::os::unix::fs::PermissionsExt;
        let script = fake_dir.join("cargo");
        fs::write(&script, "#!/bin/sh\nexit 2\n").expect("write fake cargo");
        let mut perms = fs::metadata(&script).expect("stat cargo").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).expect("chmod cargo");
        script
    };

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_xtask"));
    if let Ok(profile) = env::var("LLVM_PROFILE_FILE") {
        cmd.env("LLVM_PROFILE_FILE", profile);
    }
    let status = cmd
        .current_dir(repo_root())
        .env("CARGO", &fake_cargo)
        .arg("mutants")
        .status()
        .expect("run xtask mutants");

    assert_eq!(status.code(), Some(2));

    let _ = fs::remove_dir_all(&fake_dir);
}
