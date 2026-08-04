use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;

#[test]
fn walkthrough_with_agent_exits_2() {
    Command::cargo_bin("pinner")
        .unwrap()
        .args(["--walkthrough", "--agent", "pin"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains(
            "walkthrough requires an interactive TTY (not --agent/--format json)",
        ));
}

#[test]
fn walkthrough_with_format_json_exits_2() {
    Command::cargo_bin("pinner")
        .unwrap()
        .args(["--walkthrough", "--format", "json", "pin"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains(
            "walkthrough requires an interactive TTY",
        ));
}
