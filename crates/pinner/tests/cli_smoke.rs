use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;

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
