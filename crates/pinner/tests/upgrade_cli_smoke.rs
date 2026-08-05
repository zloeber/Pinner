use std::process::Command;

#[test]
fn upgrade_help_lists_subcommand() {
    let output = Command::new(env!("CARGO_BIN_EXE_pinner"))
        .arg("upgrade")
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
}

#[test]
fn upgrade_walkthrough_with_agent_exits_2() {
    let output = Command::new(env!("CARGO_BIN_EXE_pinner"))
        .args(["--walkthrough", "--agent", "upgrade"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}
