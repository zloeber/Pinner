use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use tempfile::tempdir;

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn audit_json_reports_floating_mise_tool() {
    let _guard = env_lock().lock().unwrap();
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join(".mise.toml"), "[tools]\nnode = \"latest\"\n").unwrap();
    // SAFETY: test-only resolve seam; serialized via env_lock.
    unsafe {
        std::env::set_var("PINNER_MISE_RESOLVE_MAP", "node=22.11.0");
    }
    Command::cargo_bin("pinner")
        .unwrap()
        .current_dir(dir.path())
        .args(["audit", "--format", "json"])
        .assert()
        .failure() // exit 1 when findings exist
        .code(1)
        .stdout(predicate::str::contains("\"name\":\"node\""));
    unsafe {
        std::env::remove_var("PINNER_MISE_RESOLVE_MAP");
    }
}

#[test]
fn explain_after_pin_shows_evidence() {
    let _guard = env_lock().lock().unwrap();
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join(".mise.toml"), "[tools]\nnode = \"latest\"\n").unwrap();
    // SAFETY: test-only resolve seam; serialized via env_lock.
    unsafe {
        std::env::set_var("PINNER_MISE_RESOLVE_MAP", "node=22.11.0");
    }
    Command::cargo_bin("pinner")
        .unwrap()
        .current_dir(dir.path())
        .args(["pin"])
        .assert()
        .success();
    Command::cargo_bin("pinner")
        .unwrap()
        .current_dir(dir.path())
        .args(["explain", "node"])
        .assert()
        .success()
        .stdout(predicate::str::contains("22.11.0"));
    unsafe {
        std::env::remove_var("PINNER_MISE_RESOLVE_MAP");
    }
}
