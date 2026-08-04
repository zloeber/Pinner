use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn pin_help_lists_commands() {
    Command::cargo_bin("pinner")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("pin"))
        .stdout(predicate::str::contains("check"))
        .stdout(predicate::str::contains("toolchain"));
}

#[test]
fn version_is_non_empty_semverish() {
    // PINNER_VERSION comes from build.rs (git describe v* tag, else Cargo.toml).
    Command::cargo_bin("pinner")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::is_match(r"^pinner \d+\.\d+\.\d+").unwrap());
}

#[test]
fn audit_accepts_terraform_ecosystem_flag() {
    let dir = tempdir().unwrap();
    Command::cargo_bin("pinner")
        .unwrap()
        .current_dir(dir.path())
        .args(["audit", "--ecosystem", "terraform"])
        .assert()
        .success()
        .stderr(predicate::str::contains("unknown ecosystem").not());
}
