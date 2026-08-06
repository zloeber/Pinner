use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use tempfile::tempdir;

fn mise_complex_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/mise-complex")
}

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn audit_json_reports_floating_mise_tool() {
    let _guard = env_lock().lock().unwrap();
    let dir = tempdir().unwrap();
    std::fs::write(
        dir.path().join(".mise.toml"),
        "[tools]\nnode = \"latest\"\n",
    )
    .unwrap();
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
fn audit_json_stdout_is_findings_only() {
    let _guard = env_lock().lock().unwrap();
    let dir = tempdir().unwrap();
    std::fs::write(
        dir.path().join(".mise.toml"),
        "[tools]\nnode = \"latest\"\n",
    )
    .unwrap();
    // SAFETY: test-only resolve seam; serialized via env_lock.
    unsafe {
        std::env::set_var("PINNER_MISE_RESOLVE_MAP", "node=22.11.0");
    }
    let output = Command::cargo_bin("pinner")
        .unwrap()
        .current_dir(dir.path())
        .args(["audit", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.trim_start().starts_with('{'));
    assert!(!stdout.contains("pinner audit ·"));
    unsafe {
        std::env::remove_var("PINNER_MISE_RESOLVE_MAP");
    }
}

#[test]
fn audit_json_mise_complex_reports_backends_and_tables() {
    let fixture = mise_complex_fixture();
    Command::cargo_bin("pinner")
        .unwrap()
        .current_dir(&fixture)
        .args(["audit", "--format", "json", "--ecosystem", "mise"])
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains("\"name\":\"awscli\""))
        .stdout(predicate::str::contains("\"requested\":\"latest\""))
        .stdout(predicate::str::contains("\"name\":\"npm:skills\""))
        .stdout(predicate::str::contains(
            "\"name\":\"github:zloeber/pinner\"",
        ))
        .stdout(predicate::str::contains("\"name\":\"yamllint\"").not());
}

#[test]
fn explain_after_pin_shows_evidence() {
    let _guard = env_lock().lock().unwrap();
    let dir = tempdir().unwrap();
    std::fs::write(
        dir.path().join(".mise.toml"),
        "[tools]\nnode = \"latest\"\n",
    )
    .unwrap();
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
